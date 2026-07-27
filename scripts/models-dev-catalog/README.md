# models-dev-catalog

从 `anomalyco/models.dev`（`dev` 分支）的三层 TOML 源生成 llm-bridge 导入契约 JSON。

## 产物

| 文件 | 说明 |
|------|------|
| `dist/catalog.json` | 三层目录：`models[]`（provider 无关标称数据）、`providers[]`（含 `api` 端点的可直连 provider + compat 映射）、`links[]`（provider ↔ 规范模型，含价格） |
| `dist/contract.json` | schema 契约：`schemaVersion` + 字段清单 + **显式记录的丢弃字段**（`cache_write`/`reasoning` 等无目标列，不静默简化） |

## 语义要点

- **只导出带 `base_model` 的 provider model**——这是官方外键，无歧义地关联回 `models/{family}/{id}` 规范模型；无 `base_model` 的条目（多为聚合商镜像）**跳过并 warn**，不猜测映射。
- **只导出含 `api` 的 provider**——无 `api` 的 provider（openai/anthropic 等 SDK 内置端点）无法映射到 `ProviderProtocol.base_url`，整体跳过；其模型经由 aggregator 的 link 间接入库。
- **字段映射**：`modelName = base_model`（如 `openai/gpt-5`）；`providerModelId` = provider 侧原始 id；`maxInputTokens = limit.input ?? limit.context ?? 4096`；`vision = modalities.input 含 image`；`thinking = reasoning`；`adaptiveThinking = false`；`compat` = npm 含 `anthropic` → `anthropicMessages` 否则 `openAiChatCompletions`；`enabled = status != "deprecated"`。

## 用法

```bash
bun install
bun src/generate.ts [--out dist] [--source <git-url>] [--ref dev]
```

环境变量：

- `SOURCE_DIR` — 跳过 git clone，直接用本地源仓库目录（测试用）
- `OUT_DIR` — 输出目录（默认 `dist/`）

## 测试

```bash
bun test        # fixture 驱动的端到端（构造源 TOML → 断言产物）
bun x tsc --noEmit
```

## 依赖

- 运行时：`smol-toml`（TOML v1.1 解析，toml-test 合规）
- 无其他运行时依赖；`git` 需可用（clone 源仓库）

## CI

由 `.github/workflows/models-dev-catalog.yml` 定时（每日）+ 手动触发，生成后发布到 `gh-pages` 分支，经 GitHub Pages 公开为 `https://moheng233.github.io/llm-bridge/catalog.json`。
