use crate::auth::AuthResult;

/// Extension type to hold authentication result (for future use in request handlers)
#[derive(Clone, Debug)]
pub struct AuthExtension {
    pub auth_result: Option<AuthResult>,
}

// Middleware functions removed - authentication is now handled directly in proxy_handler
// This module is kept for the AuthExtension type which may be useful for future enhancements
