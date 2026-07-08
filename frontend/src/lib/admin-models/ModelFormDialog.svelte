<script lang="ts">
  // 模型创建/编辑对话框 — 从 AdminModelsPage 抽出。
  // 见 PLAN.md §10 Phase B B.4。
  //
  // 父组件用法：
  //   <ModelFormDialog
  //     bind:open={showModelDialog}
  //     editingModel={editingModel}  // null = 新建
  //     onSaved={loadModels}
  //     onError={(e) => error = e}
  //   />

  import { getApi } from "$lib/api";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
  } from "$lib/components/ui/dialog/index.js";
  import type { AdminModelResponse } from "$bindings/AdminModelResponse";
  import type { ModelInput } from "$bindings/ModelInput";

  const api = getApi();

  let {
    open = $bindable(false),
    editingModel = null,
    onSaved,
    onError,
  }: {
    open: boolean;
    editingModel: AdminModelResponse | null;
    onSaved: () => void;
    onError: (e: string) => void;
  } = $props();

  // ── 表单状态 ──
  let formModelName = $state("");
  let formDisplayName = $state("");
  let formDescription = $state("");
  let formMaxInput = $state(4096);
  let formMaxOutput = $state(4096);
  let formToolCalling = $state(false);
  let formVision = $state(false);
  let formThinking = $state(false);
  let formAdaptive = $state(false);
  let formStatus = $state("stable");
  let formSaving = $state(false);

  // 当 open 或 editingModel 变化时，回填表单
  $effect(() => {
    if (!open) return;
    if (editingModel) {
      formModelName = editingModel.modelName;
      formDisplayName = editingModel.displayName;
      formDescription = editingModel.description ?? "";
      formMaxInput = editingModel.maxInputTokens;
      formMaxOutput = editingModel.maxOutputTokens;
      formToolCalling = editingModel.toolCalling;
      formVision = editingModel.vision;
      formThinking = editingModel.thinking;
      formAdaptive = editingModel.adaptiveThinking;
      formStatus = editingModel.status ?? "stable";
    } else {
      formModelName = "";
      formDisplayName = "";
      formDescription = "";
      formMaxInput = 4096;
      formMaxOutput = 4096;
      formToolCalling = false;
      formVision = false;
      formThinking = false;
      formAdaptive = false;
      formStatus = "stable";
    }
  });

  async function saveModel() {
    if (!formModelName.trim()) {
      onError("模型唯一标识 (model_name) 必填");
      return;
    }
    formSaving = true;
    const input: ModelInput = {
      modelName: formModelName.trim(),
      displayName: formDisplayName.trim() || formModelName.trim(),
      description: formDescription.trim() || null,
      maxInputTokens: formMaxInput,
      maxOutputTokens: formMaxOutput,
      toolCalling: formToolCalling,
      vision: formVision,
      thinking: formThinking,
      adaptiveThinking: formAdaptive,
      status: formStatus.trim() || null,
    };
    try {
      if (editingModel) {
        await api.admin.updateAdminModel(String(editingModel.id), input);
      } else {
        await api.admin.createAdminModel(input);
      }
      open = false;
      onSaved();
    } catch (e: any) {
      onError(e.message);
    } finally {
      formSaving = false;
    }
  }
</script>

<Dialog open={open} onOpenChange={(v) => (open = v)}>
  <DialogContent class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle class="font-mono">{editingModel ? "编辑模型" : "添加模型"}</DialogTitle>
    </DialogHeader>
    <div class="flex flex-col gap-3 max-h-[80vh] overflow-y-auto pr-1">
      <div class="flex flex-col gap-2">
        <Label for="fm-mn">模型唯一标识 (model_name)</Label>
        <Input id="fm-mn" placeholder="openai/gpt-4o" bind:value={formModelName} class="font-mono" />
        <p class="text-xs text-muted-foreground">前缀通常为品牌，如 openai/anthropic/...</p>
      </div>
      <div class="flex flex-col gap-2">
        <Label for="fm-dn">显示名</Label>
        <Input id="fm-dn" placeholder="GPT-4o" bind:value={formDisplayName} />
      </div>
      <div class="flex flex-col gap-2">
        <Label for="fm-desc">描述</Label>
        <textarea
          id="fm-desc"
          rows="2"
          bind:value={formDescription}
          class="rounded-md border border-input bg-background px-2 py-1.5 text-sm resize-y"
          placeholder="(可选) 模型简述"
        ></textarea>
      </div>
      <div class="grid grid-cols-2 gap-2">
        <div class="flex flex-col gap-2">
          <Label for="fm-mi">最大输入 tokens</Label>
          <Input id="fm-mi" type="number" bind:value={formMaxInput} />
        </div>
        <div class="flex flex-col gap-2">
          <Label for="fm-mo">最大输出 tokens</Label>
          <Input id="fm-mo" type="number" bind:value={formMaxOutput} />
        </div>
      </div>
      <div class="flex flex-col gap-2">
        <Label for="fm-status">状态</Label>
        <Input id="fm-status" placeholder="stable / beta / deprecated" bind:value={formStatus} />
      </div>
      <div class="flex flex-wrap gap-x-4 gap-y-2 text-sm">
        <label class="flex items-center gap-2 cursor-pointer">
          <Checkbox checked={formToolCalling} onCheckedChange={(v) => (formToolCalling = v)} />
          工具调用
        </label>
        <label class="flex items-center gap-2 cursor-pointer">
          <Checkbox checked={formVision} onCheckedChange={(v) => (formVision = v)} />
          视觉
        </label>
        <label class="flex items-center gap-2 cursor-pointer">
          <Checkbox checked={formThinking} onCheckedChange={(v) => (formThinking = v)} />
          思考
        </label>
        <label class="flex items-center gap-2 cursor-pointer">
          <Checkbox checked={formAdaptive} onCheckedChange={(v) => (formAdaptive = v)} />
          自适应思考
        </label>
      </div>
      <div class="flex gap-2 pt-2">
        <Button
          variant="outline"
          class="flex-1 cursor-pointer"
          onclick={() => (open = false)}
        >
          取消
        </Button>
        <Button
          class="flex-1 bg-[#22C55E] hover:bg-[#16A34A] text-black cursor-pointer"
          onclick={saveModel}
          disabled={formSaving || !formModelName.trim()}
        >
          {formSaving ? "保存中..." : "保存"}
        </Button>
      </div>
    </div>
  </DialogContent>
</Dialog>
