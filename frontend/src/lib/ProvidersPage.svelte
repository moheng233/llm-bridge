<script lang="ts">
  import { createApiClient } from "$bindings/client";
  import type {
    ProviderResponse,
    UpdateProviderRequest,
    ApiKeyEntry,
    ProviderCompatibility,
    ProviderCompatConfig,
    CompatibilitySettings,
  } from "$bindings";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";

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

  let formEnabled = $state(true);
  let formPriority = $state(0);
  let formCompatibilities = $state<Record<ProviderCompatibility, ProviderCompatConfig>>(defaultCompatibilities());
  let formBaseUrlOverride = $state("");
  let formApiKeys = $state<ApiKeyEntry[]>([]);

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
    formEnabled = true;
    formPriority = 0;
    formCompatibilities = defaultCompatibilities();
    formBaseUrlOverride = "";
    formApiKeys = [];
    dialogOpen = false;
    editing = null;
    formError = "";
  }

  function startEdit(p: ProviderResponse) {
    editing = p.provider_name;
    formEnabled = p.enabled;
    formPriority = p.priority;
    formCompatibilities = { ...defaultCompatibilities(), ...p.compatibilities };
    formBaseUrlOverride = p.base_url_override ?? "";
    // Convert ApiKeyDisplay to ApiKeyEntry (key is empty since we only have masked_key)
    formApiKeys = p.api_keys.map((ak) => ({ label: ak.label, key: "", weight: ak.weight }));
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

  function addApiKey() {
    formApiKeys = [...formApiKeys, { label: "", key: "", weight: 1 }];
  }

  function removeApiKey(index: number) {
    formApiKeys = formApiKeys.filter((_, i) => i !== index);
  }

  function updateApiKeyField(index: number, field: keyof ApiKeyEntry, value: string | number) {
    formApiKeys = formApiKeys.map((ak, i) =>
      i === index ? { ...ak, [field]: value } : ak,
    );
  }

  async function handleSubmit() {
    if (!editing) return;
    formError = "";
    const enabledCompatibilities = Object.keys(formCompatibilities).filter(
      (k) => formCompatibilities[k as ProviderCompatibility].enabled,
    );
    if (enabledCompatibilities.length === 0) {
      formError = "至少需要启用一种兼容性协议";
      return;
    }
    try {
      const req: UpdateProviderRequest = {
        enabled: formEnabled,
        priority: formPriority,
        base_url_override: formBaseUrlOverride || null,
        api_keys: formApiKeys.filter((ak) => ak.key.length > 0),
        compatibilities: formCompatibilities,
      };
      await api.providers.updateProvider(editing, req);
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

  // Compatibility settings helpers for inline editing
  function updateCompatSettingField(
    compat: ProviderCompatibility,
    field: keyof CompatibilitySettings,
    value: any,
  ) {
    const s = formCompatibilities[compat].settings ?? { pathSuffix: null, customHeaders: {}, customParams: {} };
    updateCompatSettings(compat, { ...s, [field]: value });
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
        <span class="text-sm text-muted-foreground">提供者从 models.dev 自动发现，此处配置密钥和优先级</span>
      </div>
    </div>

    <Dialog.Root bind:open={dialogOpen}>
      <Dialog.Content class="sm:max-w-xl">
        <Dialog.Header>
          <Dialog.Title>编辑提供者</Dialog.Title>
          <Dialog.Description>
            {editing ? `配置 "${editing}" 的密钥、优先级和兼容性` : ""}
          </Dialog.Description>
        </Dialog.Header>
        {#if formError}
          <Alert variant="destructive">
            <AlertDescription>{formError}</AlertDescription>
          </Alert>
        {/if}
        <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }} class="space-y-4">
          <div class="flex items-center gap-4">
            <div class="flex items-center gap-2">
              <Checkbox checked={formEnabled} onCheckedChange={() => (formEnabled = !formEnabled)} />
              <Label>启用</Label>
            </div>
            <div class="flex items-center gap-2">
              <Label for="priority">优先级</Label>
              <Input id="priority" type="number" bind:value={formPriority} class="w-20" />
            </div>
          </div>

          <div class="space-y-2">
            <Label for="base-url-override">自定义 Base URL（可选）</Label>
            <Input id="base-url-override" bind:value={formBaseUrlOverride} placeholder="留空使用默认地址" />
          </div>

          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <Label>API Keys</Label>
              <Button type="button" variant="outline" size="sm" onclick={addApiKey}>+ 添加</Button>
            </div>
            {#if formApiKeys.length === 0}
              <p class="text-xs text-muted-foreground">尚未配置任何 API Key</p>
            {:else}
              <div class="space-y-2">
                {#each formApiKeys as ak, i}
                  <div class="flex items-end gap-2 rounded-md border p-2">
                    <div class="flex-1 space-y-1">
                      <Input
                        placeholder="标签"
                        value={ak.label}
                        oninput={(e) => updateApiKeyField(i, "label", (e.target as HTMLInputElement).value)}
                        class="h-8 text-xs"
                      />
                      <Input
                        type="password"
                        placeholder="密钥"
                        value={ak.key}
                        oninput={(e) => updateApiKeyField(i, "key", (e.target as HTMLInputElement).value)}
                        class="h-8 text-xs"
                      />
                      <div class="flex items-center gap-2">
                        <Label class="text-xs">权重</Label>
                        <Input
                          type="number"
                          value={ak.weight}
                          oninput={(e) => updateApiKeyField(i, "weight", Number((e.target as HTMLInputElement).value))}
                          class="h-7 w-16 text-xs"
                          min="1"
                        />
                      </div>
                    </div>
                    <Button type="button" variant="ghost" size="sm" onclick={() => removeApiKey(i)}>✕</Button>
                  </div>
                {/each}
              </div>
            {/if}
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
                    <Badge class={opt.badgeClass}>已启用</Badge>
                  {/if}
                </div>
                {#if formCompatibilities[opt.value]?.enabled}
                  {@const settings = formCompatibilities[opt.value]?.settings ?? { pathSuffix: null, customHeaders: {}, customParams: {} }}
                  <div class="mt-2 space-y-2 rounded border p-2 text-xs">
                    <div class="space-y-1">
                      <Label class="text-xs">Path Suffix</Label>
                      <Input
                        value={settings.pathSuffix ?? ""}
                        placeholder="例如 /v1"
                        class="h-7 text-xs"
                        oninput={(e) => updateCompatSettingField(opt.value, "pathSuffix", (e.target as HTMLInputElement).value || null)}
                      />
                    </div>
                    <div class="space-y-1">
                      <Label class="text-xs">自定义 HTTP Headers（每行一个，格式：Key: Value）</Label>
                      <textarea
                        value={Object.entries(settings.customHeaders ?? {}).map(([k, v]) => `${k}: ${v}`).join("\n")}
                        class="w-full rounded-md border bg-background px-2 py-1 font-mono text-xs"
                        rows={2}
                        oninput={(e) => {
                          const headers: Record<string, string> = {};
                          for (const line of (e.target as HTMLTextAreaElement).value.split("\n").filter((l) => l.trim())) {
                            const idx = line.indexOf(":");
                            if (idx > 0) headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
                          }
                          updateCompatSettingField(opt.value, "customHeaders", headers);
                        }}
                      ></textarea>
                    </div>
                    <div class="space-y-1">
                      <Label class="text-xs">自定义 HTTP Params（每行一个，格式：key=value）</Label>
                      <textarea
                        value={Object.entries(settings.customParams ?? {}).map(([k, v]) => `${k}=${v}`).join("\n")}
                        class="w-full rounded-md border bg-background px-2 py-1 font-mono text-xs"
                        rows={2}
                        oninput={(e) => {
                          const params: Record<string, string> = {};
                          for (const line of (e.target as HTMLTextAreaElement).value.split("\n").filter((l) => l.trim())) {
                            const idx = line.indexOf("=");
                            if (idx > 0) params[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
                          }
                          updateCompatSettingField(opt.value, "customParams", params);
                        }}
                      ></textarea>
                    </div>
                  </div>
                {/if}
              {/each}
            </div>
          </div>

          <Dialog.Footer>
            <Button type="button" variant="outline" onclick={resetForm}>取消</Button>
            <Button type="submit">保存修改</Button>
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
          暂无提供者，请等待 models.dev 数据同步后自动出现
        </div>
      {:else}
        <div class="grid grid-cols-1 gap-4 p-4 md:grid-cols-2">
          {#each providers as p, rowIndex}
            <Card.Root style="animation: row-enter 220ms ease both; animation-delay: {Math.min(rowIndex * 20, 480)}ms;">
              <Card.Header>
                <div class="flex items-start justify-between">
                  <div>
                    <div class="flex items-center gap-2">
                      <Card.Title class="text-base">{p.provider_name}</Card.Title>
                      {#if p.enabled}
                        <Badge class="bg-green-100 text-green-700 hover:bg-green-100">启用</Badge>
                      {:else}
                        <Badge variant="secondary">禁用</Badge>
                      {/if}
                    </div>
                    <div class="mt-1 flex flex-wrap gap-1">
                      {#each COMPAT_OPTIONS as opt}
                        {#if p.compatibilities[opt.value]?.enabled}
                          <Badge class={opt.badgeClass}>{opt.label}</Badge>
                        {/if}
                      {/each}
                    </div>
                  </div>
                  <div class="flex gap-1">
                    <Button variant="ghost" size="sm" onclick={() => startEdit(p)}>✏️</Button>
                  </div>
                </div>
              </Card.Header>
              <Card.Content>
                <dl class="space-y-1.5 text-xs text-muted-foreground">
                  {#if p.base_url_override}
                    <div class="flex gap-2">
                      <dt class="shrink-0 font-medium">URL</dt>
                      <dd class="truncate font-mono">{p.base_url_override}</dd>
                    </div>
                  {/if}
                  <div class="flex gap-2">
                    <dt class="shrink-0 font-medium">优先级</dt>
                    <dd class="font-mono">{p.priority}</dd>
                  </div>
                  <div class="flex gap-2">
                    <dt class="shrink-0 font-medium">模型数</dt>
                    <dd class="font-mono">{p.model_count}</dd>
                  </div>
                  <div class="flex gap-2">
                    <dt class="shrink-0 font-medium">密钥</dt>
                    <dd class="font-mono">
                      {#if p.api_keys.length > 0}
                        {p.api_keys.length} 个已配置
                      {:else}
                        <span class="text-orange-500">未配置</span>
                      {/if}
                    </dd>
                  </div>
                  {#if p.api_keys.length > 0}
                    <div class="flex flex-col gap-1 pt-1">
                      {#each p.api_keys as ak}
                        <span class="font-mono text-xs">
                          {ak.label || "(无标签)"} · {ak.masked_key} · 权重 {ak.weight}
                        </span>
                      {/each}
                    </div>
                  {/if}
                </dl>
              </Card.Content>
            </Card.Root>
          {/each}
        </div>
      {/if}
    </div>
  </section>
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
