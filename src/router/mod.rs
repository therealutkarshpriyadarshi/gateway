use crate::config::RouteConfig;
use crate::error::{GatewayError, Result};
use http::Method;
use matchit::Router as MatchitRouter;
use std::collections::HashMap;

/// Route information
#[derive(Debug, Clone)]
pub struct Route {
    /// Backend service URL
    pub backend: String,
    /// Allowed HTTP methods (empty means all methods allowed)
    pub methods: Vec<Method>,
    /// Whether to strip the prefix when forwarding
    pub strip_prefix: bool,
    /// Route description
    pub description: String,
}

/// Gateway router for matching incoming requests to backend services
#[derive(Debug, Clone)]
pub struct Router {
    /// Path-based router using matchit
    matcher: MatchitRouter<Route>,
}

impl Router {
    /// Create a new router from route configurations
    pub fn new(routes: Vec<RouteConfig>) -> Result<Self> {
        let mut matcher = MatchitRouter::new();

        for route_config in routes {
            let methods = if route_config.methods.is_empty() {
                vec![]
            } else {
                route_config
                    .methods
                    .iter()
                    .map(|m| {
                        Method::from_bytes(m.to_uppercase().as_bytes())
                            .map_err(|_| GatewayError::InvalidMethod(m.clone()))
                    })
                    .collect::<Result<Vec<_>>>()?
            };

            let route = Route {
                backend: route_config.backend,
                methods,
                strip_prefix: route_config.strip_prefix,
                description: route_config.description,
            };

            // Convert path syntax from :param to {param} and *path to {*path}
            let matchit_path = convert_path_syntax(&route_config.path);

            matcher
                .insert(&matchit_path, route)
                .map_err(|e| GatewayError::InvalidRoute(format!("Failed to insert route: {}", e)))?;
        }

        Ok(Self { matcher })
    }

    /// Match a request path and method to a route
    pub fn match_route(&self, path: &str, method: &Method) -> Result<RouteMatch> {
        let matched = self
            .matcher
            .at(path)
            .map_err(|_| GatewayError::RouteNotFound(path.to_string()))?;

        let route = matched.value;

        // Check if method is allowed (empty methods means all methods are allowed)
        if !route.methods.is_empty() && !route.methods.contains(method) {
            return Err(GatewayError::InvalidMethod(format!(
                "Method {} not allowed for path {}",
                method, path
            )));
        }

        // Extract path parameters
        let params: HashMap<String, String> = matched
            .params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        Ok(RouteMatch {
            route: route.clone(),
            params,
            matched_path: path.to_string(),
        })
    }

    /// Get all routes in the router
    pub fn routes(&self) -> Vec<String> {
        // This is a simple implementation that doesn't expose internal matchit structure
        // In a real implementation, you might want to store route paths separately
        vec![]
    }
}

/// Result of matching a route
#[derive(Debug, Clone)]
pub struct RouteMatch {
    /// The matched route
    pub route: Route,
    /// Path parameters extracted from the URL
    pub params: HashMap<String, String>,
    /// The original matched path
    pub matched_path: String,
}

/// Convert path syntax from Express-style (:param, *path) to matchit syntax ({param}, {*path})
fn convert_path_syntax(path: &str) -> String {
    let mut result = String::new();
    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // Convert :param to {param}
            ':' => {
                result.push('{');
                while let Some(&next_ch) = chars.peek() {
                    if next_ch.is_alphanumeric() || next_ch == '_' {
                        result.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                result.push('}');
            }
            // Convert *path to {*path}
            '*' => {
                result.push_str("{*");
                while let Some(&next_ch) = chars.peek() {
                    if next_ch.is_alphanumeric() || next_ch == '_' {
                        result.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                result.push('}');
            }
            // Copy other characters as-is
            _ => result.push(ch),
        }
    }

    result
}

impl RouteMatch {
    /// Build the backend URL with path parameters substituted
    pub fn build_backend_url(&self, original_path: &str) -> String {
        if self.route.strip_prefix {
            // If strip_prefix is true, we need to remove the matched portion
            // and append the remaining path to the backend
            let remaining = original_path
                .strip_prefix(&self.matched_path)
                .unwrap_or(original_path);
            format!("{}{}", self.route.backend.trim_end_matches('/'), remaining)
        } else {
            // Otherwise, just append the full path
            format!(
                "{}{}",
                self.route.backend.trim_end_matches('/'),
                original_path
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_routes() -> Vec<RouteConfig> {
        vec![
            RouteConfig {
                path: "/api/users".to_string(),
                backend: "http://localhost:3000".to_string(),
                methods: vec!["GET".to_string(), "POST".to_string()],
                strip_prefix: false,
                description: "User service".to_string(),
            },
            RouteConfig {
                path: "/api/orders/:id".to_string(),
                backend: "http://localhost:3001".to_string(),
                methods: vec![],
                strip_prefix: false,
                description: "Order service".to_string(),
            },
            RouteConfig {
                path: "/v1/products/*path".to_string(),
                backend: "http://localhost:3002".to_string(),
                methods: vec!["GET".to_string()],
                strip_prefix: true,
                description: "Product service".to_string(),
            },
        ]
    }

    #[test]
    fn test_router_creation() {
        let routes = create_test_routes();
        let _router = Router::new(routes).unwrap();
        // Router created successfully - just testing that creation doesn't panic
    }

    #[test]
    fn test_exact_match() {
        let routes = create_test_routes();
        let router = Router::new(routes).unwrap();

        let result = router.match_route("/api/users", &Method::GET);
        assert!(result.is_ok());

        let route_match = result.unwrap();
        assert_eq!(route_match.route.backend, "http://localhost:3000");
        assert!(route_match.params.is_empty());
    }

    #[test]
    fn test_param_match() {
        let routes = create_test_routes();
        let router = Router::new(routes).unwrap();

        let result = router.match_route("/api/orders/123", &Method::GET);
        assert!(result.is_ok());

        let route_match = result.unwrap();
        assert_eq!(route_match.route.backend, "http://localhost:3001");
        assert_eq!(route_match.params.get("id").unwrap(), "123");
    }

    #[test]
    fn test_wildcard_match() {
        let routes = create_test_routes();
        let router = Router::new(routes).unwrap();

        let result = router.match_route("/v1/products/electronics/phones", &Method::GET);
        assert!(result.is_ok());

        let route_match = result.unwrap();
        assert_eq!(route_match.route.backend, "http://localhost:3002");
    }

    #[test]
    fn test_method_validation() {
        let routes = create_test_routes();
        let router = Router::new(routes).unwrap();

        // GET is allowed for /api/users
        assert!(router.match_route("/api/users", &Method::GET).is_ok());

        // POST is allowed for /api/users
        assert!(router.match_route("/api/users", &Method::POST).is_ok());

        // DELETE is not allowed for /api/users
        assert!(router.match_route("/api/users", &Method::DELETE).is_err());
    }

    #[test]
    fn test_route_not_found() {
        let routes = create_test_routes();
        let router = Router::new(routes).unwrap();

        let result = router.match_route("/nonexistent", &Method::GET);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_backend_url_no_strip() {
        let route_match = RouteMatch {
            route: Route {
                backend: "http://localhost:3000".to_string(),
                methods: vec![],
                strip_prefix: false,
                description: "".to_string(),
            },
            params: HashMap::new(),
            matched_path: "/api/users".to_string(),
        };

        let url = route_match.build_backend_url("/api/users/123");
        assert_eq!(url, "http://localhost:3000/api/users/123");
    }

    #[test]
    fn test_build_backend_url_with_strip() {
        let route_match = RouteMatch {
            route: Route {
                backend: "http://localhost:3000".to_string(),
                methods: vec![],
                strip_prefix: true,
                description: "".to_string(),
            },
            params: HashMap::new(),
            matched_path: "/v1/products".to_string(),
        };

        let url = route_match.build_backend_url("/v1/products/electronics");
        assert_eq!(url, "http://localhost:3000/electronics");
    }

    #[test]
    fn test_empty_methods_allows_all() {
        let routes = vec![RouteConfig {
            path: "/api/test".to_string(),
            backend: "http://localhost:3000".to_string(),
            methods: vec![], // Empty means all methods allowed
            strip_prefix: false,
            description: "".to_string(),
        }];

        let router = Router::new(routes).unwrap();

        // All methods should be allowed
        assert!(router.match_route("/api/test", &Method::GET).is_ok());
        assert!(router.match_route("/api/test", &Method::POST).is_ok());
        assert!(router.match_route("/api/test", &Method::DELETE).is_ok());
        assert!(router.match_route("/api/test", &Method::PUT).is_ok());
    }

    #[test]
    fn test_convert_path_syntax() {
        assert_eq!(convert_path_syntax("/api/users"), "/api/users");
        assert_eq!(convert_path_syntax("/api/users/:id"), "/api/users/{id}");
        assert_eq!(
            convert_path_syntax("/api/users/:id/posts/:postId"),
            "/api/users/{id}/posts/{postId}"
        );
        assert_eq!(convert_path_syntax("/api/*path"), "/api/{*path}");
        assert_eq!(
            convert_path_syntax("/v1/products/*remaining"),
            "/v1/products/{*remaining}"
        );
    }
}
