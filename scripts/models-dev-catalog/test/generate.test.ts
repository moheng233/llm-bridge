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

function write(rel: string, content: string): void {
  const full = join(fixtureDir, rel);
  mkdirSync(join(full, ".."), { recursive: true });
  writeFileSync(full, content);
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
  // git init 以提供 rev-parse HEAD
  execFileSync("git", ["init", "-q"], { cwd: fixtureDir });
  execFileSync("git", ["add", "-A"], { cwd: fixtureDir });
  execFileSync("git", ["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "fixture"], { cwd: fixtureDir });

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
