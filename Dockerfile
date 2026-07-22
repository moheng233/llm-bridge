# syntax=docker/dockerfile:1

# ─────────────────────────────────────────────────────────────────────────────
# LLM-Bridge 单二进制镜像
#
# 阶段：planner → builder(deps + app，含 vite-rs 前端构建) → runtime
# 产物：内嵌前端（vite-rs Embed）的单个 llm-bridge 可执行文件
#
# 依赖：基础镜像 llm-bridge-base（由 Dockerfile.base 构建，含 Node/pnpm/rustup/
#       nightly/cargo-chef）。先执行：
#         podman build -t llm-bridge-base -f Dockerfile.base .
# 构建：podman build -t llm-bridge .
# 运行：podman run -p 3000:3000 -v llm-bridge-data:/data llm-bridge
#
# 注意（vite-rs 机制，经源码验证）：
#   - vite-rs 的 proc-macro 在 release 编译期自行执行 `npx vite build`，
#     不是读取预先存在的 dist/。因此 builder 必须带 node/npx 与前端依赖。
#   - vite-rs 硬编码调用 npx；pnpm 安装不会生成 .bin 软链，npx 会失败。
#     这里用 npm install 安装依赖（生成扁平 node_modules + .bin），与上游
#     issue #12（pnpm 支持未合入）的现状兼容。
# ─────────────────────────────────────────────────────────────────────────────

ARG BASE_IMAGE=localhost/llm-bridge-base:latest
ARG RUNTIME_IMAGE=docker.io/library/debian:trixie-slim

# ── 阶段 1：cargo-chef 生成依赖 recipe ───────────────────────────────────────
FROM ${BASE_IMAGE} AS planner
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY tests/ tests/
COPY examples/ examples/
RUN cargo chef prepare --recipe-path recipe.json

# ── 阶段 2：编译后端（先依赖、后应用；vite-rs 编译期构建前端） ───────────────
FROM ${BASE_IMAGE} AS builder
WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json

# 只编译依赖（不含 embed-frontend；前端嵌入由 vite-rs 无条件完成）
RUN cargo chef cook --release --recipe-path recipe.json

# 拷贝应用源码与前端工程（vite-rs 编译期在 frontend/ 下执行 npx vite build）
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY tests/ tests/
COPY examples/ examples/
COPY frontend/ frontend/

# 安装前端依赖：用 npm（而非 pnpm）以生成 vite-rs/npx 可用的 .bin 软链
RUN cd frontend && npm install --no-audit --no-fund

# 编译最终二进制并剥离调试符号
RUN cargo build --release --bin llm-bridge \
    && strip target/release/llm-bridge

# ── 阶段 3：运行时镜像 ───────────────────────────────────────────────────────
FROM ${RUNTIME_IMAGE} AS runtime

# ca-certificates：reqwest/rustls 访问上游 HTTPS 所需的根证书
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 非 root 运行
RUN groupadd --system llmbridge && useradd --system --gid llmbridge llmbridge

COPY --from=builder /app/target/release/llm-bridge /usr/local/bin/llm-bridge

# SQLite 数据目录（可通过 LLM_BRIDGE_STORE_PATH 覆盖），建议挂卷持久化
RUN mkdir -p /data && chown llmbridge:llmbridge /data
VOLUME ["/data"]

USER llmbridge
WORKDIR /app

# 容器内默认监听所有网卡；数据落 /data
ENV LLM_BRIDGE_HOST=0.0.0.0 \
    LLM_BRIDGE_PORT=3000 \
    LLM_BRIDGE_STORE_PATH=/data/llm-bridge \
    RUST_LOG=info

EXPOSE 3000

ENTRYPOINT ["llm-bridge"]
