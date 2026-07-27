//! anomalyco/models.dev (dev 分支) 三层 TOML → llm-bridge 导入契约 JSON 生成器。
//!
//! 用法：
//!   bun src/generate.ts [--out <dir>] [--source <git-url>] [--ref <git-ref>]
//!
//! 环境变量：
//!   SOURCE_DIR  — 跳过 git clone，直接使用本地源仓库目录（测试用）
//!   OUT_DIR     — 输出目录（默认 dist/，--out 优先）
//!
//! 产物：
//!   <out>/catalog.json   — 三层目录（models / providers / links）
//!   <out>/contract.json  — schema 契约（版本 + 字段清单）

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { parse as parseToml } from "smol-toml";

/** 解析结果 = 任意嵌套的 TOML 表。字段类型在消费点用 asString/asTable 等逐项校验。 */
type TomlValue = string | number | boolean | Date | TomlValue[] | { [key: string]: TomlValue };
type TomlTable = { [key: string]: TomlValue };

const SCHEMA_VERSION = 1 as const;
const DEFAULT_SOURCE = "https://github.com/anomalyco/models.dev.git";
const DEFAULT_REF = "dev";

// ---------- 源 schema 类型（仅声明我们消费的字段，其余字段保留但不用） ----------

interface SourceModelLimit {
  context?: number;
  input?: number;
  output?: number;
}

interface SourceModelModalities {
  input?: string[];
  output?: string[];
}

interface SourceModel {
  name: string;
  family?: string;
  attachment?: boolean;
  reasoning?: boolean;
  tool_call?: boolean;
  structured_output?: boolean;
  temperature?: boolean;
  open_weights?: boolean;
  description?: string;
  limit?: SourceModelLimit;
  modalities?: SourceModelModalities;
}

interface SourceProvider {
  name: string;
  npm?: string;
  env?: string[];
  doc?: string;
  api?: string;
}

interface SourceCost {
  input?: number;
  output?: number;
  reasoning?: number;
  cache_read?: number;
  cache_write?: number;
  input_audio?: number;
  output_audio?: number;
}

interface SourceProviderModel {
  base_model?: string;
  base_model_omit?: string[];
  cost?: SourceCost;
  limit?: SourceModelLimit;
  modalities?: SourceModelModalities;
  status?: string;
}

// ---------- 派生 JSON 契约类型 ----------

interface CatalogModel {
  modelName: string;
  displayName: string;
  description?: string;
  maxInputTokens: number;
  maxOutputTokens: number;
  toolCalling: boolean;
  vision: boolean;
  thinking: boolean;
  adaptiveThinking: false;
}

type ProviderCompat = "openAiChatCompletions" | "openAiResponses" | "anthropicMessages";

interface CatalogProvider {
  providerId: string;
  displayName: string;
  baseUrl: string;
  compat: ProviderCompat;
}

interface CatalogLink {
  providerId: string;
  protocolKey: string;
  modelName: string;
  providerModelId: string;
  inputPricePer1m?: number;
  outputPricePer1m?: number;
  cacheReadPricePer1m?: number;
  enabled: boolean;
}

interface Catalog {
  generatedAt: string;
  sourceRev: string;
  schemaVersion: typeof SCHEMA_VERSION;
  models: CatalogModel[];
  providers: CatalogProvider[];
  links: CatalogLink[];
}

// ---------- 辅助 ----------

function asString(v: unknown, ctx: string): string | undefined {
  if (v === undefined) return undefined;
  if (typeof v !== "string") throw new Error(`${ctx}: 期望 string，实际 ${typeof v}`);
  return v;
}

function asNumber(v: unknown, ctx: string): number | undefined {
  if (v === undefined) return undefined;
  if (typeof v !== "number") throw new Error(`${ctx}: 期望 number，实际 ${typeof v}`);
  return v;
}

function asBool(v: unknown, ctx: string): boolean | undefined {
  if (v === undefined) return undefined;
  if (typeof v !== "boolean") throw new Error(`${ctx}: 期望 boolean，实际 ${typeof v}`);
  return v;
}

function asStringArray(v: unknown, ctx: string): string[] | undefined {
  if (v === undefined) return undefined;
  if (!Array.isArray(v) || !v.every((x) => typeof x === "string")) {
    throw new Error(`${ctx}: 期望 string[]`);
  }
  return v as string[];
}

function asTable<T extends object>(v: unknown, ctx: string): T | undefined {
  if (v === undefined) return undefined;
  if (typeof v !== "object" || v === null || Array.isArray(v)) {
    throw new Error(`${ctx}: 期望 table`);
  }
  return v as T;
}

/** 递归列出目录下所有 .toml 文件（相对 root 的 posix 路径，排序保证确定性）。 */
function listTomlFiles(root: string, prefix = ""): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(root).sort()) {
    const full = join(root, entry);
    const rel = prefix === "" ? entry : `${prefix}/${entry}`;
    if (statSync(full).isDirectory()) {
      out.push(...listTomlFiles(full, rel));
    } else if (entry.endsWith(".toml")) {
      out.push(rel);
    }
  }
  return out;
}

function cloneLimit(l: SourceModelLimit | undefined): SourceModelLimit {
  return l ? { ...l } : {};
}

function cloneModalities(m: SourceModelModalities | undefined): SourceModelModalities {
  return m ? { input: m.input ? [...m.input] : undefined, output: m.output ? [...m.output] : undefined } : {};
}

function omitPath(table: TomlTable, dotPath: string): void {
  const parts = dotPath.split(".");
  let cur: TomlTable = table;
  for (let i = 0; i < parts.length - 1; i++) {
    const next: TomlValue | undefined = cur[parts[i]!];
    if (typeof next !== "object" || next === null || next instanceof Date || Array.isArray(next)) return;
    cur = next;
  }
  delete cur[parts[parts.length - 1]!];
}

function mapCompat(npm: string | undefined): ProviderCompat {
  if (npm !== undefined && npm.includes("anthropic")) return "anthropicMessages";
  return "openAiChatCompletions";
}

// ---------- 主流程 ----------

function main(): void {
  const args = process.argv.slice(2);
  const argValue = (flag: string): string | undefined => {
    const i = args.indexOf(flag);
    return i >= 0 ? args[i + 1] : undefined;
  };

  const outDir = resolve(argValue("--out") ?? process.env.OUT_DIR ?? "dist");
  const sourceUrl = argValue("--source") ?? DEFAULT_SOURCE;
  const sourceRef = argValue("--ref") ?? DEFAULT_REF;
  const sourceDirOverride = process.env.SOURCE_DIR;

  let sourceDir: string;
  let cleanupDir: string | null = null;
  if (sourceDirOverride !== undefined && sourceDirOverride !== "") {
    sourceDir = resolve(sourceDirOverride);
  } else {
    cleanupDir = mkdtempSync(join(tmpdir(), "models-dev-src-"));
    execFileSync(
      "git",
      ["clone", "--depth", "1", "--filter=blob:none", "--sparse", "--branch", sourceRef, sourceUrl, cleanupDir],
      { stdio: "inherit" },
    );
    execFileSync("git", ["sparse-checkout", "set", "models", "providers"], { cwd: cleanupDir, stdio: "inherit" });
    sourceDir = cleanupDir;
  }

  try {
    const sourceRev = execFileSync("git", ["rev-parse", "HEAD"], { cwd: sourceDir, encoding: "utf8" }).trim();

    // ----- 1. 读取 models/（family 级目录：models/{family}/{model}.toml） -----
    const modelsRoot = join(sourceDir, "models");
    const models = new Map<string, SourceModel>();
    for (const rel of listTomlFiles(modelsRoot)) {
      const parsed = parseToml(readFileSync(join(modelsRoot, rel), "utf8"));
      const modelName = rel.replace(/\.toml$/, "");
      const ctx = `models/${rel}`;
      const name = asString(parsed.name, `${ctx}.name`);
      if (name === undefined) throw new Error(`${ctx}: 缺少必填字段 name`);
      models.set(modelName, {
        ...parsed,
        name,
        limit: asTable(parsed.limit, `${ctx}.limit`) as SourceModelLimit | undefined,
        modalities: asTable(parsed.modalities, `${ctx}.modalities`) as SourceModelModalities | undefined,
      } as SourceModel);
    }

    // ----- 2. 读取 providers/ -----
    const providersRoot = join(sourceDir, "providers");
    const providers: CatalogProvider[] = [];
    const links: CatalogLink[] = [];

    for (const providerId of readdirSync(providersRoot).sort()) {
      const providerDir = join(providersRoot, providerId);
      if (!statSync(providerDir).isDirectory()) continue;

      const providerTomlPath = join(providerDir, "provider.toml");
      const pctx = `providers/${providerId}/provider.toml`;
      const providerParsed = parseToml(readFileSync(providerTomlPath, "utf8")) as unknown as SourceProvider;
      const displayName = asString(providerParsed.name, `${pctx}.name`);
      if (displayName === undefined) throw new Error(`${pctx}: 缺少必填字段 name`);
      const npm = asString(providerParsed.npm, `${pctx}.npm`);
      const baseUrl = asString(providerParsed.api, `${pctx}.api`);
      if (baseUrl === undefined) {
        // 无 api 的 provider 不是 openai-compatible 端点，无法映射到 ProviderProtocol.base_url —— 整体跳过
        // （这些 provider 多为云厂商托管平台，端点由 SDK 内部决定，llm-bridge 无法直连）
        console.warn(`warn: provider ${providerId} 无 api 字段，跳过（含其全部 models）`);
        continue;
      }
      const compat = mapCompat(npm);
      providers.push({ providerId, displayName, baseUrl, compat });
      const protocolKey = `${compat}|${baseUrl}`;

      // ----- 3. provider 的 models/ -----
      const providerModelsRoot = join(providerDir, "models");
      let providerModelFiles: string[] = [];
      try {
        providerModelFiles = listTomlFiles(providerModelsRoot);
      } catch {
        providerModelFiles = [];
      }

      for (const rel of providerModelFiles) {
        const providerModelId = rel.replace(/\.toml$/, "");
        const mctx = `providers/${providerId}/models/${rel}`;
        const parsed = parseToml(readFileSync(join(providerModelsRoot, rel), "utf8")) as unknown as SourceProviderModel;
        const baseModelRef = asString(parsed.base_model, `${mctx}.base_model`);
        if (baseModelRef === undefined) {
          console.warn(`warn: ${mctx} 无 base_model，跳过`);
          continue;
        }
        const base = models.get(baseModelRef);
        if (base === undefined) throw new Error(`${mctx}: base_model 引用了不存在的 model "${baseModelRef}"`);

        // 合并：base_model 元数据为底，provider 本地字段覆盖；base_model_omit 删除继承字段
        const merged = {
          ...base,
          ...parsed,
          limit: { ...cloneLimit(base.limit), ...cloneLimit(parsed.limit) },
          modalities: (() => {
            const b = cloneModalities(base.modalities);
            const p = parsed.modalities;
            return { input: p?.input ?? b.input, output: p?.output ?? b.output };
          })(),
        } as SourceModel & SourceProviderModel;
        for (const omit of parsed.base_model_omit ?? []) omitPath(merged as unknown as TomlTable, omit);

        const cost = parsed.cost as SourceCost | undefined;
        const status = asString(parsed.status, `${mctx}.status`);

        links.push({
          providerId,
          protocolKey,
          modelName: baseModelRef,
          providerModelId,
          ...(cost?.input !== undefined ? { inputPricePer1m: cost.input } : {}),
          ...(cost?.output !== undefined ? { outputPricePer1m: cost.output } : {}),
          ...(cost?.cache_read !== undefined ? { cacheReadPricePer1m: cost.cache_read } : {}),
          enabled: status !== "deprecated",
        });
      }
    }

    // ----- 4. models 输出（只保留被至少一条 link 引用的模型，避免导出无链接孤岛） -----
    const referencedModelNames = new Set(links.map((l) => l.modelName));
    const catalogModels: CatalogModel[] = [...referencedModelNames].sort().map((modelName) => {
      const m = models.get(modelName)!;
      const ctx = `models/${modelName}.toml`;
      return {
        modelName,
        displayName: m.name,
        ...(m.description !== undefined ? { description: m.description } : {}),
        maxInputTokens: m.limit?.input ?? m.limit?.context ?? 4096,
        maxOutputTokens: m.limit?.output ?? 4096,
        toolCalling: m.tool_call ?? false,
        vision: (m.modalities?.input ?? []).includes("image"),
        thinking: m.reasoning ?? false,
        adaptiveThinking: false,
      };
      void ctx;
    });

    // ----- 5. 引用完整性校验 -----
    const providerIds = new Set(providers.map((p) => p.providerId));
    for (const link of links) {
      if (!providerIds.has(link.providerId)) {
        throw new Error(`link 引用了不存在的 provider "${link.providerId}"`);
      }
    }

    const catalog: Catalog = {
      generatedAt: new Date().toISOString(),
      sourceRev,
      schemaVersion: SCHEMA_VERSION,
      models: catalogModels,
      providers,
      links,
    };

    const contract = {
      schemaVersion: SCHEMA_VERSION,
      source: { repo: "anomalyco/models.dev", ref: sourceRef },
      fields: {
        models: [
          "modelName", "displayName", "description?", "maxInputTokens", "maxOutputTokens",
          "toolCalling", "vision", "thinking", "adaptiveThinking",
        ],
        providers: ["providerId", "displayName", "baseUrl", "compat"],
        links: [
          "providerId", "protocolKey", "modelName", "providerModelId",
          "inputPricePer1m?", "outputPricePer1m?", "cacheReadPricePer1m?", "enabled",
        ],
      },
      droppedSourceFields: {
        reason: "llm-bridge 目标表无对应列，显式丢弃（不静默简化：此处记录缺口）",
        model: ["family", "release_date", "last_updated", "knowledge", "attachment", "structured_output", "temperature", "open_weights", "license", "links", "weights", "benchmarks"],
        cost: ["reasoning", "cache_write", "input_audio", "output_audio"],
        providerModel: ["status（映射为 enabled = status != 'deprecated'）"],
      },
    };

    mkdirSync(outDir, { recursive: true });
    writeFileSync(join(outDir, "catalog.json"), JSON.stringify(catalog, null, 2) + "\n");
    writeFileSync(join(outDir, "contract.json"), JSON.stringify(contract, null, 2) + "\n");

    console.log(
      `ok: models=${catalogModels.length} providers=${providers.length} links=${links.length} sourceRev=${sourceRev.slice(0, 12)} → ${outDir}`,
    );
  } finally {
    if (cleanupDir !== null) rmSync(cleanupDir, { recursive: true, force: true });
  }
}

main();
