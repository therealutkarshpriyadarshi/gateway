/// Lua script for token bucket rate limiting in Redis
///
/// This script implements the token bucket algorithm atomically in Redis.
///
/// KEYS[1] = the rate limit key
/// ARGV[1] = maximum tokens (capacity)
/// ARGV[2] = refill rate (tokens per second)
/// ARGV[3] = current timestamp (seconds)
/// ARGV[4] = window duration (seconds)
///
/// Returns: [allowed (0/1), remaining tokens, reset_after]
pub const TOKEN_BUCKET_SCRIPT: &str = r#"
local key = KEYS[1]
local max_tokens = tonumber(ARGV[1])
local refill_rate = tonumber(ARGV[2])
local now = tonumber(ARGV[3])
local window = tonumber(ARGV[4])

-- Get current state
local state = redis.call('HMGET', key, 'tokens', 'last_refill')
local tokens = tonumber(state[1])
local last_refill = tonumber(state[2])

-- Initialize if this is the first request
if tokens == nil then
    tokens = max_tokens
    last_refill = now
end

-- Calculate token refill
local time_passed = math.max(0, now - last_refill)
local tokens_to_add = time_passed * refill_rate
tokens = math.min(max_tokens, tokens + tokens_to_add)

-- Check if we can allow the request
local allowed = 0
local remaining = tokens
local reset_after = window

if tokens >= 1 then
    tokens = tokens - 1
    allowed = 1
    remaining = tokens
else
    -- Calculate when the next token will be available
    reset_after = math.ceil((1 - tokens) / refill_rate)
end

-- Update state
redis.call('HMSET', key, 'tokens', tokens, 'last_refill', now)
redis.call('EXPIRE', key, window * 2)

return {allowed, math.floor(remaining), reset_after}
"#;

/// Lua script for sliding window rate limiting
///
/// This script implements a sliding window counter algorithm in Redis.
///
/// KEYS[1] = the rate limit key
/// ARGV[1] = maximum requests
/// ARGV[2] = window duration (seconds)
/// ARGV[3] = current timestamp (seconds)
///
/// Returns: [allowed (0/1), remaining requests, reset_after]
pub const SLIDING_WINDOW_SCRIPT: &str = r#"
local key = KEYS[1]
local max_requests = tonumber(ARGV[1])
local window = tonumber(ARGV[2])
local now = tonumber(ARGV[3])

-- Remove old entries outside the window
local window_start = now - window
redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)

-- Count requests in current window
local current_count = redis.call('ZCARD', key)

local allowed = 0
local remaining = max_requests - current_count
local reset_after = window

if current_count < max_requests then
    -- Add current request
    redis.call('ZADD', key, now, now .. ':' .. math.random())
    redis.call('EXPIRE', key, window * 2)
    allowed = 1
    remaining = remaining - 1
else
    -- Calculate when the oldest request will expire
    local oldest = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
    if oldest[2] then
        reset_after = math.ceil(tonumber(oldest[2]) + window - now)
    end
end

return {allowed, math.max(0, remaining), math.max(1, reset_after)}
"#;

/// Lua script for fixed window rate limiting
///
/// This is a simpler implementation using fixed time windows.
///
/// KEYS[1] = the rate limit key
/// ARGV[1] = maximum requests
/// ARGV[2] = window duration (seconds)
///
/// Returns: [allowed (0/1), remaining requests, reset_after]
pub const FIXED_WINDOW_SCRIPT: &str = r#"
local key = KEYS[1]
local max_requests = tonumber(ARGV[1])
local window = tonumber(ARGV[2])

-- Increment counter
local current = redis.call('INCR', key)

-- Set expiry on first request
if current == 1 then
    redis.call('EXPIRE', key, window)
end

-- Get TTL for reset time
local ttl = redis.call('TTL', key)
if ttl == -1 then
    -- No expiry set, set it now
    redis.call('EXPIRE', key, window)
    ttl = window
end

local allowed = 0
local remaining = max_requests - current

if current <= max_requests then
    allowed = 1
    remaining = remaining
else
    remaining = 0
end

return {allowed, math.max(0, remaining), math.max(1, ttl)}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scripts_are_valid() {
        // Just verify the scripts are not empty and contain expected keywords
        assert!(TOKEN_BUCKET_SCRIPT.contains("HMGET"));
        assert!(TOKEN_BUCKET_SCRIPT.contains("tokens"));
        assert!(TOKEN_BUCKET_SCRIPT.contains("refill_rate"));

        assert!(SLIDING_WINDOW_SCRIPT.contains("ZREMRANGEBYSCORE"));
        assert!(SLIDING_WINDOW_SCRIPT.contains("ZADD"));

        assert!(FIXED_WINDOW_SCRIPT.contains("INCR"));
        assert!(FIXED_WINDOW_SCRIPT.contains("EXPIRE"));
    }
}
