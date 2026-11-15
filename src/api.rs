use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::json;

use crate::auth;
use crate::config::{Config, HttpConfig, build_version};
use crate::metrics::Metrics;
use crate::providers::Providers;
use crate::server::AtlassianMcpServer;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub http: Arc<HttpConfig>,
    pub providers: Providers,
    pub tool_count: usize,
}

pub async fn serve(cfg: Config, http: HttpConfig) -> anyhow::Result<()> {
    let listen = http.listen.clone();
    let port = http.port;
    let providers = Providers::from_config(&cfg)?;
    if providers.is_empty() {
        tracing::warn!("no Atlassian products configured: /healthz is up, /readyz returns 503");
    }

    let metrics = Arc::new(Metrics::new(
        &build_version(),
        providers.jira.is_some(),
        providers.confluence.is_some(),
    )?);
    let metrics_port = http.metrics_port;
    let metrics_listen = http.metrics_listen.clone();
    let tool_count = AtlassianMcpServer::new(providers.clone(), metrics.clone()).tool_count();
    let state = AppState {
        cfg: Arc::new(cfg),
        http: Arc::new(http),
        providers,
        tool_count,
    };

    let factory_providers = state.providers.clone();
    let factory_metrics = metrics.clone();
    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(AtlassianMcpServer::new(
                factory_providers.clone(),
                factory_metrics.clone(),
            ))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            // Stateless Streamable HTTP: no Mcp-Session-Id and no server state.
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_allowed_hosts(state.http.allowed_hosts.clone()),
    );

    let protected = Router::new().nest_service("/mcp", mcp_service).route_layer(
        middleware::from_fn_with_state(state.clone(), auth::require_token),
    );

    let public = Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .route("/api/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/readyz", get(readyz));

    let app = public.merge(protected).with_state(state);

    if metrics_port != 0 {
        let metrics_addr = format!("{metrics_listen}:{metrics_port}");
        let metrics_app = Router::new()
            .route("/metrics", get(metrics_endpoint))
            .with_state(metrics);
        let metrics_listener = tokio::net::TcpListener::bind(&metrics_addr).await?;
        tracing::info!("atlassian-mcp metrics: http://{metrics_addr}/metrics");
        tokio::spawn(async move {
            if let Err(error) = axum::serve(metrics_listener, metrics_app).await {
                tracing::error!("metrics server error: {error}");
            }
        });
    } else {
        tracing::info!("metrics listener disabled (ATLASSIAN_MCP_METRICS_PORT=0)");
    }

    let addr = format!("{listen}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("atlassian-mcp listening on http://{addr}, MCP endpoint: /mcp");
    axum::serve(listener, app)
        .with_graceful_shutdown(crate::signal::shutdown_signal())
        .await?;
    Ok(())
}

async fn metrics_endpoint(State(metrics): State<Arc<Metrics>>) -> Response {
    match metrics.render() {
        Ok(body) => (
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            body,
        )
            .into_response(),
        Err(error) => {
            tracing::error!("metrics render error: {error:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "metrics render error".to_string(),
            )
                .into_response()
        }
    }
}

async fn index(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "name": "atlassian-mcp",
        "version": version(),
        "mcp": "/mcp",
        "read_only": state.cfg.read_only,
        "auth": if state.http.api_token.is_some() { "bearer" } else { "none" },
        "auth_required": state.http.require_auth,
        "products": {
            "jira": state.providers.jira.is_some(),
            "confluence": state.providers.confluence.is_some(),
        },
        "tools": state.tool_count,
    }))
}

async fn healthz(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": version(),
        "products": {
            "jira": state.providers.jira.is_some(),
            "confluence": state.providers.confluence.is_some(),
        },
    }))
}

async fn readyz(State(state): State<AppState>) -> Response {
    if state.providers.is_empty() {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "reason": "no Atlassian products configured",
            })),
        )
            .into_response()
    } else {
        Json(json!({
            "status": "ready",
            "products": {
                "jira": state.providers.jira.is_some(),
                "confluence": state.providers.confluence.is_some(),
            },
        }))
        .into_response()
    }
}

fn version() -> String {
    build_version()
}
