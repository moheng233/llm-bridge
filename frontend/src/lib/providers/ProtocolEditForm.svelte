<script lang="ts">
  // 协议编辑表单 — 参数化复用于「创建对话框内新增」与「列表展开内增改」。
  // 见 PLAN.md §10 Phase B B.3。
  //
  // 用法：
  //   <ProtocolEditForm
  //     bind:draft={protocolDraft}
  //     title="新建协议"
  //     confirmText="加入列表"
  //     onConfirm={() => ...}
  //     onCancel={() => ...}
  //   />

  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from "$lib/components/ui/select/index.js";
  import { X } from "@lucide/svelte";
  import { PROTOCOL_OPTIONS } from "$lib/constants";
  import type { ProtocolInput } from "$bindings/ProtocolInput";
  import type { ProviderCompatibility } from "$bindings/ProviderCompatibility";

  let {
    draft = $bindable(),
    title = "新建协议",
    confirmText = "保存",
    onConfirm,
    onCancel,
  }: {
    draft: ProtocolInput;
    title?: string;
    confirmText?: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
</script>

<div class="rounded-md border border-border bg-card p-3 flex flex-col gap-2 mt-1">
  <div class="flex items-center justify-between">
    <span class="text-xs font-medium">{title}</span>
    <Button
      size="icon"
      variant="ghost"
      class="h-6 w-6 cursor-pointer"
      onclick={onCancel}
      aria-label="取消"
    >
      <X class="h-3 w-3" />
    </Button>
  </div>
  <div class="flex flex-col gap-1">
    <Label class="text-xs">协议类型</Label>
    <Select type="single" value={draft.protocol} onValueChange={(v) => v && (draft.protocol = v as ProviderCompatibility)}>
      <SelectTrigger class="cursor-pointer">
        <span class="text-sm">{PROTOCOL_OPTIONS.find((o) => o.value === draft.protocol)?.label ?? draft.protocol}</span>
      </SelectTrigger>
      <SelectContent>
        {#each PROTOCOL_OPTIONS as opt}
          <SelectItem value={opt.value}>{opt.label}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
  </div>
  <div class="flex flex-col gap-1">
    <Label class="text-xs">端点 URL</Label>
    <Input
      placeholder="https://api.openai.com/v1"
      bind:value={draft.baseUrl}
      class="h-9 text-sm"
    />
  </div>
  <div class="grid grid-cols-2 gap-2">
    <div class="flex flex-col gap-1">
      <Label class="text-xs">优先级</Label>
      <Input
        type="number"
        bind:value={draft.priority}
        class="h-9 text-sm"
      />
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">compat settings</Label>
      <Input
        placeholder='compat JSON, 可选'
        value={draft.compatSettings ?? ""}
        oninput={(e) => (draft.compatSettings = (e.target as HTMLInputElement).value || null)}
        class="h-9 text-sm font-mono"
      />
    </div>
  </div>
  <label class="flex items-center gap-2 text-xs cursor-pointer">
    <Checkbox checked={draft.enabled} onCheckedChange={(v) => (draft.enabled = v)} />
    启用
  </label>
  <div class="flex gap-2 pt-1">
    <Button
      variant="outline"
      class="flex-1 cursor-pointer"
      onclick={onCancel}
    >
      取消
    </Button>
    <Button
      class="flex-1 bg-[#22C55E] hover:bg-[#16A34A] text-black cursor-pointer"
      onclick={onConfirm}
      disabled={!draft.baseUrl.trim()}
    >
      {confirmText}
    </Button>
  </div>
</div>
