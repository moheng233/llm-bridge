# 网关架构与运行边界

## 分层

- axum HTTP/SSE 与 `/v1/ws` 是并列传输绑定，使用 `server/chat_common.rs` 的共享请求 supervisor。
- TokenAuth 验证 Bearer；SessionAuth 每次按数据库用户状态刷新角色，AdminAuth 基于当前角色授权。成员只能读取自己的 Token/用量/trace；管理员可查询全部。
- Store 通过 `LLMModel → ModelProvider → ProviderProtocol → Provider` 解析本地路由与实际定价。Provider 持有跨协议共享的 Key；ModelProvider 是上游模型能力与价格覆盖，不能污染模型标称值。
- ProviderActor 启动适配器流；supervisor 持有上游流与准入预留，不能把 Actor 消息返回当作上游结束。

## 请求生命周期

原子准入锁定 Token 并创建/更新唯一 `(token_id, period_key)` 账本，校验加入本次估算预留后的额度。客户端收到的真实 usage 在流末合并后修正预留。估算不是实际 tokens 的绝对上界；配额是个人自限，不是管理员强制预算。

候选路由只在允许重试且尚未输出时切换；已经输出不能重放。失败启动不记虚假 tokens；启动后取消/中断但缺少 usage 时保留预留。取消关闭上游，不 abort 结算 supervisor。一个业务请求只有一个终结与结算点。

TraceWriter 消费 Begin/Route/Streaming/Finalize，Finalize 与 usage_daily rollup 同事务更新。观察事件队列满时丢弃并计数，不能阻塞业务请求；因此它是可观测性管线，不替代配额账本。真实成本按最终路由与 usage 计算；缺价/缺 usage 为 null，不当作免费；缓存输入扣除后按缓存价格计算，reasoning 不重复叠加到输出费用。

开启 otel 时 request_id、OTel trace_id、stdout 终态日志和持久 trace 可关联。OTLP traces/logs/metrics 使用兼容的线程式导出器。内容快照 Opt-In；默认 retention 30 天、每 6 小时清理一次且启动先执行，0 表示不清理。删除 trace 同时删除其请求/响应快照，不删除 usage_daily 或配额账本。

## 数据库与升级

10 张业务表：users、tokens、usage_records、models、providers、provider_protocols、model_providers、llm_request_traces、usage_daily、cli_sessions。另有内部 `llm_bridge_schema_lock` 用于启动/导入串行化。

默认 SQLite 文件为 `${LLM_BRIDGE_STORE_PATH:-./data}/sqlite.db`，启动递归创建缺失父目录。`LLM_BRIDGE_DATABASE_URL` 非空时覆盖 SQLite 路径；PostgreSQL URL 需启用 postgresql feature。不要把 URL/密码记录到启动日志或提交到仓库。

启动升级是保守的可加性 schema 策略，不是通用迁移框架：

1. 同一事务中创建缺少的表、匹配 PostgreSQL enum、验证现有表具备所有预期列。
2. 同 Token/周期历史重复用量行合并计数，保留最小 ID，再建立复合唯一索引；不丢弃计数。
3. 缺少旧表字段、枚举不兼容、未知 DDL 或唯一约束冲突会拒绝启动并回滚；不会忽略 already-exists 后继续假装升级成功。
4. 不自动重建/删库，也不承诺任意历史列类型/约束变更的自动迁移。部署前备份；不支持的旧结构需运维显式迁移后启动。

## 构建与部署

默认 Cargo features 为空：`cargo run` 仅启动后端 API，后端检查和测试不依赖 Node 或前端产物。先显式执行 `pnpm --dir frontend install --frozen-lockfile`，再用 `cargo xtask dev` 启动独立 Vite 与后端。浏览器默认访问 `http://127.0.0.1:5173`，Vite 将 `/api`、`/auth`、`/v1` 的 HTTP/SSE/WebSocket 转发到后端；`LLM_BRIDGE_PORT` 和 `LLM_BRIDGE_UI_PORT` 分别控制后端与浏览器入口端口。

启动器监控 Rust 输入并重建后端，不重启 Vite，也不自动生成 TS 绑定。编译失败后等待源码修复；服务意外退出则报错并清理其他进程。公开地址默认使用 Vite 入口，显式 `LLM_BRIDGE_BASE_URL` 保留，OIDC callback 必须与公开地址一致。Unix 退出时向子进程组发送 SIGTERM，等待最多 10 秒后强制清理；Windows 使用 Job Object 管理进程树。

生产使用 `cargo xtask build --features otel,postgresql`：前端 `pnpm run build` 成功后，再构建启用 `embed-frontend` 的 release 后端。可按部署需求不启用 otel/postgresql；生产无需 Node/Vite 运行时。`build.rs` 仅检查 `frontend/dist/index.html` 并追踪产物目录（含新增、删除文件），不执行前端构建。Docker 保留等价的 pnpm/Cargo 分层流程，workspace 的 `xtask` 不进入生产二进制。

集成依据：[Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html)、[cargo-xtask 社区模式](https://github.com/matklad/cargo-xtask)、[Vite server proxy](https://vite.dev/config/server-options#server-proxy)。启动器复用 [duct](https://docs.rs/duct/) 执行有限任务；进程树管理采用 [process-wrap API](https://docs.rs/process-wrap/10.0.0/process_wrap/std/) / [上游实践](https://github.com/watchexec/process-wrap)，信号接收采用 [ctrlc API](https://docs.rs/ctrlc/3.5.2/ctrlc/) / [上游实践](https://github.com/Detegr/rust-ctrlc)。

Unix 下监听 SIGTERM/SIGINT（Ctrl+C），收到信号后停止接受新 HTTP 连接，等待已有 HTTP/SSE 请求结束，再进入 main 的导出器 shutdown。容器停止宽限期需要覆盖仍在进行的请求；本次停止验收使用有限时长的 SSE 请求，不承诺无限流会在任意固定宽限期内结束。

Dockerfile.base 提供构建工具链，Dockerfile 使用相同 feature 组合。非 root 用户持有 `/data`，SQLite 文件位于 `/data/sqlite.db`，挂载整个 `/data`。已有卷不由入口脚本删除；bind mount 的宿主机目录需允许容器用户写入。未配置 OIDC 或 Discovery 失败的免登录管理员设计要求可信网络/反向代理边界。

## 明确未支持

本仓库不含可安装 VS Code 插件；不提供团队强制预算、Prometheus `/metrics`、eval、自动目录入库或浏览器免确认签发。低频 P3 项（多候选 n、logprobs、OpenRouter 路由偏好/扩展采样、完整多模态输出等）不宣称端到端支持。真实 IdP、三类商业上游、ARM 和发布镜像的验证证据单独记录于 STATUS；本地夹具通过不能替代这些环境验收。
