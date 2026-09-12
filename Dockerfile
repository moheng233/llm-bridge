# syntax=docker/dockerfile:1
# 先构建 Dockerfile.base；前端显式构建后由 rust-embed 嵌入。
ARG BASE_IMAGE=localhost/llm-bridge-base:latest
ARG RUNTIME_IMAGE=docker.io/library/debian:trixie-slim
FROM ${BASE_IMAGE} AS planner
WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY xtask/ xtask/
COPY .cargo/ .cargo/
COPY src/ src/
COPY tests/ tests/
COPY examples/ examples/
RUN cargo chef prepare --recipe-path recipe.json

FROM ${BASE_IMAGE} AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --package llm-bridge --no-default-features --features embed-frontend,otel,postgresql --recipe-path recipe.json --locked
COPY Cargo.toml Cargo.lock build.rs ./
COPY xtask/ xtask/
COPY .cargo/ .cargo/
COPY src/ src/
COPY tests/ tests/
COPY examples/ examples/
COPY frontend/ frontend/
RUN pnpm --dir frontend install --frozen-lockfile && pnpm --dir frontend run build
RUN cargo build --release --package llm-bridge --bin llm-bridge --no-default-features --features embed-frontend,otel,postgresql --locked \
    && strip target/release/llm-bridge

FROM ${RUNTIME_IMAGE} AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN groupadd --system llmbridge && useradd --system --gid llmbridge llmbridge
COPY --from=builder /app/target/release/llm-bridge /usr/local/bin/llm-bridge
RUN mkdir -p /data && chown llmbridge:llmbridge /data
VOLUME ["/data"]
USER llmbridge
WORKDIR /app
ENV LLM_BRIDGE_HOST=0.0.0.0 \
    LLM_BRIDGE_PORT=3000 \
    LLM_BRIDGE_STORE_PATH=/data \
    RUST_LOG=info
EXPOSE 3000
ENTRYPOINT ["llm-bridge"]
