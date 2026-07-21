<script setup lang="ts">
import { type AdminModelResponse } from "@bindings/AdminModelResponse";
import { type ModelInput } from "@bindings/ModelInput";
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";
import {
  Plus,
  Cpu,
  ChevronDown,
  ChevronRight,
  Pencil,
  Trash2,
  Link2,
  FlaskConical,
} from "@lucide/vue";

import ModelLinkEditForm from "~/components/providers/ModelLinkEditForm.vue";
import { useApiCall } from "~/composables/useApiCall";
import { useReactiveMap, useReactiveSet } from "~/composables/useReactiveCollections";
import { getApi, formatTokens, parseTokens } from "~/lib/api";
import { SKELETON_ROWS } from "~/lib/constants";
import { useAuthStore } from "~/stores/auth";

const api = getApi();
const authStore = useAuthStore();
const { isAdmin } = storeToRefs(authStore);

const models = ref<AdminModelResponse[]>([]);
const providers = ref<ProviderResponse[]>([]);
const expandedId = ref<number | null>(null);
const linksCache = useReactiveMap<number, ModelLinkView[]>();
const linksLoading = useReactiveSet<number>();
const linkTesting = useReactiveSet<number>();
const linkTestResults = useReactiveMap<
  number,
  { success: boolean; latencyMs: number; error?: string; testedAt: number }
>();

// Model form dialog
const showModelDialog = ref(false);
const editingModel = ref<AdminModelResponse | null>(null);
const form = ref({
  modelName: "",
  displayName: "",
  description: "",
  maxInput: "4K",
  maxOutput: "4K",
  toolCalling: false,
  vision: false,
  thinking: false,
  adaptive: false,
  status: "stable",
});
const formSaving = ref(false);

// Delete dialog
const deleteDialogOpen = ref(false);
const deleteTarget = ref<{ type: string; modelId: number; name: string; linkId?: number } | null>(
  null,
);

const {
  loading,
  error,
  execute: fetchData,
} = useApiCall(() => Promise.all([api.admin.listAdminModels(), api.admin.listProviders()]));

async function loadData() {
  const result = await fetchData();
  if (result) {
    models.value = result[0];
    providers.value = result[1];
  }
}

watchEffect(() => {
  if (isAdmin.value) loadData();
});

async function toggleLinks(modelId: number) {
  if (expandedId.value === modelId) {
    expandedId.value = null;
    return;
  }
  expandedId.value = modelId;
  if (!linksCache.has(modelId)) {
    linksLoading.add(modelId);
    try {
      const links = await api.admin.listModelProviders(String(modelId));
      linksCache.set(modelId, links);
    } catch (e: any) {
      error.value = e.message;
    } finally {
      linksLoading.delete(modelId);
    }
  }
}

function openCreateDialog() {
  editingModel.value = null;
  form.value = {
    modelName: "",
    displayName: "",
    description: "",
    maxInput: "4K",
    maxOutput: "4K",
    toolCalling: false,
    vision: false,
    thinking: false,
    adaptive: false,
    status: "stable",
  };
  showModelDialog.value = true;
}
function openEditDialog(m: AdminModelResponse) {
  editingModel.value = m;
  form.value = {
    modelName: m.modelName,
    displayName: m.displayName,
    description: m.description ?? "",
    maxInput: formatTokens(m.maxInputTokens),
    maxOutput: formatTokens(m.maxOutputTokens),
    toolCalling: m.toolCalling,
    vision: m.vision,
    thinking: m.thinking,
    adaptive: m.adaptiveThinking,
    status: m.status ?? "stable",
  };
  showModelDialog.value = true;
}

async function saveModel() {
  if (!form.value.modelName.trim()) {
    error.value = "模型唯一标识必填";
    return;
  }
  formSaving.value = true;
  const input: ModelInput = {
    modelName: form.value.modelName.trim(),
    displayName: form.value.displayName.trim() || form.value.modelName.trim(),
    description: form.value.description.trim() || null,
    maxInputTokens: parseTokens(form.value.maxInput) ?? 0,
    maxOutputTokens: parseTokens(form.value.maxOutput) ?? 0,
    toolCalling: form.value.toolCalling,
    vision: form.value.vision,
    thinking: form.value.thinking,
    adaptiveThinking: form.value.adaptive,
    status: form.value.status || null,
  };
  try {
    if (editingModel.value) await api.admin.updateAdminModel(String(editingModel.value.id), input);
    else await api.admin.createAdminModel(input);
    showModelDialog.value = false;
    loadData();
  } catch (e: any) {
    error.value = e.message;
  } finally {
    formSaving.value = false;
  }
}

function confirmDelete(t: { type: string; modelId: number; name: string; linkId?: number }) {
  deleteTarget.value = t;
  deleteDialogOpen.value = true;
}

async function doDelete() {
  const t = deleteTarget.value;
  if (!t) return;
  deleteDialogOpen.value = false;
  deleteTarget.value = null;
  try {
    if (t.type === "model") {
      await api.admin.deleteAdminModel(String(t.modelId));
      linksCache.delete(t.modelId);
    } else if (t.linkId) {
      await api.admin.deleteModelProvider(String(t.modelId), String(t.linkId));
      linksCache.delete(t.modelId);
    }
    loadData();
  } catch (e: any) {
    error.value = e.message;
  }
}

// ── Link editing ──
const editingLinkModelId = ref<number | null>(null);
const editingLink = ref<ModelLinkView | null>(null); // null = 新建
const linkDialogOpen = ref(false);

async function refreshLinks(modelId: number) {
  linksLoading.add(modelId);
  try {
    const links = await api.admin.listModelProviders(String(modelId));
    linksCache.set(modelId, links);
    loadData();
  } catch (e: any) {
    error.value = e.message;
  } finally {
    linksLoading.delete(modelId);
  }
}

function openCreateLink(modelId: number) {
  editingLinkModelId.value = modelId;
  editingLink.value = null;
  linkDialogOpen.value = true;
}

function openEditLink(modelId: number, link: ModelLinkView) {
  editingLinkModelId.value = modelId;
  editingLink.value = link;
  linkDialogOpen.value = true;
}

function handleLinkSaved(modelId: number) {
  // 不在此处清空 editingLinkModelId——保持组件挂载，让弹窗通过 v-model:open 自行关闭。
  // 仅刷新数据；清空状态在弹窗关闭后处理。
  refreshLinks(modelId);
}

// 弹窗关闭后清空编辑状态（延迟一帧，避免与 saved → open=false 流程竞争）。
watch(linkDialogOpen, (v) => {
  if (!v) {
    const id = editingLinkModelId.value;
    if (id !== null) {
      nextTick(() => {
        if (!linkDialogOpen.value) {
          editingLinkModelId.value = null;
          editingLink.value = null;
        }
      });
    }
  }
});

function getLinkTestResult(linkId: number) {
  return linkTestResults.get(linkId) ?? null;
}

function formatLinkTestResult(linkId: number): string {
  const result = getLinkTestResult(linkId);
  if (!result) return "";
  if (result.success) return `测试成功 · ${result.latencyMs}ms`;
  return `测试失败 · ${result.error ?? "未知错误"}`;
}

async function handleTestLink(modelId: number, link: ModelLinkView) {
  linkTesting.add(link.id);

  try {
    const resp = await api.admin.testModelProviderReply(String(modelId), String(link.id), {
      prompt: null,
    });
    linkTestResults.set(link.id, {
      success: resp.success,
      latencyMs: resp.latencyMs,
      error: resp.error ?? undefined,
      testedAt: Date.now(),
    });
  } catch (e: any) {
    linkTestResults.set(link.id, {
      success: false,
      latencyMs: 0,
      error: e.message || "请求失败",
      testedAt: Date.now(),
    });
  } finally {
    linkTesting.delete(link.id);
  }
}
</script>

<template>
  <PageShell>
    <SectionHeader title="模型管理" description="大语言模型标称能力 + 提供者连接" :icon="Cpu">
      <template #actions>
        <Button
          class="cursor-pointer gap-2 bg-cta font-medium text-black hover:bg-cta-hover"
          @click="openCreateDialog"
          ><Plus class="h-4 w-4" /> 添加模型</Button
        >
      </template>
    </SectionHeader>

    <ErrorState v-if="error" :error="error" inline @retry="loadData" />

    <div v-if="loading" class="flex flex-col gap-3">
      <Skeleton v-for="i in SKELETON_ROWS.adminModels" :key="i" class="h-16 w-full rounded-lg" />
    </div>
    <EmptyState v-else-if="models.length === 0" :icon="Cpu" title="暂无模型，点击上方按钮添加" />
    <div v-else class="flex flex-col gap-2 overflow-auto">
      <div v-for="m in models" :key="m.id" class="rounded-lg border border-border bg-card">
        <button
          class="flex w-full cursor-pointer items-center gap-3 px-4 py-3 text-left transition-all duration-150 hover:bg-accent/60 hover:shadow-sm"
          @click="toggleLinks(m.id)"
        >
          <ChevronDown v-if="expandedId === m.id" class="h-4 w-4 shrink-0 text-muted-foreground" />
          <ChevronRight v-else class="h-4 w-4 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <span class="font-mono font-medium text-foreground">{{ m.modelName }}</span>
              <Badge v-if="m.status" variant="secondary" class="text-xs">{{ m.status }}</Badge>
              <Badge variant="outline" class="text-xs">{{ m.providerCount }} 个连接</Badge>
            </div>
            <div class="mt-0.5 flex flex-wrap gap-3 text-xs text-muted-foreground">
              <span>{{ m.displayName }}</span>
              <span>↑{{ formatTokens(m.maxInputTokens) }}</span>
              <span>↓{{ formatTokens(m.maxOutputTokens) }}</span>
              <Badge v-if="m.toolCalling" variant="outline" class="py-0 text-[10px]">tools</Badge>
              <Badge v-if="m.vision" variant="outline" class="py-0 text-[10px]">vision</Badge>
              <Badge v-if="m.thinking" variant="outline" class="py-0 text-[10px]">thinking</Badge>
            </div>
          </div>
          <div class="flex items-center gap-1">
            <Button
              size="icon"
              variant="ghost"
              class="h-7 w-7 cursor-pointer text-muted-foreground hover:text-foreground"
              @click.stop="openEditDialog(m)"
              ><Pencil class="h-3.5 w-3.5"
            /></Button>
            <Button
              size="icon"
              variant="ghost"
              class="h-7 w-7 cursor-pointer text-muted-foreground hover:text-destructive"
              @click.stop="confirmDelete({ type: 'model', modelId: m.id, name: m.modelName })"
              ><Trash2 class="h-3.5 w-3.5"
            /></Button>
          </div>
        </button>
        <div
          v-if="expandedId === m.id"
          class="flex flex-col gap-3 border-t border-border px-4 py-3"
        >
          <!-- Header: title + add link button -->
          <div class="flex items-center justify-between">
            <span class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
              提供者连接
            </span>
            <Button
              size="sm"
              variant="outline"
              class="h-7 cursor-pointer gap-1"
              @click="openCreateLink(m.id)"
            >
              <Link2 class="h-3 w-3" />
              添加连接
            </Button>
          </div>

          <div
            v-if="linksLoading.has(m.id)"
            class="flex items-center gap-2 py-2 text-sm text-muted-foreground"
          >
            <Spinner class="h-4 w-4" /> 加载连接...
          </div>
          <div
            v-else-if="!linksCache.get(m.id)?.length"
            class="flex items-center gap-2 py-2 text-sm text-muted-foreground italic"
          >
            <Link2 class="h-4 w-4" /> 暂无连接 — 该模型尚未关联任何提供者
          </div>
          <div v-else class="flex flex-col gap-1.5">
            <div
              v-for="link in linksCache.get(m.id)"
              :key="link.id"
              class="flex items-center justify-between rounded-md border border-border bg-muted/30 px-3 py-2 text-sm"
            >
              <div class="flex min-w-0 flex-wrap items-center gap-2">
                <Badge variant="outline" class="shrink-0 font-mono text-xs">{{
                  link.protocol
                }}</Badge>
                <span class="font-mono text-xs text-foreground">{{
                  link.providerDisplayName
                }}</span>
                <span class="text-xs text-muted-foreground"
                  >→ {{ link.providerModelId }} P{{ link.priority }}</span
                >
                <Badge v-if="!link.enabled" variant="secondary" class="text-xs">禁用</Badge>
                <span v-if="link.inputPricePer1m !== null" class="text-xs text-muted-foreground"
                  >${{ link.inputPricePer1m }}/M</span
                >
                <span
                  v-if="getLinkTestResult(link.id)"
                  class="text-xs"
                  :class="getLinkTestResult(link.id)?.success ? 'text-cta' : 'text-destructive'"
                  >{{ formatLinkTestResult(link.id) }}</span
                >
              </div>
              <div class="flex shrink-0 items-center gap-1">
                <Button
                  size="icon"
                  variant="ghost"
                  class="h-6 w-6 cursor-pointer text-muted-foreground hover:text-foreground"
                  :disabled="linkTesting.has(link.id)"
                  @click="handleTestLink(m.id, link)"
                  aria-label="测试连接"
                >
                  <Spinner v-if="linkTesting.has(link.id)" class="h-3 w-3" />
                  <FlaskConical v-else class="h-3 w-3" />
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  class="h-6 w-6 cursor-pointer text-muted-foreground hover:text-foreground"
                  @click="openEditLink(m.id, link)"
                  aria-label="编辑连接"
                  ><Pencil class="h-3 w-3"
                /></Button>
                <Button
                  size="icon"
                  variant="ghost"
                  class="h-6 w-6 cursor-pointer text-muted-foreground hover:text-destructive"
                  @click="
                    confirmDelete({
                      type: 'link',
                      modelId: m.id,
                      linkId: link.id,
                      name: link.providerDisplayName,
                    })
                  "
                  ><Trash2 class="h-3 w-3"
                /></Button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Model Form Dialog -->
    <Dialog :open="showModelDialog" @update:open="(v: boolean) => (showModelDialog = v)">
      <DialogContent class="sm:max-w-md">
        <DialogHeader
          ><DialogTitle class="font-mono">{{
            editingModel ? "编辑模型" : "添加模型"
          }}</DialogTitle></DialogHeader
        >
        <div class="flex flex-col gap-4">
          <div class="flex flex-col gap-2">
            <Label>唯一标识 (model_name)</Label
            ><Input v-model="form.modelName" placeholder="gpt-4o" />
          </div>
          <div class="flex flex-col gap-2">
            <Label>显示名称</Label><Input v-model="form.displayName" placeholder="GPT-4o" />
          </div>
          <div class="flex flex-col gap-2">
            <Label>描述</Label><Input v-model="form.description" placeholder="可选" />
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div class="flex flex-col gap-2">
              <Label>最大输入</Label>
              <Input v-model="form.maxInput" placeholder="如 1M / 256K" />
            </div>
            <div class="flex flex-col gap-2">
              <Label>最大输出</Label>
              <Input v-model="form.maxOutput" placeholder="如 1M / 256K" />
            </div>
          </div>
          <div class="flex flex-col gap-2">
            <Label>模型能力</Label>
            <div class="flex flex-wrap gap-4">
              <Label class="flex cursor-pointer items-center gap-2 text-sm"
                ><Checkbox v-model="form.toolCalling" /> 工具调用</Label
              >
              <Label class="flex cursor-pointer items-center gap-2 text-sm"
                ><Checkbox v-model="form.vision" /> 视觉</Label
              >
              <Label class="flex cursor-pointer items-center gap-2 text-sm"
                ><Checkbox v-model="form.thinking" /> 推理</Label
              >
            </div>
          </div>
          <Button
            class="cursor-pointer bg-cta font-medium text-black hover:bg-cta-hover"
            @click="saveModel"
            :disabled="formSaving || !form.modelName.trim()"
            >{{ formSaving ? "保存中..." : editingModel ? "保存" : "创建" }}</Button
          >
        </div>
      </DialogContent>
    </Dialog>

    <!-- Delete Confirm Dialog -->
    <Dialog :open="deleteDialogOpen" @update:open="(v: boolean) => (deleteDialogOpen = v)">
      <DialogContent class="sm:max-w-sm">
        <DialogHeader><DialogTitle class="font-mono text-sm">确认删除</DialogTitle></DialogHeader>
        <p class="text-sm text-muted-foreground">
          确定要删除
          <span class="font-mono font-medium text-foreground">{{ deleteTarget?.name }}</span>
          吗？该操作不可撤销。
        </p>
        <div class="flex gap-2">
          <Button variant="outline" class="flex-1 cursor-pointer" @click="deleteDialogOpen = false"
            >取消</Button
          >
          <Button variant="destructive" class="flex-1 cursor-pointer" @click="doDelete"
            >确认删除</Button
          >
        </div>
      </DialogContent>
    </Dialog>

    <!-- Model Link (Add / Edit) Dialog -->
    <ModelLinkEditForm
      v-if="editingLinkModelId !== null"
      :model-id="editingLinkModelId"
      :model-name="models.find((m) => m.id === editingLinkModelId)?.modelName ?? ''"
      :providers="providers"
      :current-model="models.find((m) => m.id === editingLinkModelId) ?? null"
      :editing-link="editingLink"
      v-model:open="linkDialogOpen"
      @saved="handleLinkSaved(editingLinkModelId!)"
      @error="(msg: string) => (error = msg)"
    />
  </PageShell>
</template>
<route lang="json">
{
  "meta": { "requiresAdmin": true }
}
</route>
