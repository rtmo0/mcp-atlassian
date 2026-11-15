use std::time::Duration;

use anyhow::Result;
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGaugeVec, Opts, Registry, TextEncoder,
};

/// Prometheus metrics. Labels are bounded: tool name, product and status only.
/// JQL/CQL, URLs, user ids and page ids are never used as labels.
pub struct Metrics {
    registry: Registry,
    pub tool_requests: IntCounterVec,
    pub tool_duration: HistogramVec,
    pub in_flight: IntGaugeVec,
}

impl Metrics {
    pub fn new(version: &str, jira: bool, confluence: bool) -> Result<Self> {
        let registry = Registry::new();

        let info = prometheus::IntGaugeVec::new(
            Opts::new("atlassian_mcp_info", "Atlassian MCP build information"),
            &["version"],
        )?;
        info.with_label_values(&[version]).set(1);
        registry.register(Box::new(info))?;

        let products = IntGaugeVec::new(
            Opts::new(
                "atlassian_mcp_product_configured",
                "Whether a product is configured (1) or not (0)",
            ),
            &["product"],
        )?;
        products.with_label_values(&["jira"]).set(i64::from(jira));
        products
            .with_label_values(&["confluence"])
            .set(i64::from(confluence));
        registry.register(Box::new(products))?;

        let tool_requests = IntCounterVec::new(
            Opts::new(
                "atlassian_mcp_tool_requests_total",
                "MCP tool calls by tool, product and status",
            ),
            &["tool", "product", "status"],
        )?;
        registry.register(Box::new(tool_requests.clone()))?;

        let tool_duration = HistogramVec::new(
            HistogramOpts::new(
                "atlassian_mcp_tool_duration_seconds",
                "MCP tool latency in seconds",
            )
            .buckets(vec![0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0]),
            &["tool"],
        )?;
        registry.register(Box::new(tool_duration.clone()))?;

        let in_flight = IntGaugeVec::new(
            Opts::new(
                "atlassian_mcp_in_flight_requests",
                "Currently active MCP tool calls",
            ),
            &["tool"],
        )?;
        registry.register(Box::new(in_flight.clone()))?;

        Ok(Self {
            registry,
            tool_requests,
            tool_duration,
            in_flight,
        })
    }

    pub fn tool_started(&self, tool: &str) {
        self.in_flight.with_label_values(&[tool]).inc();
    }

    pub fn tool_finished(&self, tool: &str, product: &str, status: &str, duration: Duration) {
        self.in_flight.with_label_values(&[tool]).dec();
        self.tool_requests
            .with_label_values(&[tool, product, status])
            .inc();
        self.tool_duration
            .with_label_values(&[tool])
            .observe(duration.as_secs_f64());
    }

    pub fn render(&self) -> Result<String> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }
}
