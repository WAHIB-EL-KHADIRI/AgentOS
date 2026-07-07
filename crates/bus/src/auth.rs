//! Shared bearer-token authorization for AgentOS network surfaces
//! (HTTP health/API, gRPC-over-HTTP bus endpoints, SSE event stream).
//!
//! The token is opt-in: when `AGENTOS_API_TOKEN` is unset, surfaces stay
//! open (the runtime binds to localhost by default). When set, protected
//! endpoints require `Authorization: Bearer <token>`; the SSE endpoint
//! additionally accepts `?token=<token>` because `EventSource` cannot set
//! request headers.

/// Optional shared API token loaded from the environment.
#[derive(Debug, Clone)]
pub struct ApiToken {
    token: Option<String>,
}

impl ApiToken {
    pub fn new(token: Option<String>) -> Self {
        let token = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        Self { token }
    }

    /// Load from `AGENTOS_API_TOKEN`. Absent or empty means auth disabled.
    pub fn from_env() -> Self {
        Self::new(std::env::var("AGENTOS_API_TOKEN").ok())
    }

    /// Whether requests must present the token.
    pub fn required(&self) -> bool {
        self.token.is_some()
    }

    /// Authorize from an `Authorization` header value.
    /// Accepts only the `Bearer <token>` scheme.
    pub fn authorize_header(&self, header: Option<&str>) -> bool {
        let Some(expected) = &self.token else {
            return true;
        };
        let Some(header) = header else {
            return false;
        };
        match header.strip_prefix("Bearer ") {
            Some(candidate) => constant_time_eq(candidate.trim(), expected),
            None => false,
        }
    }

    /// Authorize from either an `Authorization` header or a `token` query
    /// parameter (SSE: browsers' `EventSource` cannot set headers).
    pub fn authorize_header_or_query(&self, header: Option<&str>, query: Option<&str>) -> bool {
        if self.token.is_none() {
            return true;
        }
        if self.authorize_header(header) {
            return true;
        }
        let Some(expected) = &self.token else {
            return true;
        };
        let Some(query) = query else {
            return false;
        };
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .any(|(key, value)| key == "token" && constant_time_eq(value, expected))
    }
}

/// Constant-time string comparison: the runtime does not depend on input
/// length or content beyond a single accumulated difference bit.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut diff = a.len() ^ b.len();
    let max = a.len().max(b.len());
    for i in 0..max {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= (x ^ y) as usize;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_token_allows_everything() {
        let token = ApiToken::new(None);
        assert!(!token.required());
        assert!(token.authorize_header(None));
        assert!(token.authorize_header(Some("Bearer whatever")));
        assert!(token.authorize_header_or_query(None, None));

        let empty = ApiToken::new(Some("   ".into()));
        assert!(!empty.required());
        assert!(empty.authorize_header(None));
    }

    #[test]
    fn test_bearer_header_authorization() {
        let token = ApiToken::new(Some("s3cret".into()));
        assert!(token.required());
        assert!(token.authorize_header(Some("Bearer s3cret")));
        assert!(!token.authorize_header(Some("Bearer wrong")));
        assert!(!token.authorize_header(Some("s3cret")));
        assert!(!token.authorize_header(Some("Basic s3cret")));
        assert!(!token.authorize_header(None));
    }

    #[test]
    fn test_query_token_authorization() {
        let token = ApiToken::new(Some("s3cret".into()));
        assert!(token.authorize_header_or_query(None, Some("token=s3cret")));
        assert!(token.authorize_header_or_query(None, Some("a=b&token=s3cret&c=d")));
        assert!(!token.authorize_header_or_query(None, Some("token=wrong")));
        assert!(!token.authorize_header_or_query(None, Some("nottoken=s3cret")));
        assert!(!token.authorize_header_or_query(None, None));
        // Header still works on the query-capable path.
        assert!(token.authorize_header_or_query(Some("Bearer s3cret"), None));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
    }
}
