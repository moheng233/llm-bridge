<script setup lang="ts">
import { type ProviderModelResponse } from "@bindings/ProviderModelResponse";
import { type ProviderResponse } from "@bindings/ProviderResponse";
import { Globe } from "@lucide/vue";

import { useApiCall } from "~/composables/useApiCall";
import { useReactiveMap, useReactiveSet } from "~/composables/useReactiveCollections";
import { getApi } from "~/lib/api";
import { SKELETON_ROWS } from "~/lib/constants";
import { protocolViewToInput } from "~/lib/utils/provider";
import { useAuthStore } from "~/stores/auth";

const api = getApi();
const authStore = useAuthStore();
const { isAdmin } = storeToRefs(authStore);

const providers = ref<ProviderResponse[]>([]);
const expandedId = ref<number | null>(null);
const modelsCache = useReactiveMap<number, ProviderModelResponse[]>();
const modelsLoading = useReactiveSet<number>();

// Edit dialog
const editingProvider = ref<ProviderResponse | null>(null);
const editDialogOpen = ref(false);

// Delete dialog
const deleteDialogOpen = ref(false);
const deleteTarget = ref<{
  type: "provider" | "model";
  providerId: number;
  providerName: string;
  modelId?: number;
  modelName?: string;
} | null>(null);

const { loading, error, execute: fetchProviders } = useApiCall(() => api.admin.listProviders());

async function loadProviders() {
  const result = await fetchProviders();
  if (result) providers.value = result;
}

watchEffect(() => {
  if (isAdmin.value) loadProviders();
});

// --- Expand / models ---
async function toggleModels(providerId: number) {
  if (expandedId.value === providerId) {
    expandedId.value = null;
    return;
  }
  expandedId.value = providerId;
  if (!modelsCache.has(providerId)) {
    modelsLoading.add(providerId);
    try {
      const models = await api.admin.listProviderModels(String(providerId));
      modelsCache.set(providerId, models);
    } catch (e: any) {
      error.value = e.message;
    } finally {
      modelsLoading.delete(providerId);
    }
  }
}

// --- Toggle provider enabled ---
async function handleToggle(p: ProviderResponse) {
  error.value = "";
  try {
    const protocols = p.protocols.map(protocolViewToInput);
    await api.admin.updateProvider(String(p.id), {
      displayName: p.displayName,
      enabled: !p.enabled,
      priority: p.priority,
      apiKeys: p.apiKeys.map((k: any) => ({ label: k.label, key: "", weight: k.weight })),
      protocols,
      quotaAdapter: p.quotaAdapter,
      quotaAdapterConfig: p.quotaAdapterConfig,
    });
    loadProviders();
  } catch (e: any) {
    error.value = e.message;
  }
}

// --- Toggle model enabled ---
async function handleToggleModel(providerId: number, m: ProviderModelResponse) {
  error.value = "";
  try {
    await api.admin.updateProviderModel(String(providerId), String(m.id), {
      providerModelId: m.providerModelId,
      protocolId: m.protocolId,
      displayName: m.displayName,
      maxInputTokens: m.maxInputTokens ?? 0,
      maxOutputTokens: m.maxOutputTokens ?? 0,
      toolCalling: m.toolCalling ?? false,
      vision: m.vision ?? false,
      thinking: m.thinking ?? false,
      adaptiveThinking: m.adaptiveThinking ?? false,
      inputPricePer1m: m.inputPricePer1m,
      outputPricePer1m: m.outputPricePer1m,
      cacheReadPricePer1m: m.cacheReadPricePer1m,
      enabled: !m.enabled,
    });
    if (expandedId.value === providerId) {
      modelsCache.delete(providerId);
      modelsLoading.add(providerId);
      try {
        const models = await api.admin.listProviderModels(String(providerId));
        modelsCache.set(providerId, models);
      } catch (_: any) {
        /* ignore */
      } finally {
        modelsLoading.delete(providerId);
      }
    }
  } catch (e: any) {
    error.value = e.message;
  }
}

// --- Edit ---
function openEditDialog(p: ProviderResponse) {
  editingProvider.value = p;
  editDialogOpen.value = true;
}
function closeEditDialog() {
  editDialogOpen.value = false;
  editingProvider.value = null;
}

// --- Delete ---
function openDeleteDialog(
  type: "provider" | "model",
  providerId: number,
  providerName: string,
  modelId?: number,
  modelName?: string,
) {
  deleteTarget.value = { type, providerId, providerName, modelId, modelName };
  deleteDialogOpen.value = true;
}

async function confirmDelete() {
  const t = deleteTarget.value;
  if (!t) return;
  error.value = "";
  deleteDialogOpen.value = false;
  deleteTarget.value = null;
  try {
    if (t.type === "provider") {
      await api.admin.deleteProvider(String(t.providerId));
      loadProviders();
    } else if (t.type === "model" && t.modelId !== undefined) {
      await api.admin.deleteProviderModel(String(t.providerId), String(t.modelId));
      expandedId.value = t.providerId;
      modelsCache.delete(t.providerId);
      toggleModels(t.providerId);
    }
  } catch (e: any) {
    error.value = e.message;
  }
}
</script>

<template>
  <PageShell>
    <SectionHeader title="提供者管理" description="配置上游 LLM 提供者" :icon="Globe">
      <template #actions>
        <ProviderCreateDialog @created="loadProviders" @error="(e: string) => (error = e)" />
      </template>
    </SectionHeader>

    <ErrorState v-if="error" :error="error" inline @retry="loadProviders" />

    <!-- Loading -->
    <div v-if="loading" class="flex flex-col gap-3">
      <Skeleton v-for="i in SKELETON_ROWS.providers" :key="i" class="h-16 w-full rounded-lg" />
    </div>

    <!-- Empty -->
    <EmptyState
      v-else-if="providers.length === 0"
      :icon="Globe"
      title="暂无提供者，点击上方按钮添加"
    />

    <!-- List -->
    <div v-else class="flex flex-col gap-2 overflow-auto">
      <ProviderRow
        v-for="p in providers"
        :key="p.id"
        :provider="p"
        :expanded="expandedId === p.id"
        :models="modelsCache.get(p.id) || []"
        :models-loading="modelsLoading.has(p.id)"
        @toggle-expand="toggleModels(p.id)"
        @toggle-enabled="handleToggle(p)"
        @edit="openEditDialog(p)"
        @delete-provider="openDeleteDialog('provider', p.id, p.displayName || p.providerId)"
        @toggle-model="(m: ProviderModelResponse) => handleToggleModel(p.id, m)"
        @delete-model="
          (m: ProviderModelResponse) =>
            openDeleteDialog('model', p.id, p.displayName || p.providerId, m.id, m.modelName)
        "
        @error="(e: string) => (error = e)"
      />
    </div>

    <!-- Edit dialog -->
    <ProviderCreateDialog
      v-if="editingProvider"
      :provider="editingProvider"
      v-model:open="editDialogOpen"
      @updated="
        loadProviders();
        closeEditDialog();
      "
      @error="(e: string) => (error = e)"
    />

    <!-- Delete confirmation -->
    <Dialog :open="deleteDialogOpen" @update:open="(v: boolean) => (deleteDialogOpen = v)">
      <DialogContent class="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle class="font-mono text-sm">确认删除</DialogTitle>
        </DialogHeader>
        <div class="flex flex-col gap-4">
          <p v-if="deleteTarget?.type === 'provider'" class="text-sm text-muted-foreground">
            确定要删除提供者
            <span class="font-mono font-medium text-foreground">{{
              deleteTarget.providerName
            }}</span>
            吗？该操作会同时删除其下所有模型，且不可撤销。
          </p>
          <p v-else-if="deleteTarget?.type === 'model'" class="text-sm text-muted-foreground">
            确定要删除模型
            <span class="font-mono font-medium text-foreground">{{ deleteTarget.modelName }}</span
            >（属于 <span class="font-mono text-foreground">{{ deleteTarget.providerName }}</span
            >）吗？该操作不可撤销。
          </p>
          <div class="flex gap-2">
            <Button
              variant="outline"
              class="flex-1 cursor-pointer"
              @click="deleteDialogOpen = false"
              >取消</Button
            >
            <Button variant="destructive" class="flex-1 cursor-pointer" @click="confirmDelete"
              >确认删除</Button
            >
          </div>
        </div>
      </DialogContent>
    </Dialog>
  </PageShell>
</template>

<route lang="json">
{
  "meta": { "requiresAdmin": true }
}
</route>
