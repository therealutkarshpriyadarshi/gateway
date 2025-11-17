use super::types::RetryConfig;
use backoff::{backoff::Backoff, ExponentialBackoff, ExponentialBackoffBuilder};
use tracing::{debug, warn};

/// Retry executor with exponential backoff
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    /// Create a new retry executor
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Execute a function with retries
    pub async fn execute<F, Fut, T, E>(&self, mut f: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut backoff = self.create_backoff();
        let mut attempt = 0;

        loop {
            attempt += 1;
            debug!(
                attempt,
                max_retries = self.config.max_retries,
                "Executing request"
            );

            match f().await {
                Ok(result) => {
                    if attempt > 1 {
                        debug!(attempt, "Request succeeded after retries");
                    }
                    return Ok(result);
                }
                Err(e) => {
                    if attempt > self.config.max_retries {
                        warn!(
                            attempt,
                            max_retries = self.config.max_retries,
                            error = %e,
                            "Request failed after max retries"
                        );
                        return Err(e);
                    }

                    if let Some(wait) = backoff.next_backoff() {
                        debug!(
                            attempt,
                            wait_ms = wait.as_millis(),
                            error = %e,
                            "Request failed, retrying after backoff"
                        );
                        tokio::time::sleep(wait).await;
                    } else {
                        // Backoff exhausted
                        warn!(attempt, error = %e, "Backoff exhausted");
                        return Err(e);
                    }
                }
            }
        }
    }

    /// Execute with retries, but only if error matches predicate
    pub async fn execute_with_predicate<F, Fut, T, E, P>(
        &self,
        mut f: F,
        should_retry: P,
    ) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
        P: Fn(&E) -> bool,
    {
        let mut backoff = self.create_backoff();
        let mut attempt = 0;

        loop {
            attempt += 1;
            debug!(
                attempt,
                max_retries = self.config.max_retries,
                "Executing request"
            );

            match f().await {
                Ok(result) => {
                    if attempt > 1 {
                        debug!(attempt, "Request succeeded after retries");
                    }
                    return Ok(result);
                }
                Err(e) => {
                    if !should_retry(&e) {
                        debug!(attempt, error = %e, "Error not retryable");
                        return Err(e);
                    }

                    if attempt > self.config.max_retries {
                        warn!(
                            attempt,
                            max_retries = self.config.max_retries,
                            error = %e,
                            "Request failed after max retries"
                        );
                        return Err(e);
                    }

                    if let Some(wait) = backoff.next_backoff() {
                        debug!(
                            attempt,
                            wait_ms = wait.as_millis(),
                            error = %e,
                            "Request failed, retrying after backoff"
                        );
                        tokio::time::sleep(wait).await;
                    } else {
                        warn!(attempt, error = %e, "Backoff exhausted");
                        return Err(e);
                    }
                }
            }
        }
    }

    fn create_backoff(&self) -> ExponentialBackoff {
        ExponentialBackoffBuilder::new()
            .with_initial_interval(self.config.initial_backoff())
            .with_max_interval(self.config.max_backoff())
            .with_multiplier(self.config.backoff_multiplier)
            .with_max_elapsed_time(None) // We handle max retries manually
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_retry_succeeds_immediately() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 10,
            max_backoff_ms: 100,
            backoff_multiplier: 2.0,
        };
        let executor = RetryExecutor::new(config);

        let result = executor
            .execute(|| async { Ok::<_, String>("success") })
            .await;

        assert_eq!(result, Ok("success"));
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 10,
            max_backoff_ms: 100,
            backoff_multiplier: 2.0,
        };
        let executor = RetryExecutor::new(config);

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = executor
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    let current = attempts.fetch_add(1, Ordering::SeqCst);
                    if current < 2 {
                        Err("failed".to_string())
                    } else {
                        Ok("success")
                    }
                }
            })
            .await;

        assert_eq!(result, Ok("success"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_attempts() {
        let config = RetryConfig {
            max_retries: 2,
            initial_backoff_ms: 10,
            max_backoff_ms: 100,
            backoff_multiplier: 2.0,
        };
        let executor = RetryExecutor::new(config);

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = executor
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<String, _>("always fails".to_string())
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // Initial + 2 retries
    }

    #[tokio::test]
    async fn test_retry_with_predicate() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 10,
            max_backoff_ms: 100,
            backoff_multiplier: 2.0,
        };
        let executor = RetryExecutor::new(config);

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        // Should not retry on "permanent" error
        let result = executor
            .execute_with_predicate(
                || {
                    let attempts = attempts_clone.clone();
                    async move {
                        attempts.fetch_add(1, Ordering::SeqCst);
                        Err::<String, _>("permanent")
                    }
                },
                |e| *e != "permanent",
            )
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1); // No retries
    }

    #[tokio::test]
    async fn test_exponential_backoff_timing() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 50,
            max_backoff_ms: 500,
            backoff_multiplier: 2.0,
        };
        let executor = RetryExecutor::new(config);

        let start = std::time::Instant::now();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let _ = executor
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<String, _>("fail")
                }
            })
            .await;

        let elapsed = start.elapsed();

        // Should have waited roughly: 50ms + 100ms + 200ms = 350ms
        // Allow some tolerance for execution overhead
        assert!(elapsed >= Duration::from_millis(300));
        assert!(elapsed < Duration::from_millis(600));
    }
}
