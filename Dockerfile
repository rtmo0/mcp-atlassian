FROM rust:1.98-slim-bookworm AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake perl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
# Cache dependencies separately from the application sources.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && printf 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src
COPY src ./src
RUN touch src/main.rs && cargo build --release --locked

FROM debian:bookworm-slim
ARG VERSION=dev
LABEL org.opencontainers.image.title="atlassian-mcp" \
      org.opencontainers.image.description="Read-only MCP server for Atlassian Jira and Confluence (Cloud)" \
      org.opencontainers.image.source="https://github.com/rtmo0/mcp-atlassian" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0" \
      org.opencontainers.image.version=$VERSION
ENV ATLASSIAN_MCP_BUILD_VERSION=$VERSION \
    ATLASSIAN_MCP_LISTEN=0.0.0.0 \
    ATLASSIAN_MCP_METRICS_LISTEN=0.0.0.0
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 atlassian-mcp
COPY --from=build /src/target/release/atlassian-mcp /usr/local/bin/atlassian-mcp
RUN /usr/local/bin/atlassian-mcp --version
USER atlassian-mcp
EXPOSE 8080 9090
ENTRYPOINT ["atlassian-mcp"]
CMD ["serve"]
