<script lang="ts">
  import { createApiClient } from "$bindings/client";
  import type {
    ProviderResponse,
    CreateProviderRequest,
    UpdateProviderRequest,
    ProviderCompatibility,
    ProviderCompatConfig,
    CompatibilitySettings,
  } from "$bindings";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as Accordion from "$lib/components/ui/accordion/index.js";

  const api = createApiClient({ baseUrl: "", credentials: "include" });

  const COMPAT_OPTIONS: { value: ProviderCompatibility; label: string; badgeClass: string }[] = [
    { value: "open_ai_responses", label: "OpenAI Responses", badgeClass: "bg-emerald-100 text-emerald-700 hover:bg-emerald-100" },
    { value: "open_ai_chat_completions", label: "OpenAI Chat", badgeClass: "bg-teal-100 text-teal-700 hover:bg-teal-100" },
    { value: "anthropic_messages", label: "Anthropic Messages", badgeClass: "bg-orange-100 text-orange-700 hover:bg-orange-100" },
  ];

  function defaultCompatibilities(): Record<ProviderCompatibility, ProviderCompatConfig> {
    return {
      open_ai_responses: { enabled: false, settings: null },
      open_ai_chat_completions: { enabled: false, settings: null },
      anthropic_messages: { enabled: false, settings: null },
    };
  }

  let providers = $state<ProviderResponse[]>([]);
  let loading = $state(true);
  let error = $state("");
  let dialogOpen = $state(false);
  let editing = $state<string | null>(null);
  let formError = $state("");

  let formProviderName = $state("");
  let formCompatibilities = $state<Record<ProviderCompatibility, ProviderCompatConfig>>(defaultCompatibilities());
  let formBaseUrl = $state("");
  let formApiKey = $state("");

  async function load() {
    loading = true;
    error = "";
    try {
      providers = await api.providers.listProviders();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  $effect(() => { load(); });

  function resetForm() {
    formProviderName = "";
    formCompatibilities = defaultCompatibilities();
    formBaseUrl = "";
    formApiKey = "";
    dialogOpen = false;
    editing = null;
    formError = "";
  }

  function startEdit(p: ProviderResponse) {
    editing = p.provider_name;
    formProviderName = p.provider_name;
    formCompatibilities = { ...defaultCompatibilities(), ...p.compatibilities };
    formBaseUrl = p.base_url ?? "";
    formApiKey = "";
    dialogOpen = true;
    formError = "";
  }

  function toggleCompat(compat: ProviderCompatibility) {
    const current = formCompatibilities[compat];
    formCompatibilities = {
      ...formCompatibilities,
      [compat]: { enabled: !current.enabled, settings: current.settings },
    };
  }

  function updateCompatSettings(compat: ProviderCompatibility, settings: CompatibilitySettings) {
    formCompatibilities = {
      ...formCompatibilities,
      [compat]: { ...formCompatibilities[compat], settings },
    };
  }

  function buildCompatPayload(): Record<ProviderCompatibility, ProviderCompatConfig> {
    const result: Record<string, ProviderCompatConfig> = {};
    for (const opt of COMPAT_OPTIONS) {
      const cfg = formCompatibilities[opt.value];
      if (cfg.enabled) {
        result[opt.value] = cfg;
      }
    }
    return result as Record<ProviderCompatibility, ProviderCompatConfig>;
  }

  async function handleSubmit() {
    formError = "";
    const compatibilities = buildCompatPayload();
    if (Object.keys(compatibilities).length === 0) {
      formError = "至少需要启用一种兼容性协议";
      return;
    }
    try {
      if (editing) {
        await api.providers.updateProvider(editing, {
          compatibilities,
          baseUrl: formBaseUrl || null,
          apiKey: formApiKey,
        });
      } else {
        await api.providers.createProvider({
          providerName: formProviderName,
          compatibilities,
          baseUrl: formBaseUrl || null,
          apiKey: formApiKey,
        });
      }
      resetForm();
      await load();
    } catch (e: any) {
      formError = e.message;
    }
  }

  async function handleDelete(name: string) {
    try {
      await api.providers.deleteProvider(name);
      await load();
    } catch (e: any) {
      error = e.message;
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <section
    class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border bg-background shadow-sm"
  >
    <div
      class="flex shrink-0 flex-col gap-3 border-b px-4 py-3 lg:flex-row lg:items-center lg:justify-between"
    >
      <div class="flex items-center gap-3">
        <h2 class="text-xl font-semibold">提供者管理</h2>
        <Button onclick={() => { resetForm(); dialogOpen = true; }}>
          + 新建提供者
        </Button>
      </div>
    </div>

    <Dialog.Root bind:open={dialogOpen}>
      <Dialog.Content class="sm:max-w-xl">
        <Dialog.Header>
          <Dialog.Title>{editing ? "编辑提供者" : "新建提供者"}</Dialog.Title>
          <Dialog.Description>
            {editing ? "修改提供者的配置信息" : "添加一个新的 API 提供者"}
          </Dialog.Description>
        </Dialog.Header>
        {#if formError}
          <Alert variant="destructive">
            <AlertDescription>{formError}</AlertDescription>
          </Alert>
        {/if}
        <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }} class="space-y-4">
          <div class="space-y-2">
            <Label for="provider-name">名称</Label>
            <Input id="provider-name" bind:value={formProviderName} disabled={!!editing} required />
          </div>

          <div class="space-y-2">
            <Label>兼容性协议</Label>
            <div class="space-y-3 rounded-md border p-3">
              {#each COMPAT_OPTIONS as opt}
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-2">
                    <Checkbox
                      checked={formCompatibilities[opt.value]?.enabled ?? false}
                      onCheckedChange={() => toggleCompat(opt.value)}
                    />
                    <span class="text-sm">{opt.label}</span>
                  </div>
                  {#if formCompatibilities[opt.value]?.enabled}
                    <Badge class="{opt.badgeClass}">已启用</Badge>
                  {/if}
                </div>
                {#if formCompatibilities[opt.value]?.enabled}
                  <CompatSettingsPanel
                    compat={opt.value}
                    settings={formCompatibilities[opt.value]?.settings ?? { pathSuffix: null, customHeaders: {}, customParams: {} }}
                    onChange={(s) => updateCompatSettings(opt.value, s)}
                  />
                {/if}
              {/each}
            </div>
          </div>

          <div class="space-y-2">
            <Label for="base-url">自定义 Base URL（可选）</Label>
            <Input id="base-url" bind:value={formBaseUrl} placeholder="留空使用默认地址" />
          </div>
          <div class="space-y-2">
            <Label for="api-key">API Key</Label>
            <Input id="api-key" type="password" bind:value={formApiKey} required />
          </div>
          <Dialog.Footer>
            <Button type="button" variant="outline" onclick={resetForm}>取消</Button>
            <Button type="submit">{editing ? "保存修改" : "创建"}</Button>
          </Dialog.Footer>
        </form>
      </Dialog.Content>
    </Dialog.Root>

    <div class="min-h-0 flex-1 overflow-auto [scrollbar-gutter:stable]">
      {#if loading}
        <div class="flex items-center justify-center py-16">
          <Spinner class="size-8" />
        </div>
      {:else if error}
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      {:else if providers.length === 0}
        <div class="rounded-lg border border-dashed py-12 text-center text-sm text-muted-foreground">
          暂无提供者，点击上方按钮创建
        </div>
      {:else}
        <div class="grid grid-cols-1 gap-4 p-4 md:grid-cols-2">
          {#each providers as p, rowIndex}
            <Card.Root style="animation: row-enter 220ms ease both; animation-delay: {Math.min(rowIndex * 20, 480)}ms;">
              <Card.Header>
                <div class="flex items-start justify-between">
                  <div>
                    <Card.Title class="text-base">{p.provider_name}</Card.Title>
                    <div class="mt-1 flex flex-wrap gap-1">
                      {#each COMPAT_OPTIONS as opt}
                        {#if p.compatibilities[opt.value]?.enabled}
                          <Badge class="{opt.badgeClass}">{opt.label}</Badge>
                        {/if}
                      {/each}
                    </div>
                  </div>
                  <div class="flex gap-1">
                    <Button variant="ghost" size="sm" onclick={() => startEdit(p)}>✏️</Button>
                    <Button variant="ghost" size="sm" onclick={() => handleDelete(p.provider_name)}>🗑️</Button>
                  </div>
                </div>
              </Card.Header>
              <Card.Content>
                <dl class="space-y-1.5 text-xs text-muted-foreground">
                  {#if p.base_url}
                    <div class="flex gap-2">
                      <dt class="shrink-0 font-medium">URL</dt>
                      <dd class="truncate font-mono">{p.base_url}</dd>
                    </div>
                  {/if}
                  <div class="flex gap-2">
                    <dt class="shrink-0 font-medium">密钥</dt>
                    <dd class="font-mono">已配置</dd>
                  </div>
                </dl>
              </Card.Content>
            </Card.Root>
          {/each}
        </div>
      {/if}
    </div>
  </section>
</div>

<script lang="ts" context="module">
  // CompatSettingsPanel component
</script>

<script lang="ts">
  // Inline component for compatibility settings panel
  let compat: ProviderCompatibility = $bindable();
  let settings: CompatibilitySettings = $bindable();
  let onChange: (s: CompatibilitySettings) => void = $bindable();

  function updateField(field: keyof CompatibilitySettings, value: any) {
    const newSettings = { ...settings, [field]: value };
    onChange(newSettings);
  }

  function parseHeaders(value: string): Record<string, string> {
    const headers: Record<string, string> = {};
    for (const line of value.split("\n").filter((l) => l.trim())) {
      const idx = line.indexOf(":");
      if (idx > 0) {
        headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
      }
    }
    return headers;
  }

  function serializeHeaders(headers: Record<string, string>): string {
    return Object.entries(headers).map(([k, v]) => `${k}: ${v}`).join("\n");
  }

  function parseParams(value: string): Record<string, string> {
    const params: Record<string, string> = {};
    for (const line of value.split("\n").filter((l) => l.trim())) {
      const idx = line.indexOf("=");
      if (idx > 0) {
        params[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
      }
    }
    return params;
  }

  function serializeParams(params: Record<string, string>): string {
    return Object.entries(params).map(([k, v]) => `${k}=${v}`).join("\n");
  }

  let headersText = $state(serializeHeaders(settings.customHeaders ?? {}));
  let paramsText = $state(serializeParams(settings.customParams ?? {}));

  $effect(() => {
    headersText = serializeHeaders(settings.customHeaders ?? {});
  });

  $effect(() => {
    paramsText = serializeParams(settings.customParams ?? {});
  });
</script>

<div class="mt-2 space-y-2 rounded border p-2 text-xs">
  <div class="space-y-1">
    <Label class="text-xs">Path Suffix</Label>
    <Input
      bind:value={settings.pathSuffix}
      placeholder="例如 /v1 或 /openai/deployments/xxx"
      class="h-7 text-xs"
      onchange={() => updateField("pathSuffix", settings.pathSuffix)}
    />
  </div>
  <div class="space-y-1">
    <Label class="text-xs">自定义 HTTP Headers（每行一个，格式：Key: Value）</Label>
    <textarea
      bind:value={headersText}
      class="w-full rounded-md border bg-background px-2 py-1 font-mono text-xs"
      rows={2}
      onchange={() => updateField("customHeaders", parseHeaders(headersText))}
    />
  </div>
  <div class="space-y-1">
    <Label class="text-xs">自定义 HTTP Params（每行一个，格式：key=value）</Label>
    <textarea
      bind:value={paramsText}
      class="w-full rounded-md border bg-background px-2 py-1 font-mono text-xs"
      rows={2}
      onchange={() => updateField("customParams", parseParams(paramsText))}
    />
  </div>
</div>

<style>
  @keyframes row-enter {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
