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
import { getApi } from "~/lib/api";
import { SKELETON_ROWS } from "~/lib/constants";
import { useAuthStore } from "~/stores/auth";

const api = getApi();
const authStore = useAuthStore();
const { isAdmin } = storeToRefs(authStore);

const models = ref<AdminModelResponse[]>([]);
const providers = ref<ProviderResponse[]>([]);
const loading = ref(true);
const error = ref("");
const expandedId = ref<number | null>(null);
const linksCache = ref<Map<number, ModelLinkView[]>>(new Map());
const linksLoading = ref<Set<number>>(new Set());
const linkTesting = ref<Set<number>>(new Set());
const linkTestResults = ref<
  Map<number, { success: boolean; latencyMs: number; error?: string; testedAt: number }>
>(new Map());

// Model form dialog
const showModelDialog = ref(false);
const editingModel = ref<AdminModelResponse | null>(null);
const form = ref({
  modelName: "",
  displayName: "",
  description: "",
  maxInput: 4096,
  maxOutput: 4096,
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

async function loadData() {
  loading.value = true;
  error.value = "";
  try {
    [models.value, providers.value] = await Promise.all([
      api.admin.listAdminModels(),
      api.admin.listProviders(),
    ]);
  } catch (e: any) {
    error.value = e.message;
  } finally {
    loading.value = false;
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
  if (!linksCache.value.has(modelId)) {
    const s = new Set(linksLoading.value);
    s.add(modelId);
    linksLoading.value = s;
    try {
      const links = await api.admin.listModelProviders(String(modelId));
      const m = new Map(linksCache.value);
      m.set(modelId, links);
      linksCache.value = m;
    } catch (e: any) {
      error.value = e.message;
    } finally {
      const s = new Set(linksLoading.value);
      s.delete(modelId);
      linksLoading.value = s;
    }
  }
}

function openCreateDialog() {
  editingModel.value = null;
  form.value = {
    modelName: "",
    displayName: "",
    description: "",
    maxInput: 4096,
    maxOutput: 4096,
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
    maxInput: m.maxInputTokens,
    maxOutput: m.maxOutputTokens,
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
    maxInputTokens: form.value.maxInput,
    maxOutputTokens: form.value.maxOutput,
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
      linksCache.value.delete(t.modelId);
    } else if (t.linkId) {
      await api.admin.deleteModelProvider(String(t.modelId), String(t.linkId));
      linksCache.value.delete(t.modelId);
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
  const s = new Set(linksLoading.value);
  s.add(modelId);
  linksLoading.value = s;
  try {
    const links = await api.admin.listModelProviders(String(modelId));
    const m = new Map(linksCache.value);
    m.set(modelId, links);
    linksCache.value = m;
    loadData();
  } catch (e: any) {
    error.value = e.message;
  } finally {
    const s2 = new Set(linksLoading.value);
    s2.delete(modelId);
    linksLoading.value = s2;
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
  return linkTestResults.value.get(linkId) ?? null;
}

function formatLinkTestResult(linkId: number): string {
  const result = getLinkTestResult(linkId);
  if (!result) return "";
  if (result.success) return `测试成功 · ${result.latencyMs}ms`;
  return `测试失败 · ${result.error ?? "未知错误"}`;
}

async function handleTestLink(modelId: number, link: ModelLinkView) {
  const loadingSet = new Set(linkTesting.value);
  loadingSet.add(link.id);
  linkTesting.value = loadingSet;

  try {
    const resp = await api.admin.testModelProviderReply(String(modelId), String(link.id), {
      prompt: null,
    });
    const m = new Map(linkTestResults.value);
    m.set(link.id, {
      success: resp.success,
      latencyMs: resp.latencyMs,
      error: resp.error ?? undefined,
      testedAt: Date.now(),
    });
    linkTestResults.value = m;
  } catch (e: any) {
    const m = new Map(linkTestResults.value);
    m.set(link.id, {
      success: false,
      latencyMs: 0,
      error: e.message || "请求失败",
      testedAt: Date.now(),
    });
    linkTestResults.value = m;
  } finally {
    const doneSet = new Set(linkTesting.value);
    doneSet.delete(link.id);
    linkTesting.value = doneSet;
  }
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col gap-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="font-mono text-xl font-bold text-foreground">模型管理</h2>
        <p class="mt-1 text-sm text-muted-foreground">大语言模型标称能力 + 提供者连接</p>
      </div>
      <Button
        class="cursor-pointer gap-2 bg-[#22C55E] font-medium text-black hover:bg-[#16A34A]"
        @click="openCreateDialog"
        ><Plus class="h-4 w-4" /> 添加模型</Button
      >
    </div>

    <Alert v-if="error" class="border-destructive/30 bg-destructive/10"
      ><AlertDescription class="text-sm text-destructive">{{ error }}</AlertDescription></Alert
    >

    <div v-if="loading" class="flex flex-col gap-3">
      <Skeleton v-for="i in SKELETON_ROWS.adminModels" :key="i" class="h-16 w-full rounded-lg" />
    </div>
    <div
      v-else-if="models.length === 0"
      class="flex flex-1 items-center justify-center text-muted-foreground"
    >
      <div class="flex flex-col items-center gap-2">
        <Cpu class="h-8 w-8 opacity-30" />
        <p class="text-sm">暂无模型，点击上方按钮添加</p>
      </div>
    </div>
    <div v-else class="flex flex-col gap-2 overflow-auto">
      <div v-for="m in models" :key="m.id" class="rounded-lg border border-border bg-card">
        <button
          class="flex w-full cursor-pointer items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/50"
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
              <span>↑{{ m.maxInputTokens.toLocaleString() }}</span>
              <span>↓{{ m.maxOutputTokens.toLocaleString() }}</span>
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
                  :class="
                    getLinkTestResult(link.id)?.success ? 'text-emerald-600' : 'text-destructive'
                  "
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
              <Label>最大输入 tokens</Label><Input v-model.number="form.maxInput" type="number" />
            </div>
            <div class="flex flex-col gap-2">
              <Label>最大输出 tokens</Label><Input v-model.number="form.maxOutput" type="number" />
            </div>
          </div>
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
          <Button
            class="cursor-pointer bg-[#22C55E] font-medium text-black hover:bg-[#16A34A]"
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
  </div>
</template>
<route lang="json">
{
  "meta": { "requiresAdmin": true }
}
</route>
