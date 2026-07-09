<script setup lang="ts">
import { type ApiKeyEntry } from "@bindings/ApiKeyEntry";
import { type ProtocolInput } from "@bindings/ProtocolInput";
import { type ProviderQuotaAdapter } from "@bindings/ProviderQuotaAdapter";
import { type ProviderResponse } from "@bindings/ProviderResponse";
import { Plus } from "@lucide/vue";

import ProviderForm from "./ProviderForm.vue";
import { getApi } from "~/lib/api";
import {
  emptyProtocol,
  protocolViewToInput,
  buildQuotaConfigString,
  parseQuotaConfigString,
} from "~/lib/utils/provider";

const api = getApi();

const props = defineProps<{ provider?: ProviderResponse | null }>();
const openExternal = defineModel<boolean>("open", { default: false });

const emit = defineEmits<{
  created: [];
  updated: [];
  error: [msg: string];
}>();

// ── mode ──
const isEdit = ref(false);
const showCreate = ref(false);

// ── form state ──
const providerId = ref("");
const displayName = ref("");
const apiKeys = ref<ApiKeyEntry[]>([]);
const protocols = ref<ProtocolInput[]>([]);
const protocolDraft = ref<ProtocolInput | null>(null);
const protocolDraftIndex = ref<number | null>(null);
const enabled = ref(true);
const priority = ref(100);
const quotaAdapter = ref<ProviderQuotaAdapter | null>(null);
const quotaBaseUrl = ref("");
const quotaKeyLabelFilter = ref("");

function syncFormFromProvider() {
  const p = props.provider;
  if (p) {
    isEdit.value = true;
    providerId.value = p.providerId;
    displayName.value = p.displayName;
    apiKeys.value = p.apiKeys.map((k) => ({ label: k.label, key: "", weight: k.weight }));
    protocols.value = p.protocols.map(protocolViewToInput);
    enabled.value = p.enabled;
    priority.value = p.priority;
    quotaAdapter.value = p.quotaAdapter;
    const cfg = p.quotaAdapterConfig ? parseQuotaConfigString(p.quotaAdapterConfig) : null;
    quotaBaseUrl.value = cfg?.baseUrl ?? "";
    quotaKeyLabelFilter.value = cfg?.keyLabelFilter ?? "";
  } else {
    resetForm();
  }
}

function resetForm() {
  isEdit.value = false;
  providerId.value = "";
  displayName.value = "";
  apiKeys.value = [];
  protocols.value = [];
  protocolDraft.value = null;
  protocolDraftIndex.value = null;
  enabled.value = true;
  priority.value = 100;
  quotaAdapter.value = null;
  quotaBaseUrl.value = "";
  quotaKeyLabelFilter.value = "";
}

// 编辑模式下，v-if 创建组件时 open 已经是 true，watch 不会触发，
// 因此 onMounted 主动填充表单。watch 负责后续 open→close→open 的循环。
onMounted(() => {
  if (props.provider) syncFormFromProvider();
});
watch(openExternal, (v, oldV) => {
  // 从 false→true 时重新填充（关闭后再次打开同一个编辑窗口）
  if (v && !oldV && props.provider) syncFormFromProvider();
});

function addApiKey() {
  apiKeys.value = [
    ...apiKeys.value,
    { label: `key-${apiKeys.value.length + 1}`, key: "", weight: 1 },
  ];
}
function removeApiKey(i: number) {
  apiKeys.value = apiKeys.value.filter((_, j) => j !== i);
}

function openProtocolEditor(index?: number) {
  if (index !== undefined && protocols.value[index]) {
    protocolDraft.value = { ...protocols.value[index] };
    protocolDraftIndex.value = index;
  } else {
    protocolDraft.value = emptyProtocol();
    protocolDraftIndex.value = null;
  }
}
function confirmProtocolDraft() {
  const d = protocolDraft.value;
  if (!d) return;
  if (!d.baseUrl.trim()) {
    emit("error", "协议端点 URL 必填");
    return;
  }
  if (protocolDraftIndex.value !== null && protocols.value[protocolDraftIndex.value]) {
    const list = [...protocols.value];
    list[protocolDraftIndex.value] = d;
    protocols.value = list;
  } else {
    protocols.value = [...protocols.value, d];
  }
  protocolDraft.value = null;
  protocolDraftIndex.value = null;
}
function cancelProtocolDraft() {
  protocolDraft.value = null;
  protocolDraftIndex.value = null;
}
function removeProtocol(i: number) {
  protocols.value = protocols.value.filter((_, j) => j !== i);
}

async function handleSubmit() {
  if (!isEdit.value && !providerId.value.trim()) {
    emit("error", "提供者 ID 必填");
    return;
  }
  const cleanedKeys = apiKeys.value.filter((k) => k.label.trim() || k.key.trim());
  const cfg = buildQuotaConfigString(quotaBaseUrl.value, quotaKeyLabelFilter.value);
  try {
    if (isEdit.value && props.provider) {
      await api.admin.updateProvider(String(props.provider.id), {
        displayName: displayName.value.trim() || props.provider.providerId,
        apiKeys: cleanedKeys,
        protocols: protocols.value,
        enabled: enabled.value,
        priority: priority.value,
        quotaAdapter: quotaAdapter.value,
        quotaAdapterConfig: cfg,
      });
      emit("updated");
      openExternal.value = false;
    } else {
      await api.admin.createProvider({
        providerId: providerId.value.trim(),
        displayName: displayName.value.trim() || providerId.value.trim(),
        apiKeys: cleanedKeys,
        protocols: protocols.value,
        enabled: enabled.value,
        priority: priority.value,
        quotaAdapter: quotaAdapter.value,
        quotaAdapterConfig: cfg,
      });
      emit("created");
      showCreate.value = false;
      resetForm();
    }
  } catch (e: any) {
    emit("error", e.message);
  }
}
</script>

<template>
  <!-- Edit mode -->
  <Dialog
    v-if="provider"
    :open="openExternal"
    @update:open="
      (v: boolean) => {
        openExternal = v;
        if (!v) resetForm();
      }
    "
  >
    <DialogContent class="sm:max-w-4xl">
      <DialogHeader
        ><DialogTitle class="font-mono">编辑提供者 · {{ providerId }}</DialogTitle></DialogHeader
      >
      <ProviderForm
        v-model:is-edit="isEdit"
        v-model:provider-id="providerId"
        v-model:display-name="displayName"
        v-model:api-keys="apiKeys"
        v-model:protocols="protocols"
        v-model:protocol-draft="protocolDraft"
        v-model:enabled="enabled"
        v-model:priority="priority"
        v-model:quota-adapter="quotaAdapter"
        v-model:quota-base-url="quotaBaseUrl"
        v-model:quota-key-label-filter="quotaKeyLabelFilter"
        @add-api-key="addApiKey"
        @remove-api-key="removeApiKey"
        @open-protocol-editor="openProtocolEditor"
        @confirm-protocol-draft="confirmProtocolDraft"
        @cancel-protocol-draft="cancelProtocolDraft"
        @remove-protocol="removeProtocol"
        @submit="handleSubmit"
        @cancel="openExternal = false"
      />
    </DialogContent>
  </Dialog>

  <!-- Create mode -->
  <Dialog
    v-else
    :open="showCreate"
    @update:open="
      (v) => {
        if (!v) resetForm();
        showCreate = v;
      }
    "
  >
    <DialogTrigger as-child>
      <Button
        class="cursor-pointer gap-2 bg-[#22C55E] font-medium text-black hover:bg-[#16A34A]"
        @click="
          resetForm();
          showCreate = true;
        "
      >
        <Plus class="h-4 w-4" /> 添加自定义提供者
      </Button>
    </DialogTrigger>
    <DialogContent class="sm:max-w-4xl">
      <DialogHeader><DialogTitle class="font-mono">添加自定义提供者</DialogTitle></DialogHeader>
      <ProviderForm
        v-model:is-edit="isEdit"
        v-model:provider-id="providerId"
        v-model:display-name="displayName"
        v-model:api-keys="apiKeys"
        v-model:protocols="protocols"
        v-model:protocol-draft="protocolDraft"
        v-model:enabled="enabled"
        v-model:priority="priority"
        v-model:quota-adapter="quotaAdapter"
        v-model:quota-base-url="quotaBaseUrl"
        v-model:quota-key-label-filter="quotaKeyLabelFilter"
        @add-api-key="addApiKey"
        @remove-api-key="removeApiKey"
        @open-protocol-editor="openProtocolEditor"
        @confirm-protocol-draft="confirmProtocolDraft"
        @cancel-protocol-draft="cancelProtocolDraft"
        @remove-protocol="removeProtocol"
        @submit="handleSubmit"
        @cancel="showCreate = false"
      />
    </DialogContent>
  </Dialog>
</template>
