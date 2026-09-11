import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

let fixtureDir: string;
let outDir: string;

const MODEL_GPT = `
name = "GPT-5.4"
family = "gpt"
reasoning = true
tool_call = true
attachment = true

[limit]
context = 400_000
input = 272_000
output = 128_000

[modalities]
input = ["text", "image"]
output = ["text"]
`;

const PROVIDER_OPENAI = `
name = "OpenAI"
npm = "@ai-sdk/openai"
env = ["OPENAI_API_KEY"]
api = "https://api.openai.com/v1"
doc = "https://platform.openai.com/docs"
`;

const PROVIDER_MODEL_GPT = `
base_model = "openai/gpt-5.4"

[cost]
input = 1.25
output = 10.00
cache_read = 0.125
cache_write = 1.25

[limit]
output = 32_000
`;

const PROVIDER_ANTHROPIC = `
name = "Anthropic"
npm = "@ai-sdk/anthropic"
env = ["ANTHROPIC_API_KEY"]
api = "https://api.anthropic.com"
doc = "https://docs.anthropic.com"
`;

const PROVIDER_MODEL_CLAUDE = `
base_model = "openai/gpt-5.4"
status = "deprecated"

[cost]
input = 5.00
output = 25.00
`;

const PROVIDER_NOAPI = `
name = "Bedrock"
npm = "@ai-sdk/amazon-bedrock"
env = ["AWS_REGION"]
doc = "https://aws.amazon.com"
`;

function write(rel: string, content: string, dir: string = fixtureDir): void {
  const full = join(dir, rel);
  mkdirSync(join(full, ".."), { recursive: true });
  writeFileSync(full, content);
}

/** 在 fixture 内完成 git init + commit，全部通过 -c/环境变量注入身份，不依赖全局 git 配置（bun 沙箱限制）。 */
function gitCommitFixture(dir: string): void {
  const env = {
    ...process.env,
    GIT_AUTHOR_NAME: "t",
    GIT_AUTHOR_EMAIL: "t@t",
    GIT_COMMITTER_NAME: "t",
    GIT_COMMITTER_EMAIL: "t@t",
    HOME: dir,
    XDG_CONFIG_HOME: dir,
  };
  execFileSync("git", ["init", "-q"], { cwd: dir, env });
  execFileSync("git", ["add", "-A"], { cwd: dir, env });
  execFileSync("git", ["-c", "commit.gpgsign=false", "commit", "-qm", "fixture"], { cwd: dir, env });
}

beforeAll(() => {
  fixtureDir = mkdtempSync(join(tmpdir(), "models-dev-fixture-"));
  outDir = mkdtempSync(join(tmpdir(), "models-dev-out-"));
  write("models/openai/gpt-5.4.toml", MODEL_GPT);
  write("providers/openai/provider.toml", PROVIDER_OPENAI);
  write("providers/openai/models/gpt-5.4.toml", PROVIDER_MODEL_GPT);
  write("providers/anthropic/provider.toml", PROVIDER_ANTHROPIC);
  write("providers/anthropic/models/claude-opus-4-8.toml", PROVIDER_MODEL_CLAUDE);
  write("providers/bedrock/provider.toml", PROVIDER_NOAPI);
  // git init 以提供 rev-parse HEAD（身份经环境变量注入，不依赖全局配置）
  gitCommitFixture(fixtureDir);

  execFileSync("bun", ["src/generate.ts", "--out", outDir], {
    cwd: join(import.meta.dir, ".."),
    env: { ...process.env, SOURCE_DIR: fixtureDir },
    stdio: "inherit",
  });
});

afterAll(() => {
  rmSync(fixtureDir, { recursive: true, force: true });
  rmSync(outDir, { recursive: true, force: true });
});

function readCatalog(): {
  generatedAt: string;
  sourceRev: string;
  schemaVersion: number;
  models: Array<Record<string, unknown>>;
  providers: Array<Record<string, unknown>>;
  links: Array<Record<string, unknown>>;
} {
  return JSON.parse(readFileSync(join(outDir, "catalog.json"), "utf8"));
}

describe("generate", () => {
  test("models：base_model 元数据 + 覆盖合并", () => {
    const c = readCatalog();
    expect(c.schemaVersion).toBe(1);
    expect(c.models).toHaveLength(1);
    const m = c.models[0]!;
    expect(m.modelName).toBe("openai/gpt-5.4");
    expect(m.displayName).toBe("GPT-5.4");
    expect(m.maxInputTokens).toBe(272000); // limit.input 优先于 context
    expect(m.maxOutputTokens).toBe(128000); // 来自 base_model（provider 覆盖在 link 层，不影响 model 标称）
    expect(m.toolCalling).toBe(true);
    expect(m.vision).toBe(true);
    expect(m.thinking).toBe(true);
    expect(m.adaptiveThinking).toBe(false);
  });

  test("providers：含 api 才导出，compat 按 npm 映射，bedrock 无 api 被跳过", () => {
    const c = readCatalog();
    expect(c.providers).toHaveLength(2);
    const openai = c.providers.find((p) => p.providerId === "openai")!;
    expect(openai.baseUrl).toBe("https://api.openai.com/v1");
    expect(openai.compat).toBe("openAiChatCompletions");
    const anthropic = c.providers.find((p) => p.providerId === "anthropic")!;
    expect(anthropic.compat).toBe("anthropicMessages");
    expect(c.providers.find((p) => p.providerId === "bedrock")).toBeUndefined();
  });

  test("links：价格透传、cache_write 丢弃、deprecated → enabled=false", () => {
    const c = readCatalog();
    expect(c.links).toHaveLength(2);

    const l1 = c.links.find((l) => l.providerId === "openai")!;
    expect(l1.protocolKey).toBe("openAiChatCompletions|https://api.openai.com/v1");
    expect(l1.modelName).toBe("openai/gpt-5.4");
    expect(l1.providerModelId).toBe("gpt-5.4");
    expect(l1.inputPricePer1m).toBe(1.25);
    expect(l1.outputPricePer1m).toBe(10);
    expect(l1.cacheReadPricePer1m).toBe(0.125);
    expect(l1.cacheWritePricePer1m).toBeUndefined(); // 无目标列，显式丢弃
    expect(l1.enabled).toBe(true);

    const l2 = c.links.find((l) => l.providerId === "anthropic")!;
    expect(l2.providerModelId).toBe("claude-opus-4-8");
    expect(l2.enabled).toBe(false); // status = deprecated
  });

  test("contract.json：schemaVersion 与缺口记录", () => {
    const contract = JSON.parse(readFileSync(join(outDir, "contract.json"), "utf8"));
    expect(contract.schemaVersion).toBe(1);
    expect(contract.droppedSourceFields.cost).toContain("cache_write");
  });
});

// ── BUG-020：提供者覆盖/omit 输出到 link 关联，不污染模型标称 ──

describe("generate overrides (BUG-020)", () => {
  let fxDir: string;
  let outOverride: string;
  function generateOverride() {
    execFileSync("bun", ["src/generate.ts", "--out", outOverride], {
      cwd: join(import.meta.dir, ".."), env: { ...process.env, SOURCE_DIR: fxDir }, stdio: "pipe",
    });
    return JSON.parse(readFileSync(join(outOverride, "catalog.json"), "utf8")) as {
      models: Array<Record<string, unknown>>; providers: Array<Record<string, unknown>>;
      links: Array<Record<string, unknown>>;
    };
  }
  beforeAll(() => {
    fxDir = mkdtempSync(join(tmpdir(), "models-dev-ov-"));
    outOverride = mkdtempSync(join(tmpdir(), "models-dev-ov-out-"));
    write("models/acme/alpha.toml", `
name = "Alpha"
tool_call = true
reasoning = true
[limit]
context = 100_000
input = 80_000
output = 40_000
[modalities]
input = ["text"]
output = ["text"]
`, fxDir);
    write("providers/ov/provider.toml", PROVIDER_OPENAI, fxDir);
    write("providers/ov/models/alpha.toml", `
base_model = "acme/alpha"
tool_call = false
[limit]
output = 8_000
[modalities]
input = ["text", "image"]
`, fxDir);
    write("providers/ov/models/beta.toml", `
base_model = "acme/alpha"
base_model_omit = ["limit.input", "reasoning", "tool_call"]
`, fxDir);
    write("providers/ov/models/gamma.toml", `
base_model = "acme/alpha"
base_model_omit = ["limit.input"]
[limit]
context = 32_000
`, fxDir);
    write("providers/ov/models/delta.toml", `
base_model = "acme/alpha"
base_model_omit = ["limit", "tool_call", "reasoning"]
`, fxDir);
    gitCommitFixture(fxDir);
  });
  afterAll(() => {
    rmSync(fxDir, { recursive: true, force: true });
    rmSync(outOverride, { recursive: true, force: true });
  });
  test("覆盖和 omit 改变关联有效能力但不污染标称能力", () => {
    const c = generateOverride();
    const model = c.models.find((m) => m.modelName === "acme/alpha")!;
    expect(model).toMatchObject({ maxInputTokens: 80000, maxOutputTokens: 40000, vision: false, thinking: true, toolCalling: true });
    const effective = (id: string) => {
      const link = c.links.find((l) => l.providerModelId === id)!;
      return Object.fromEntries(["maxInputTokens", "maxOutputTokens", "vision", "thinking", "toolCalling"].map((key) => [key, link[key] ?? model[key]]));
    };
    expect(effective("alpha")).toMatchObject({ maxInputTokens: 80000, maxOutputTokens: 8000, vision: true, toolCalling: false });
    expect(effective("beta")).toMatchObject({ maxInputTokens: 100000, thinking: false, toolCalling: false });
    expect(effective("gamma")).toMatchObject({ maxInputTokens: 32000 });
    expect(effective("delta")).toMatchObject({ maxInputTokens: 4096, maxOutputTokens: 4096, thinking: false, toolCalling: false });
  });
  test("重复生成保留全部上游别名且业务内容不变", () => {
    const first = generateOverride();
    const second = generateOverride();
    expect(second.models).toEqual(first.models);
    expect(second.providers).toEqual(first.providers);
    expect(second.links).toEqual(first.links);
    expect(second.links.map((link) => link.providerModelId).sort()).toEqual(["alpha", "beta", "delta", "gamma"]);
  });

  test("base_model 引用不存在的 model → 报错（坏引用断言）", () => {
    const badDir = mkdtempSync(join(tmpdir(), "models-dev-badref-"));
    const badOut = mkdtempSync(join(tmpdir(), "models-dev-badref-out-"));
    try {
      write("models/acme/alpha.toml", `name = "Alpha"\nfamily = "acme"\n`, badDir);
      write("providers/ov/provider.toml", PROVIDER_OPENAI, badDir);
      write("providers/ov/models/ghost.toml", `base_model = "acme/nonexistent"\n`, badDir);
      gitCommitFixture(badDir);
      expect(() =>
        execFileSync("bun", ["src/generate.ts", "--out", badOut], {
          cwd: join(import.meta.dir, ".."),
          env: { ...process.env, SOURCE_DIR: badDir },
          stdio: "pipe",
        }),
      ).toThrow();
    } finally {
      rmSync(badDir, { recursive: true, force: true });
      rmSync(badOut, { recursive: true, force: true });
    }
  });
});
