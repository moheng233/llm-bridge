# LLM-Bridge 项目状态与后续工作

> 最后核查：2026-09-11。依据当前工作区源码、[README.md](README.md)、[PLAN.md](PLAN.md) 及本次会话中的构建、测试和本地 HTTP 验证。
> 本报告包含尚未提交的用户改动，不代表某个已发布版本；重写文档不表示问题已经修复。

## 1. 当前结论与范围

项目处于 **基础网关可受控试用、关键正确性问题待修复、计划功能仍未全部交付** 的阶段。普通非流式聊天和手动配置主链路可以运行，但不能将现状概括为“只剩测试和文档”。

完成当前计划还需要：修复已有 BUG，交付 WebSocket RPC 与设备码登录，接通目录导入后端和 UI，补齐可观测性的数据与生命周期。不同模块工作量差异较大，不沿用旧报告的完成百分比和历史工时估计。

### 1.1 既定设计，不作为缺陷

- 用户已确认：**未配置 OIDC，以及 OIDC Discovery 失败后继续以免登录管理员模式运行，属于有意设计**。不列为 BUG，不要求改为拒绝启动；部署时需使用符合该信任模型的访问边界。
- 免登录模式开放管理功能，不等于 `/v1/models` 和 `/v1/chat/completions` 免 Bearer Token。两个客户端接口仍使用 `TokenAuth`。
- PLAN §4.1 的 vscode Token 默认 `allowed_models=[]`、配额 unlimited，名称以 `vscode:` 开头；不擅自修改这些默认值。个人 Token 自限与团队强制预算是不同契约，见 BUG-004。
- 自动导入不取代手动配置；运行时路由仍查询本地数据库，不依赖远端目录在线。
- PLAN §6 排除完整浏览器 E2E 自动化、i18n、allowedModels 多选 UI、一次性 Token 下载等；WS 协议契约测试和局部 HTTP 集成测试仍在范围。
- PLAN 明确不引入 Prometheus `/metrics`、eval 管线、旧 ConnectionActor 转发层、CLI 回环回调或免确认签发，也不做定时自动同步目录到业务数据库。

### 1.2 文档职责与证据口径

- 本文统一维护项目总览、问题台账、后续工作及验收顺序，不再另设独立 BUG 文档。
- 问题编号、影响与证据见第 3 节，功能缺口见第 4 节，后续工作和验收条件见第 5 节。
- [PLAN.md](PLAN.md) 保留设计规格；其中过时的状态文字不能替代源码和验证结果，差异见本文第 4 节。
- “已实现”只表示存在接线或业务代码；“已复现”表示本次运行观察到问题；“代码确认”不等于已完成真实上游、压力或部署测试。

## 2. 已有实现与限制

### 2.1 技术与数据基础

| 维度 | 当前实现 |
| --- | --- |
| 后端 | Rust Edition 2024 / nightly、axum 0.8、tokio、ractor 0.16 |
| HTTP/TLS | reqwest 0.13 + rustls；统一客户端入口负责安装 crypto provider |
| 数据库 | toasty 0.10，主程序使用 SQLite；PostgreSQL 仅启用了库层 feature，未完成运行配置接线 |
| 前端 | Vue 3 + TypeScript + Vite 8 + Vue Router 5 + Pinia + Tailwind CSS 4 + reka-ui |
| 类型生成 | ts-rs 类型导出 + axfetchum API 客户端生成；漂移检查尚未启用 |
| 可观测性 | tracing、OTLP traces/logs/GenAI metrics、SQLite trace 与 daily rollup |

当前注册 **9 张表**：`users`、`tokens`、`usage_records`、`models`、`providers`、`provider_protocols`、`model_providers`、`llm_request_traces`、`usage_daily`。`CliSession` 尚未加入。

实现入口：[Cargo.toml](Cargo.toml)、[frontend/package.json](frontend/package.json)、[src/http.rs](src/http.rs)、[src/db/mod.rs](src/db/mod.rs)。

### 2.2 功能现状

| 能力 | 已有实现 | 当前限制 |
| --- | --- | --- |
| OpenAI HTTP 入口 | 模型列表、流式 SSE、非流式聚合；system/developer、采样参数、reasoning、工具调用及图片输入的转换代码 | null 工具历史、模型字段名、回退和流式账本仍有缺陷，不能宣称完全兼容 |
| 上游适配 | OpenAI Chat、OpenAI Responses、Anthropic Messages；自定义 URL/path/headers、SSE 解析、usage/工具调用处理 | 解析测试通过不代表三类真实服务已完整验收 |
| 模型与 Provider 管理 | Provider、Protocol、LLMModel、ModelProvider 管理接口和页面；四表应用层关联及多 Key 加权选择 | 候选列表存在，但请求只使用第一条；四表关联不是 SQL 原生 JOIN |
| 认证与 Token | OIDC 服务、Session、首用户 Admin、Token CRUD、Bearer 校验、模型范围、周期配额 | OIDC 前端角色契约、降权生效、用户隔离、配额原子性与结算待修复 |
| 管理界面 | 模型目录、模型管理、Provider/协议管理、Token、用户、仪表盘、trace 列表与详情 | 首次生产构建顺序、部分示例指标和查询交互待补齐 |
| 可观测性基础 | request_id、trace 异步写入、事务内 daily rollup、GenAI metrics 代码、请求快照 Opt-In、保留任务 | 成本、响应快照、trace_id 入库及流式终态尚未闭环 |
| 目录生成发布 | models.dev 三层 TOML 派生脚本、测试、定时/手动 Actions、Pages JSON | 提供者能力覆盖未输出；网关预览/导入及 UI 未实现 |

主要入口：[src/server/openai_api.rs](src/server/openai_api.rs)、[src/server/admin.rs](src/server/admin.rs)、[src/server/auth.rs](src/server/auth.rs)、[src/server/tokens.rs](src/server/tokens.rs)、[src/store/router.rs](src/store/router.rs)、[frontend/src/App.vue](frontend/src/App.vue)。

当前聊天 handler 直接编排 Store 和每请求 ProviderActor；GatewayManager 提供路由查询消息及周期配额检查任务。Provider 的独立上游流任务与 Actor 退出不是同一生命周期，这是流式结算缺陷的关键边界。

## 3. 已发现问题

共 **20 条记录**，均未在本次审查中修复。P1 表示影响功能正确性、用户隔离、配额或可重复部署；P2 表示文档、指标展示、维护或质量门槛。BUG-004 需要先明确产品治理边界，不能直接按强制用户预算扩展设计。

| 编号 | 优先级 | 问题及影响 | 证据 |
| --- | --- | --- | --- |
| BUG-001 | P1 | 用量汇总、trace 列表和详情未按用户隔离；开启快照时可能读取他人请求内容 | 代码确认 |
| BUG-002 | P1 | 用户在数据库降权后，旧 Session 仍保留管理员权限 | 代码确认 |
| BUG-003 | P1 | 流式结算等待 Actor 而非流结束；客户端收到 100 tokens，账本仍为 0，trace 总量为空 | 已复现 |
| BUG-004 | P1 | 成员可自行创建或修改无限额、全模型 Token；自限不能提供 README 宣称的团队强制约束 | 代码确认，治理边界待明确 |
| BUG-005 | P1 | 配额查询、检查和读改写分离；同 Token 同周期无复合唯一约束，存在并发超额或丢计数风险 | 代码确认，未做并发压力复现 |
| BUG-006 | P1 | 配额准入只检查已有用量，没有校验加上本次预留后是否越界 | 代码确认 |
| BUG-007 | P1 | 标准 assistant 工具历史 `content: null` 返回 422，阻断多轮工具调用 | 已复现 |
| BUG-008 | P1 | `/v1/models` 输出 `ownedBy`，缺少 OpenAI 标准 `owned_by` | 已复现 |
| BUG-009 | P1 | 首路由返回 503 后直接失败，备用路由没有收到请求 | 已复现 |
| BUG-010 | P1 | OIDC 写入 `Admin`/`Member`，前端只认 `admin`；`/auth/me` 实际 DTO 与前端声明不同 | 代码确认 |
| BUG-011 | P1 | 图片 URL 没有内网/重定向目标策略，且完整下载后才检查大小 | 代码确认，未访问真实内网目标 |
| BUG-012 | P1 | README 的 `cargo run` 无法在 `cli`、`llm-bridge` 两个二进制间选择 | 已复现 |
| BUG-013 | P1 | 不存在的数据目录未创建，启动报数据库文件打不开；容器目录层级也需校正 | 已复现本地空目录场景 |
| BUG-014 | P1 | 前端先 vue-tsc 后 Vite，干净环境缺自动声明；预先生成后完整构建才通过 | 已复现 |
| BUG-015 | P1 | 启动 push_schema 遇表已存在就跳过，不能保证旧库的新表/字段升级 | 代码确认，旧版本升级未实测 |
| BUG-016 | P2 | README/注释中的技术栈、授权行为、命令、路径和文档引用落后于实现 | 代码确认 |
| BUG-017 | P2 | PostgreSQL feature 没有让主程序脱离写死的 SQLite 连接 URL | 代码确认，未连接 PostgreSQL |
| BUG-018 | P2 | `embed-frontend,otel` 组合有 `needless_return`，未达到 clippy 零警告门槛 | 已复现；同组合编译检查通过 |
| BUG-019 | P2 | 仪表盘主数值来自真实 API，环比却固定为 `+12.0%`、`+8.4%`、`-3.1%` | 代码确认 |
| BUG-020 | P1 | 目录脚本计算 merged/omit，但未把提供者 limit/modalities 等覆盖输出到关联数据 | 代码确认；现有测试未覆盖该断言 |

本节保留全部问题编号和证据摘要，相关实现入口见第 2、4 节，回归与关闭条件见第 5 节，已执行验证见第 6 节。其中 BUG-003/005/006/009 共享请求准入、预留、回退与结束结算边界，应协同设计，不能分别打补丁后仍重复扣减或丢失取消事件。

## 4. 对照 PLAN 的功能差距

### 4.1 WebSocket RPC：主体未实现

对应 PLAN §4，目标是与 OpenAI HTTP 并列的 `/v1/ws` 传输绑定。

| 阶段 | 当前状态 | 后续交付 |
| --- | --- | --- |
| A 协议类型与导出 | 既有 LM 消息类型已补 TS 导出；WS 类型未实现 | WsChatParams、客户端/服务端信封、错误码、done/listModels 类型；Serde/TS union 契约断言 |
| B 共享聊天编排 | UsageAccumulator、估算与结算仍在 HTTP handler 内 | 修复已知生命周期 BUG 后提取共享 prepare/usage/结算逻辑与结构化错误；区分行为修复和纯移动 |
| C WS handler | 无路由、读写任务和连接内请求表 | 握手 Bearer 鉴权、chat/listModels/cancel、请求 ID 归属、多路复用、断连清理、保活和背压 |
| D 文档与示例 | WS 契约测试、客户端示例和 API 文档未交付 | chunk 序列、并发、cancel 结算、401、坏帧、慢消费测试；插件开发文档及最小客户端 |

实施时保持计划参数：每连接最多 8 个并发请求、30s Ping、出站 mpsc(64)、慢消费关闭码 1008。ToolCall 参数累计完整后一次发出，Text/Thinking 按现有增量语义处理，不另建旧 ConnectionActor/GatewayManager 转发链。

证据：[src/types.rs](src/types.rs)、[src/server/mod.rs](src/server/mod.rs)、[src/actors/provider/mod.rs](src/actors/provider/mod.rs)。

### 4.2 设备码登录：未实现

对应 PLAN §4.1。目前没有 CliSession 模型、会话服务、四个端点、确认页或自动吊销逻辑。已有 [src/bin/cli.rs](src/bin/cli.rs) 是数据库管理 CLI，不是插件登录实现。

- [ ] 新增 CliSession 的 pending/approved/consumed/expired 状态、用户码、有效期和一次性 Token 领取能力。
- [ ] 实现创建会话、轮询会话、确认授权、浏览器验证页四个端点及 Session/OIDC 跳转。
- [ ] 保留主动输码和确认要求，覆盖过期、重复确认、重复领取和无效会话。
- [ ] 签发 `vscode:` Token 前吊销该用户旧 vscode Token；默认全模型/unlimited 不变。
- [ ] 补 UI、TS 绑定及插件登录时序文档；新增表前明确 schema 升级或开发期重建策略。

WS 可以使用手工创建的 Token 独立验收；设备码登录也可独立交付，不必等待 WS 完成。连接存续期 Token 吊销不主动断开是计划明确接受的首版限制，不作为额外必做项。

### 4.3 可观测性：存储和页面已有，数据链路待补齐

对应 PLAN §5。request_id 中间件、GenAI span/metrics 代码、TraceWriter、事务内 daily rollup、三个查询接口、仪表盘、trace 详情与 retention 均已有实现。

| 部分 | 剩余工作 | 验收要点 |
| --- | --- | --- |
| O1 请求关联 | 将 OTel trace_id 写入 DB；验证 stdout、OTLP 与 request_id 互查 | 不能仅因中间件有 record 调用就宣称三方关联完成 |
| O2/O3 生命周期 | 修复流式结算；完整处理 success/error/cancelled、首块时间与上游元数据 | 实际 usage、终态、耗时一致；HTTP/WS 使用同一套结束语义 |
| O4 成本 | 从实际 ModelProvider 路由定价与 usage 派生成本，写 trace 和 rollup | 当前真实请求 cost_usd 为 None；缺价与零费用需区分，避免缓存/推理重复计费 |
| O4 查询与仪表盘 | trace 时间范围、前端 Token 筛选、真正分页及真实环比 | API 有分页参数，但 UI 固定第一页 200 条；固定示例指标不能作为真实数据展示 |
| O5 内容追踪 | Opt-In 下采集响应 parts；验证内容保留和删除 | 页面能渲染 response_parts，但实际请求路径始终留空；请求快照已经采集 |

trace 生命周期当前只有 Begin/Finalize 事件，不能把枚举存在视为 streaming/cancelled 状态已完整接通。异步写入器和 retention 已存在，不需重新开发；流式计数缺陷需要重新打开 O2/O3 的验收。

主要实现：[src/observability/trace_writer.rs](src/observability/trace_writer.rs)、[src/observability/retention.rs](src/observability/retention.rs)、[src/server/usage.rs](src/server/usage.rs)、[frontend/src/pages/dashboard.vue](frontend/src/pages/dashboard.vue)、[frontend/src/pages/traces/index.vue](frontend/src/pages/traces/index.vue)、[frontend/src/pages/traces/[id].vue](frontend/src/pages/traces/%5Bid%5D.vue)。

### 4.4 models.dev 目录导入：生成发布已通，应用侧未接

对应 PLAN §7。旧的运行时 models.dev 直接集成已删除，新方案是独立生成三层 JSON，由管理员手动预览并导入本地数据库，不是恢复旧的远端运行时依赖。

| 阶段 | 当前状态 | 剩余交付 |
| --- | --- | --- |
| Phase 1 派生管道 | 脚本、4 个 fixture 测试、定时/手动 Actions、Pages 产物已验证 | 修复 BUG-020；补覆盖/omit、坏引用、重复生成断言；明确 schema 兼容性 |
| Phase 2 后端导入 | 未实现配置、预览、diff、导入接口和专用 upsert 组合 | sourceUrl/环境变量、条件拉取、契约校验、三层预览；事务幂等 upsert、错误统计与 Key 保留 |
| Phase 3 前端导入 | 未实现共享对话框和两个入口 | 搜索/筛选、三层选择及依赖联动、进度反馈、模型页与 Provider 页接线、绑定生成 |

公开发布实测快照：

| 项目 | 2026-09-11 验证结果 |
| --- | --- |
| [catalog.json](https://moheng233.github.io/llm-bridge/catalog.json) | HTTP 200，schemaVersion=1 |
| [contract.json](https://moheng233.github.io/llm-bridge/contract.json) | HTTP 200，schemaVersion=1 |
| generatedAt | `2026-09-11T03:31:56.729Z` |
| sourceRev | `bc0281f9dad9fc6c0a90b39254b970813273bbc8` |
| 内容 | 329 个模型、187 个提供者、3810 条关联；未发现模型或提供者悬空引用 |

发布主链路已交付不等于全部源数据语义已经正确：脚本只导出带 api 的 Provider、带 base_model 的关联及被关联引用的模型；提供者能力覆盖未输出的问题仍待修复。现有测试没有证明整个源目录覆盖、幂等和能力合并都已验收。

导入必须保证：已有 API Key 不被清空，重复导入不产生重复行，提供者覆盖不污染模型标称值。计划采用单一可覆盖 URL 和管理员手动触发；当前 workflow 只有每日与手动触发，没有跨仓库 push 自动触发。

实现依据：[scripts/models-dev-catalog/src/generate.ts](scripts/models-dev-catalog/src/generate.ts)、[scripts/models-dev-catalog/test/generate.test.ts](scripts/models-dev-catalog/test/generate.test.ts)、[.github/workflows/models-dev-catalog.yml](.github/workflows/models-dev-catalog.yml)。

### 4.5 按需兼容与前端收尾

PLAN §3 的 P3 项基本仍待实现，保持“按需”优先级，不将全部 OpenRouter 特有字段设为基础版本的强制门槛：

- 多候选 `n`、logprobs/top_logprobs、parallel_tool_calls、user/metadata 等参数贯通。
- OpenRouter provider 路由偏好、models fallback 列表、route/transforms/plugins、扩展采样、多模态输出等字段。
- assistant refusal 的独立字段、tool 结果内非文本 part、reasoning_details、images/audio 输出。Responses 适配器已有拒绝文本转发，不等于这些端到端字段已经实现。

PLAN §2 中公共组件“待接入”、模型管理缺失、仪表盘缺失、行 hover 缺失、无 favicon 等描述已过时：主要页面已接 PageShell/SectionHeader/useApiCall 等，已有最大内容宽度、交互反馈和定制 favicon。仍需补齐全局搜索快捷键、表单边界与单位校验、badge 一致性和真实指标文案；视口与交互验收尚未全面执行，不把源码样式存在等同于体验验收通过。

VS Code 插件本体不在当前仓库中形成完整交付。PLAN §4/4.1 主要提供网关侧接入基础；若“项目完成”还要求可安装插件，需另行明确扩展实现、模型注册/同步、连接管理、凭据保存和发布验收范围，不能因 WS 完成就把插件标为完成。

## 5. 后续实施顺序与验收

以下是建议执行顺序，不是对未完成任务的完成声明，也不改变 PLAN 的既定产品约束。

| 工作包 | 内容与依赖 | 完成标准 |
| --- | --- | --- |
| W1 现有正确性 | BUG-001/002/003/005/006/007/008/009/010/011；BUG-004 先明确治理边界 | 成员数据隔离、降权生效、标准工具历史、候选回退、受控图片访问和真实流式账本有针对性回归 |
| W2 可重复部署 | BUG-012/013/014/015/016/017/018；与 W1 独立部分可并行 | 干净环境单次构建和空目录启动；支持的 feature 零警告；数据库升级/重建政策明确且不会静默丢数据 |
| W3 共享编排与 WS | W1 生命周期修复后按 PLAN §4.A/B/C/D 交付 | 8 路并发归属正确，cancel/断连确实终止上游并结算，坏帧/401/慢消费契约通过，文档与示例可用 |
| W4 设备码登录 | 可独立于 W3；新表依赖 W2 schema 策略 | 10 分钟过期、5s 轮询、主动确认、明文一次领取、旧 vscode Token 吊销均有验证 |
| W5 可观测性收尾 | 复用 W1/W3 终结事件，关闭 BUG-019 | 定价到成本、usage 到 rollup、trace 关联、响应快照和查询 UI 全链路一致 |
| W6 目录导入 | 先修 BUG-020 和契约，再后端、最后 UI | 导入与重导入幂等、引用完整、覆盖正确、Key 保留；两种页面入口均可操作 |
| W7 收尾与交付 | 前端一致性、文档和发布验证；P3 按实际需求排期 | 支持范围、未支持字段和已知限制明确，不能用模拟服务结果替代真实部署验收 |

### 5.1 首批回归清单

- [ ] 延迟末尾 usage 的流式请求：客户端、trace、配额账本均记录真实 100 tokens；错误/取消不误报 success，也不重复结算。
- [ ] 同 Token 并发准入、接近额度上限、同周期首次请求：不丢计数、不产生重复周期记录、不突破已定义的预留约束。
- [ ] 首选路由 503/网络失败：只在允许重试且未输出时切换候选，整个业务请求不重复计费。
- [ ] OpenAI 契约：null/缺失 content 工具历史、多轮工具结果和标准模型字段通过；原有文本/图片路径不回归。
- [ ] 两个成员及管理员：汇总、列表、详情不能越权；降权后旧 Session 的权限按约定失效；免登录设计不变。
- [ ] 干净前端依赖目录、默认/自定义数据目录及明确支持的 feature 组合可以重复验证。
- [ ] WS 契约、设备码一次领取、目录幂等/Key 保留分别随所属功能交付，不等待最后统一补测。

### 5.2 需明确的边界与维护项

- **配额治理**：BUG-004 先决策用户强制预算是否属于产品范围；如果只提供个人自限，应收窄 README 承诺，而不是擅自更改 vscode Token 默认值。
- **数据库升级**：旧 STATUS 曾记录开发期删库重建；这不是正式迁移能力，也不是本次授权执行删库。需明确哪些旧版本允许人工重建、何时起提供保留数据的升级路径；新增 CliSession 不能只依赖忽略 already exists 错误。
- **类型与 CI**：[tests/generate_ts_client.rs](tests/generate_ts_client.rs) 的漂移检查仍被注释；启用时避免生成器重写文件掩盖差异。现有工作流仅覆盖目录发布，后端/前端构建和行为检查应纳入质量门槛。
- **维护债务**：遗留 check_auth/auth_token、应用层关联查询热点、适配器重复 SSE 处理可后续清理；优先围绕实测风险，不以大重构替代功能修复。
- **Key 保护**：上游 Key 当前存于数据库 JSON 中。文件权限、备份访问和加密存储的产品要求需明确，不将加密方案未经决策直接扩展为强制工作。
- **文档交付**：README 的技术栈、OIDC 行为、启动命令、存储路径、PostgreSQL 及前端嵌入机制需与实现一致；当前缺少计划引用的 API/架构/WS 文档，应随接口交付补齐，而非继续链接不存在的文件。

## 6. 验证记录与未验证范围

以下结果来自 2026-09-11 本次会话的先前审查；本次重写 STATUS 只做文档一致性校验，没有重新声称运行全部业务检查。

| 检查 | 结果 | 边界 |
| --- | --- | --- |
| `cargo test --lib --locked --offline -- --skip export_bindings` | 52 通过，0 失败，65 过滤 | 跳过绑定导出避免改写产物；不是全目标/全部 feature/真实上游测试 |
| `cargo build --bin llm-bridge --locked --offline` | 通过 | 当前源码的默认 debug 主程序 |
| `cargo clippy --all-targets --locked --offline -- -D warnings` | 通过 | 默认 features |
| `cargo check --all-targets --features embed-frontend,otel --locked --offline` | 前端产物生成后通过 | 不等于 release 镜像或 OTLP 服务集成验收 |
| 相同生产 feature 组合的 clippy `-D warnings` | 失败：needless_return | BUG-018；不是整个项目无法编译 |
| `pnpm run build` | 干净环境首次失败；先 `pnpm exec vite build` 后完整构建通过 | BUG-014；预热是定位手段，不是正式流程已修复 |
| 本地临时实例 HTTP 冒烟 | 管理配置、Token 创建、mock 非流式对话通过 | OIDC 失败降级也已观察到，按既定设计保留 |
| 工具历史、回退、流式 usage | 分别复现 422、不回退和漏记账本 | BUG-007/009/003 |
| 目录生成器 `bun test` | 4 通过，0 失败，28 个断言 | fixture 覆盖有限，不能证明能力覆盖合并和重复生成正确 |
| Pages 两个 JSON | HTTP 200、schemaVersion=1、无悬空引用 | 仅发布快照，不代表应用侧导入已存在 |

验证中的 rustup 工作目录、pnpm/Bun 沙箱缓存权限、探针转义和 SQLite 外部读取锁问题已与业务故障分开处理，不计入产品 BUG。临时 HTTP 实例使用独立数据和本机 mock，测试后已停止。

仍未验证：真实 IdP 登录全流程、三类真实上游完整兼容性、release/Docker/ARM 交付、PostgreSQL、旧版本升级、并发压力、完整桌面/移动端 UI 交互，以及 README 的内存与二进制体积指标。没有这些结果前，不恢复旧报告中的“全链路已打通”“数据库迁移完成”或“仅剩测试文档”等结论。

## 7. 状态维护规则

修复 BUG 时在本文第 3 节更新问题状态和证据，在第 5、6 节更新验收结果与验证命令；功能验收按工作包更新，不能仅因文件或表字段存在就标为完成。设计变化另行修改 [PLAN.md](PLAN.md)，不能用状态报告静默推翻用户决策。

本版替换旧的 Svelte、7 张表、历史 Phase 百分比和已失效任务总线。**当前收尾目标是：已有功能正确、部署可重复、WS/设备码/目录导入实际可用、可观测性数据可信。** 完成这些后，再按单独确认的范围评估 VS Code 插件和低频兼容能力。


- ✅ `cargo check --lib` — 编译通过
