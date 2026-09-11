# LLM-Bridge 项目状态与交付记录

> 最后核查：2026-09-11。实现分支：`feat/status-delivery`。
> 本文记录当前工作区的实现和实际验收，不代表远端已经发布。设计规格见 [PLAN.md](PLAN.md)，使用与部署见 [README.md](README.md)。

## 1. 当前结论与范围

原 STATUS 的 **W1–W7 网关侧工作已实现并完成本地行为、浏览器及 release 部署验收**。原 20 条问题保留编号，关闭依据见第 3 节。交付包括 HTTP/WS 共享编排、设备码登录、三层目录导入、数据隔离与配额结算、可观测性闭环和可重复部署。

这不是“所有外部环境均已验证”的声明：真实 IdP、三类商业上游完整兼容性、ARM、多节点压力和远端发布仍在第 6 节明确列出。本地受控上游用于复现协议边界；Docker 与 PostgreSQL 验收使用真实运行实例。

### 1.1 保留的产品约束

- 未配置 OIDC，或 OIDC Discovery 失败后继续以免登录管理员模式运行，是既定设计。部署者必须提供可信网络/反向代理访问边界；不改为拒绝启动。
- `/v1/models`、`/v1/chat/completions`、`/v1/ws` 仍要求 Bearer Token，免登录管理模式不取消客户端认证。
- 配额是**个人 Token 自限**，不是团队强制预算。vscode Token 名称以 `vscode:` 开头，默认 `allowed_models=[]`、配额 unlimited；这些默认值不变。
- 手动配置始终可用，业务路由只读本地数据库。目录仅由管理员手动预览/导入，不做定时自动入库。
- 不引入旧 ConnectionActor 转发链、CLI 回环回调、免确认签发、Prometheus `/metrics` 或 eval 管线。
- 可安装 VS Code 插件本体、完整浏览器 E2E 自动化、i18n、allowedModels 多选 UI 等不属于本次网关交付。

## 2. 当前实现

### 2.1 技术与数据基础

| 维度 | 当前实现 |
| --- | --- |
| 后端 | Rust Edition 2024 / nightly，axum 0.8、tokio、ractor 0.16 |
| HTTP/TLS | reqwest 0.13 + rustls，共享客户端初始化 crypto provider |
| 数据库 | toasty 0.10；默认 SQLite，`LLM_BRIDGE_DATABASE_URL` 接通 PostgreSQL（需 feature） |
| 前端 | Vue 3、TypeScript、Vite 8、Vue Router、Pinia、Tailwind CSS 4、reka-ui |
| 类型与 CI | ts-rs + axfetchum；不改写仓库产物的漂移检查，后端合约、前端构建/lint 和生产 feature 检查 |
| 可观测性 | request_id、OTel trace_id、stdout 终态、OTLP traces/logs/GenAI metrics、持久 trace 与 daily rollup |

10 张业务表：`users`、`tokens`、`usage_records`、`models`、`providers`、`provider_protocols`、`model_providers`、`llm_request_traces`、`usage_daily`、`cli_sessions`。另有内部 schema/导入串行化锁表。

### 2.2 接口与数据流

- OpenAI HTTP 支持 SSE/非流式、工具历史的 null/缺失 content、标准 `owned_by`；文本、图片、工具和 reasoning 沿用既有适配器。
- HTTP 与 `/v1/ws` 共用 `src/server/chat_common.rs` 的准入、候选回退、usage 累积和最终结算。Actor 返回不再等同于上游流结束。
- SessionAuth 每次从数据库刷新用户角色；成员的汇总、trace 列表和详情按归属限制，管理员可查看全部。
- Token/周期账本原子预留并具备复合唯一约束，准入包含本次预留。末尾真实 usage 修正预留；启动后取消且缺 usage 时保留预留，不把未知消耗当作免费。
- 可重试失败仅在尚未输出时切换候选；已经输出后不重放。一个业务请求只终结和结算一次。
- TraceWriter 路由/Streaming/Finalize 事件关联实际路由、首块、终态和成本。观察事件满队列可丢弃并计数，不替代配额账本。

## 3. 原问题台账与关闭依据

| 编号 | 原问题 | 当前状态与证据 |
| --- | --- | --- |
| BUG-001 | 成员可读取其他用户的用量与 trace | 已修复；双成员/管理员的汇总、列表、详情 HTTP 合约通过 |
| BUG-002 | 数据库降权不影响旧 Session | 已修复；沿用旧 Session cookie 的降权回归通过 |
| BUG-003 | 流式账本提前结算为 0 | 已修复；延迟末尾 usage 的客户端、trace、账本均为真实 100 tokens，错误/取消不误报成功 |
| BUG-004 | 个人自限被描述为团队强制约束 | 已按既定范围关闭；收窄文档承诺，不扩展强制预算、不更改 vscode 默认值 |
| BUG-005 | 并发准入超额、丢计数或重复周期行 | 已修复；并发首次预留共用一个周期账本，并遵守准入上限 |
| BUG-006 | 未检查加入本次预留后的边界 | 已修复；边界可准入，超额拒绝且不遗留用量；跨周期结算仍定位原周期 |
| BUG-007 | null 工具历史返回 422 | 已修复；标准工具历史与回退 HTTP 合约通过，原文本/图片转换回归通过 |
| BUG-008 | 模型列表缺少 owned_by | 已修复；HTTP 合约检查标准字段 |
| BUG-009 | 首选失败后备用路由未收到请求 | 已修复；输出前失败回退成功，整个业务请求不重复计费 |
| BUG-010 | OIDC 前后端角色/DTO 不一致 | 已修复；统一 me DTO 和 admin/member 角色，权限合约通过 |
| BUG-011 | 图片目标与下载大小未受控 | 已修复；地址/重定向目标策略、有界下载及内网/映射地址拒绝回归；未访问真实内网目标 |
| BUG-012 | cargo run 无法选择默认二进制 | 已修复；独立工作目录中 `cargo +nightly run --manifest-path ...` 未指定 --bin 即启动网关 |
| BUG-013 | 空数据目录无法启动、容器路径错误 | 已修复；默认与嵌套空目录自动创建，非 root 容器 `/data/sqlite.db` 可写且重启保留 Token |
| BUG-014 | 干净前端首次构建失败 | 已修复；Vite 先生成自动声明再运行 vue-tsc；Docker 干净依赖安装后一次完整 build 通过 |
| BUG-015 | 旧库被简单忽略 already-exists | 已修复；事务创建缺表、合并重复周期计数并建立唯一索引；不兼容结构回滚且保留旧行，边界见第 6 节 |
| BUG-016 | 技术栈、授权、命令与部署文档过时 | 已更新 README、PLAN 及架构/API 文档，明确默认路径、数据库 URL 优先级和 feature 行为 |
| BUG-017 | PostgreSQL feature 未接入运行配置 | 已修复；真实 PostgreSQL 导入/重导入、跨进程数据保留与聊天落库通过；未产生 SQLite 回退库 |
| BUG-018 | 生产 feature clippy 警告 | 已修复；默认与 `embed-frontend,otel,postgresql` 生产组合 clippy 零警告，release 镜像构建通过 |
| BUG-019 | 仪表盘使用固定示例环比 | 已修复；真实上周期汇总驱动环比，浏览器验证分页、Token/日期筛选和计算结果 |
| BUG-020 | 目录未输出提供者能力覆盖 | 已修复；覆盖只进入关联层，生成器覆盖/omit/引用/重复生成回归通过 |

另外修复了流内上游错误被当作成功、OTLP 指标导出导致主进程退出、容器未处理 SIGTERM 而被强杀、模型表单错误被模态框遮住，以及生成绑定误入手写风格 lint 的问题。

## 4. W1–W7 交付与验收

| 工作包 | 交付与验收 |
| --- | --- |
| W1 现有正确性 | 成员隔离、实时降权、原子配额、真实终态、工具历史、标准模型字段、输出前回退及图片访问策略 |
| W2 可重复部署 | 默认二进制、空目录创建、保守升级、干净前端构建、feature 组合、非 root release/Docker、PostgreSQL 与 SIGTERM 正常退出 |
| W3 共享编排与 WS | 协议信封与 TS union、共享 supervisor、鉴权、listModels/chat/cancel、8 路并发、30s Ping、mpsc(64)、慢消费 1008、断连清理；6 项 HTTP/WS 合约通过 |
| W4 设备码登录 | CliSession 状态机、四端点、浏览器主动确认、10 分钟过期、5s 轮询、一次领取、旧 vscode Token 吊销、UI/绑定/时序文档 |
| W5 可观测性 | request_id/trace_id 关联、终态/首块/上游元数据、实际路由定价、真实环比/筛选/分页、Opt-In 响应快照和 retention |
| W6 目录导入 | 派生契约、可覆盖 URL、条件拉取、三层预览、事务幂等与 Key/调度设置保留、共享三层选择对话框及两个入口 |
| W7 收尾与交付 | 全局搜索/快捷键、表单边界与单位、错误可见性、badge 语义、类型漂移和构建行为 CI、支持范围与接口文档 |

### 4.1 原首批回归清单

- [x] 延迟末尾 usage 的流式请求：客户端、trace、配额账本真实 100 tokens，错误/取消不误报 success、不重复结算。
- [x] 同 Token 并发准入、临界额度与同周期首次请求：不丢计数、不重复周期行、不突破预留约束。
- [x] 首选路由 503/网络失败：仅在可重试且尚未输出时切换候选，不重复计费。
- [x] OpenAI null/缺失 content 工具历史、多轮工具结果、标准模型字段与原文本/图片路径。
- [x] 两个成员与管理员的汇总/列表/详情隔离，旧 Session 降权生效，免登录设计不变。
- [x] 干净前端依赖目录、默认/自定义数据目录和支持的 feature 组合。
- [x] WS、设备码一次领取、目录幂等/Key 保留随所属功能分别验收。

## 5. 实际验证记录

### 5.1 自动化与构建

| 检查 | 结果与范围 |
| --- | --- |
| `cargo test --all-targets --locked --offline -- --skip export_bindings` | 81 通过，0 失败；1 个显式客户端生成测试按设计 ignored，88 个绑定导出过滤。包含 67 个库测试及权限、配额、目录、HTTP/WS 合约；默认 features 不含 otel |
| `cargo test --no-default-features --features embed-frontend,otel,postgresql --test otel_contract --locked --offline` | 2 通过；覆盖 OTLP 导出和 shutdown 的实际运行边界 |
| `python3 scripts/check-bindings.py` | 88 个类型导出与 API 客户端匹配仓库产物；比较过程不改写仓库绑定 |
| 默认/生产 feature clippy `-D warnings` | 通过；生产为 `--no-default-features --features embed-frontend,otel,postgresql` |
| `pnpm --dir frontend run lint` | 通过；生成绑定由 TypeScript 编译与漂移检查负责，手写代码保留现有 lint 规则 |
| 目录生成器 `bun test` | 7 通过，0 失败，38 个断言 |
| Docker 多阶段构建 | 干净前端安装、`vite build && vue-tsc -b`、生产 feature Rust release 编译与 strip 通过 |

本地最终镜像：`localhost/llm-bridge:status-delivery`，镜像 ID `sha256:602dfbdbefac4722490cc2fc4ffbc985694b838aa4feb1db45ce79d44a7f14b8`。构建显式使用本地 `localhost/llm-bridge-base:status-delivery`；README 的默认命令先构建 `latest` 基础镜像。

### 5.2 真实运行与浏览器证据

- 本地 HTTP/SSE 与 WS 使用独立数据库和受控上游，覆盖延迟 usage、流内错误、取消、断连、并发与输出前候选回退。受控上游不是商业上游兼容性认证。
- 启用 OTLP 后实际导出 traces/logs/metrics，持久 trace 的 trace_id 与请求日志关联；短指标导出周期下主进程持续运行。终态、usage 与成本已在 API/数据库核对。
- 非 root release 容器默认数据路径为 `/data/sqlite.db`，写入 Token 后重启仍可读取。
- 关闭冒烟复现了旧容器不响应 SIGTERM、宽限期后退出 137；修复后在两秒延迟 SSE 的首块与末尾 usage 之间发送 TERM，仍收到 `[DONE]` 与真实 100 tokens，容器退出 0。持久 trace 为 success/100，该 Token 的账本与日聚合均为累计 2 请求/200 tokens。此项以真实容器冒烟验证，未增加依赖 Docker 的永久测试。
- PostgreSQL 使用真实容器数据库；release 网关重新连接后原导入模型仍在，聊天为 200，日账本为 `1 请求 / 100 tokens / $0.00016`，容器没有 SQLite 回退文件。
- retention 验收时将独立测试库的一条 trace 设为过期：trace 数 `53 → 52`，过期行及快照删除，daily rollup 仍为 `53 请求 / 5200 tokens`。
- release 浏览器追踪页第 2 页实际显示第 51–52 条；新增另一个 Token 的请求后，Token 筛选缩到该 Token 的 1 条，未来日期筛选为 0 条。
- 在独立测试库放入受控上周期聚合 `26 请求 / 2600 tokens / $0.00416`：当前 `53 / 5200 / $0.00832`，页面显示 `+103.8% / +100.0% / +100.0%`。这是验证计算的测试数据，不是生产历史流量。
- 目录两个 UI 入口、选择联动、导入/重导入和 Key 保留已操作；PostgreSQL 首次导入 4 条，重导入更新/复用 4 条，不增加重复行。
- 设备码确认页、主动输码与确认、一次领取和旧凭据吊销分别验证；不宣称真实外部 IdP 已完成验收。
- Ctrl+K 打开页面/模型搜索并支持导航；模型表单拒绝负数，错误在对话框内可见，`256K` 保存为 `256000`，重新打开没有旧错误。

## 6. 已知边界与未验证范围

### 数据和安全边界

- 数据库升级是保守可加性策略，不是通用迁移框架：创建缺表、合并重复周期计数；缺失旧列、不兼容 enum/DDL/约束会拒绝启动并事务回滚。不自动删库，不承诺任意历史列类型变化的自动迁移。部署前仍需备份。
- 配额估算不是实际 tokens 的绝对上界，个人可修改自己的 Token 限额；不要将其用于团队强制预算承诺。
- trace 事件在压力下可按设计丢弃；daily rollup 属于可观测性管线，不替代业务配额账本。成本来自已配置价格和 usage，不是供应商账单。
- API Keys 当前存于数据库 JSON。数据库文件、卷、备份与访问权限必须保护；不在本次范围内擅自引入加密存储方案。
- WS 连接存续期的 Token 吊销不主动断开既有连接，这是计划接受的限制。

### 外部环境与范围外能力

- 未验收真实 IdP 登录全流程、三类商业上游的全部协议组合、ARM、长期压力/多实例容量、完整桌面与移动端矩阵，以及内存/二进制体积宣传指标。
- 本地 release 镜像已构建运行，但未推送远端镜像，也未触发本分支的远端 Actions/Pages 发布。既有公开 Pages 的 HTTP 200 快照不能证明新生成器已经发布。
- 目录 workflow 为每日与手动触发，不承诺跨仓库 push 自动触发。生成器只纳入有 api 的 Provider、有效 base_model 关联及被引用模型；未映射的 cache_write/reasoning/audio 价格等保持显式排除。
- 多候选 `n`、logprobs/top_logprobs、parallel_tool_calls、OpenRouter provider 偏好/扩展采样、多模态输出和独立 refusal/reasoning_details 等 P3 字段仍按需排期，不宣称完整 OpenRouter 等价。
- VS Code 插件本体尚未交付；网关的 WS/设备码接口及客户端示例不等于可安装扩展。

## 7. 文档与维护入口

- [README](README.md)：使用、配置、运行与构建命令。
- [架构与升级边界](docs/architecture.md)：共享请求生命周期、配额、观察管线和数据库升级。
- [管理 API](docs/admin-api.md)、[WS 协议](docs/ws-api.md)、[设备码登录](docs/device-login.md)、[目录导入](docs/catalog-import.md)：接口契约、错误与示例。
- 后续修复继续维护原问题编号、实际验证命令和边界；设计变化更新 PLAN，不以状态文档静默改变产品范围。
