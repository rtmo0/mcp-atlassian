use axum::Json;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use subtle::ConstantTimeEq;

use crate::api::AppState;

/// Header 401: MCP clients use Bearer, agents may use X-Api-Key.
pub const CHALLENGE: &str = "Bearer realm=\"atlassian-mcp\"";

#[derive(Debug)]
pub struct Unauthorized;

impl IntoResponse for Unauthorized {
    fn into_response(self) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            [(WWW_AUTHENTICATE, CHALLENGE)],
            Json(json!({ "error": "unauthorized: Bearer or X-Api-Key required" })),
        )
            .into_response()
    }
}

pub async fn require_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, Unauthorized> {
    let Some(expected) = state.http.api_token.as_deref() else {
        return Ok(next.run(req).await);
    };
    if header_token(req.headers()).is_some_and(|token| tokens_eq(token, expected)) {
        return Ok(next.run(req).await);
    }
    Err(Unauthorized)
}

fn header_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .or_else(|| {
            let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
            let (scheme, token) = value.split_once(' ')?;
            scheme.eq_ignore_ascii_case("bearer").then_some(token)
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn tokens_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(name: &'static str, value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(name, HeaderValue::from_static(value));
        headers
    }

    #[test]
    fn accepts_api_key_header() {
        let headers = headers_with("x-api-key", "s3cret");
        assert_eq!(header_token(&headers), Some("s3cret"));
    }

    #[test]
    fn accepts_bearer_token() {
        let headers = headers_with(AUTHORIZATION.as_str(), "Bearer s3cret");
        assert_eq!(header_token(&headers), Some("s3cret"));
    }

    #[test]
    fn rejects_other_auth_schemes() {
        let headers = headers_with(AUTHORIZATION.as_str(), "Basic c2VjcmV0");
        assert_eq!(header_token(&headers), None);
    }

    #[test]
    fn accepts_bearer_case_insensitively() {
        let headers = headers_with(AUTHORIZATION.as_str(), "bearer s3cret");
        assert_eq!(header_token(&headers), Some("s3cret"));
    }

    #[test]
    fn compares_tokens_fully() {
        assert!(tokens_eq("secret", "secret"));
        assert!(!tokens_eq("secret", "secreT"));
        assert!(!tokens_eq("secret", "secret2"));
    }
}
