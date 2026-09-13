import { type AdminModelResponse } from "@bindings/AdminModelResponse";
import { type CatalogModelPreview } from "@bindings/CatalogModelPreview";
import { type CatalogPreview } from "@bindings/CatalogPreview";
import { ApiError } from "@bindings/client";
import { type CreateModelConnectionsRequest } from "@bindings/CreateModelConnectionsRequest";
import { type CreateModelConnectionsResponse } from "@bindings/CreateModelConnectionsResponse";
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";

import { getApi } from "~/lib/api";
import {
  connectionDraftToInput,
  validateConnectionDraft,
  type ConnectionDraft,
} from "~/lib/connection-draft";
import {
  modelToDraft,
  modelDraftToInput,
  validateModelDraft,
  type ModelDraft,
} from "~/lib/model-form";
import { providerToDraft, validateProviderDraft } from "~/lib/utils/provider";
import { useConnectionTestsStore } from "~/stores/connection-tests";
export type SetupContext = { providerId?: number; modelId?: number; templateProviderId?: string };
export interface SetupConnection {
  key: string;
  modelKey: string;
  existingModelId: number | null;
  link: ConnectionDraft;
}
export function useConnectionSetup(context: SetupContext) {
  const api = getApi();
  const router = useRouter();
  const step = ref<"provider" | "configuration" | "models" | "result">("provider");
  const catalog = shallowRef<CatalogPreview | null>(null);
  const savedProvider = ref<ProviderResponse | null>(null);
  const providers = ref<ProviderResponse[]>([]);
  const models = ref<AdminModelResponse[]>([]);
  const lockedModel = ref<AdminModelResponse | null>(null);
  const providerDraft = ref(providerToDraft(null));
  const providerBaseline = ref(JSON.stringify(providerDraft.value));
  const modelDrafts = reactive<Record<string, ModelDraft>>({});
  const selectedConnections = ref<SetupConnection[]>([]);
  const localLinks = ref<(ModelLinkView & { modelId: number })[]>([]);
  const fieldErrors = ref<Record<string, string>>({});
  const result = ref<CreateModelConnectionsResponse | null>(null);
  const error = ref("");
  const contextError = ref("");
  const uncertain = ref(false);
  const providerRecovery = ref<ProviderResponse | null>(null);
  const providerRecoveryChecked = ref(false);
  let pendingProviderId = "";
  const activeKey = ref("");
  const referenceProvider = ref(context.templateProviderId ?? "");
  const conflictingModel = ref<AdminModelResponse | null>(null);
  const conflictKey = ref("");
  let sequence = 0;
  let modelIndex = new Map<string, CatalogModelPreview>();
  const catalogCall = useApiCall(() => api.modelsImport.preview());
  const loadCall = useApiCall(() =>
    Promise.all([api.admin.listProviders(), api.admin.listAdminModels()]),
  );
  const linksCall = useApiCall(async (providerId: number) => {
    const rows = await api.admin.listProviderModels(String(providerId));
    const output: (ModelLinkView & { modelId: number })[] = [];
    for (const model of models.value.filter((model) =>
      rows.some((row) => row.modelName === model.modelName),
    )) {
      const links = await api.admin.listModelProviders(String(model.id));
      output.push(
        ...links
          .filter((link) => link.providerId === providerId)
          .map((link) => ({ ...link, modelId: model.id })),
      );
    }
    return output;
  });
  let failure: unknown;
  const saveCall = useApiCall(async () => {
    try {
      return savedProvider.value
        ? await api.admin.updateProvider(String(savedProvider.value.id), providerDraft.value)
        : await api.admin.createProvider(providerDraft.value);
    } catch (e) {
      failure = e;
      throw e;
    }
  });
  const batchCall = useApiCall(async (body: CreateModelConnectionsRequest) => {
    try {
      return await api.admin.createModelConnections(body);
    } catch (e) {
      failure = e;
      throw e;
    }
  });
  const saving = computed(() => saveCall.loading.value || batchCall.loading.value);
  const loading = computed(() => loadCall.loading.value || linksCall.loading.value);
  const dirty = computed(
    () =>
      !result.value &&
      (JSON.stringify(providerDraft.value) !== providerBaseline.value ||
        selectedConnections.value.length > 0),
  );
  async function loadCatalog() {
    const preview = await catalogCall.execute();
    if (preview) {
      catalog.value = preview;
      modelIndex = new Map(preview.models.map((row) => [row.model.modelName, row]));
    }
  }
  function reset() {
    providerDraft.value.apiKeys = [];
    savedProvider.value = null;
    providerDraft.value = providerToDraft(null);
    providerBaseline.value = JSON.stringify(providerDraft.value);
    selectedConnections.value = [];
    for (const key of Object.keys(modelDrafts)) delete modelDrafts[key];
    localLinks.value = [];
    fieldErrors.value = {};
    result.value = null;
    error.value = "";
    uncertain.value = false;
    providerRecovery.value = null;
    providerRecoveryChecked.value = false;
    pendingProviderId = "";
    conflictingModel.value = null;
    activeKey.value = "";
    step.value = "provider";
  }
  async function load() {
    contextError.value = "";
    for (const id of [context.providerId, context.modelId])
      if (id !== undefined && (!Number.isSafeInteger(id) || id < 1)) {
        contextError.value = "记录不存在或已删除";
        return;
      }
    const data = await loadCall.execute();
    if (!data) {
      contextError.value = loadCall.error.value;
      return;
    }
    [providers.value, models.value] = data;
    if (context.modelId !== undefined) {
      lockedModel.value = models.value.find((model) => model.id === context.modelId) ?? null;
      if (!lockedModel.value) {
        contextError.value = "记录不存在或已删除";
        return;
      }
    }
    if (context.providerId !== undefined) {
      if (!providers.value.some((provider) => provider.id === context.providerId)) {
        contextError.value = "记录不存在或已删除";
        return;
      }
      await selectProvider(context.providerId);
    } else if (context.templateProviderId) {
      await loadCatalog();
      if (
        catalog.value &&
        !catalog.value.providers.some(
          (row) => row.provider.providerId === context.templateProviderId,
        )
      ) {
        contextError.value = "目录提供者不存在";
        return;
      }
      if (catalog.value) await selectProvider(context.templateProviderId);
    }
  }
  async function selectProvider(value: number | string | null) {
    if (saving.value) return;
    selectedConnections.value = [];
    result.value = null;
    error.value = "";
    uncertain.value = false;
    providerRecovery.value = null;
    providerRecoveryChecked.value = false;
    pendingProviderId = "";
    fieldErrors.value = {};
    localLinks.value = [];
    for (const key of Object.keys(modelDrafts)) delete modelDrafts[key];
    if (typeof value === "number") {
      const call = useApiCall(() => api.admin.getProvider(String(value)));
      const provider = await call.execute();
      if (!provider) {
        contextError.value = call.error.value;
        return;
      }
      savedProvider.value = provider;
      providerDraft.value = providerToDraft(provider);
      providerBaseline.value = JSON.stringify(providerDraft.value);
      referenceProvider.value = context.templateProviderId ?? provider.providerId;
      const links = await linksCall.execute(provider.id);
      if (!links) {
        error.value = linksCall.error.value;
        return;
      }
      localLinks.value = links;
      step.value = provider.protocols.some((protocol) => protocol.enabled)
        ? "models"
        : "configuration";
      if (step.value === "models" && lockedModel.value) addLocal(lockedModel.value.id);
    } else {
      savedProvider.value = null;
      providerDraft.value = providerToDraft(null);
      if (value !== null) {
        const template = catalog.value?.providers.find(
          (row) => row.provider.providerId === value,
        )?.provider;
        if (!template) {
          error.value = "请重新加载目录后选择提供者";
          return;
        }
        providerDraft.value.providerId = providers.value.some(
          (provider) => provider.providerId === value,
        )
          ? ""
          : template.providerId;
        providerDraft.value.displayName = template.displayName;
        providerDraft.value.protocols[0]!.protocol = template.compat;
        providerDraft.value.protocols[0]!.baseUrl = template.baseUrl;
        referenceProvider.value = template.providerId;
      } else referenceProvider.value = "";
      providerBaseline.value = JSON.stringify(providerToDraft(null));
      step.value = "configuration";
    }
  }
  async function acceptProvider(provider: ProviderResponse) {
    savedProvider.value = provider;
    providerDraft.value = providerToDraft(provider);
    providerBaseline.value = JSON.stringify(providerDraft.value);
    useConnectionTestsStore().invalidateProvider(provider.id);
    providers.value = [...providers.value.filter((row) => row.id !== provider.id), provider];
    await router.replace({
      path: "/admin/setup",
      query: {
        providerId: String(provider.id),
        ...(lockedModel.value ? { modelId: String(lockedModel.value.id) } : {}),
        ...(referenceProvider.value ? { templateProviderId: referenceProvider.value } : {}),
      },
    });
  }
  async function useRecoveredProvider() {
    if (saving.value || loading.value || !uncertain.value || !providerRecovery.value) return;
    await acceptProvider(providerRecovery.value);
    uncertain.value = false;
    providerRecovery.value = null;
    providerRecoveryChecked.value = false;
    pendingProviderId = "";
    fieldErrors.value = {};
    error.value = "";
  }
  function allowProviderRetry() {
    if (
      saving.value ||
      loading.value ||
      !providerRecoveryChecked.value ||
      providerRecovery.value ||
      savedProvider.value
    )
      return;
    uncertain.value = false;
    providerRecoveryChecked.value = false;
    pendingProviderId = "";
    error.value = "";
  }
  async function saveProvider(advance = true) {
    if (saving.value || loading.value || uncertain.value) return;
    fieldErrors.value = validateProviderDraft(providerDraft.value, savedProvider.value);
    if (advance && !providerDraft.value.protocols.some((protocol) => protocol.enabled))
      fieldErrors.value.protocols = "添加模型前至少保留一个已启用的有效协议";
    if (
      !savedProvider.value &&
      providers.value.some(
        (provider) => provider.providerId === providerDraft.value.providerId.trim(),
      )
    )
      fieldErrors.value.providerId = "实例 ID 已存在；使用已有提供者或输入新的独立实例 ID";
    if (Object.keys(fieldErrors.value).length) return;
    error.value = "";
    if (!savedProvider.value || JSON.stringify(providerDraft.value) !== providerBaseline.value) {
      failure = undefined;
      const provider = await saveCall.execute();
      if (!provider) {
        if (failure instanceof ApiError && failure.status < 500) {
          error.value = failure.message;
          if (failure.status === 409)
            fieldErrors.value.providerId = "实例 ID 已存在，请使用已有提供者或修改 ID";
        } else {
          uncertain.value = true;
          pendingProviderId = providerDraft.value.providerId;
          providerRecovery.value = null;
          providerRecoveryChecked.value = false;
          error.value = "保存结果待确认；请重新加载本地记录，不要自动重发创建。";
        }
        return;
      }
      await acceptProvider(provider);
    }
    uncertain.value = false;
    if (!advance) {
      await router.push(`/providers/${savedProvider.value!.id}`);
      return;
    }
    step.value = "models";
    if (lockedModel.value && !selectedConnections.value.length) addLocal(lockedModel.value.id);
  }
  function addConnection(key: string, modelKey: string, existingModelId: number | null) {
    const provider = savedProvider.value;
    if (!provider || saving.value || selectedConnections.value.some((item) => item.key === key))
      return;
    const protocols = provider.protocols.filter((protocol) => protocol.enabled);
    const chosen = protocols.length === 1 ? protocols[0]! : null;
    const link: ConnectionDraft = {
      providerId: provider.id,
      protocolId: chosen?.id ?? null,
      providerModelId: modelKey,
      displayName: "",
      maxInput: "",
      maxOutput: "",
      inputPrice: "",
      outputPrice: "",
      cachePrice: "",
      priority: "100",
      toolCalling: null,
      vision: null,
      thinking: null,
      adaptiveThinking: null,
      enabled: true,
    };
    selectedConnections.value.push({
      key,
      modelKey,
      existingModelId,
      link,
    });
    activeKey.value = key;
  }
  function selectCatalogKeys(keys: string[]) {
    if (saving.value) return;
    const selectedKeys = new Set(keys.map((key) => `catalog:${key}`));
    selectedConnections.value = selectedConnections.value.filter(
      (item) => !item.key.startsWith("catalog:") || selectedKeys.has(item.key),
    );
    for (const key of keys) {
      const row = modelIndex.get(key);
      if (!row || (lockedModel.value && lockedModel.value.modelName !== key)) continue;
      const local = models.value.find((model) => model.modelName === key);
      if (!local && !modelDrafts[key]) modelDrafts[key] = modelToDraft(row.model);
      addConnection(`catalog:${key}`, key, local?.id ?? null);
    }
    if (!selectedConnections.value.some((item) => item.key === activeKey.value))
      activeKey.value = selectedConnections.value[0]?.key ?? "";
    fieldErrors.value = {};
  }
  function addLocal(id: number) {
    const model = models.value.find((model) => model.id === id);
    if (model && (!lockedModel.value || lockedModel.value.id === id))
      addConnection(`local-${++sequence}`, model.modelName, model.id);
  }
  function addManual() {
    if (lockedModel.value) {
      addLocal(lockedModel.value.id);
      return;
    }
    const key = `manual-${++sequence}`;
    modelDrafts[key] = {
      modelName: "",
      displayName: "",
      description: "",
      maxInput: "",
      maxOutput: "",
      status: "",
      toolCalling: false,
      vision: false,
      thinking: false,
      adaptiveThinking: false,
    };
    addConnection(key, key, null);
    selectedConnections.value.find((item) => item.key === key)!.link.providerModelId = "";
  }
  function reuseConflict() {
    if (!conflictingModel.value) return;
    for (const item of selectedConnections.value)
      if (item.modelKey === conflictKey.value) item.existingModelId = conflictingModel.value.id;
    conflictingModel.value = null;
    error.value = "已明确复用本地定义，请确认连接字段后再次保存。";
  }
  async function submitConnections() {
    if (saving.value || uncertain.value || !savedProvider.value) return;
    error.value = "";
    fieldErrors.value = {};
    if (!selectedConnections.value.length) {
      error.value = "请至少选择一个连接";
      return;
    }
    const items: CreateModelConnectionsRequest["items"] = [];
    for (const [index, item] of selectedConnections.value.entries()) {
      const errors = validateConnectionDraft(item.link, savedProvider.value);
      for (const [field, message] of Object.entries(errors))
        fieldErrors.value[`${index}.${field}`] = message;
      if (item.existingModelId === null) {
        const draft = modelDrafts[item.modelKey]!;
        for (const [field, message] of Object.entries(validateModelDraft(draft)))
          fieldErrors.value[`${index}.model.${field}`] = message;
        items.push({
          model: { kind: "new", model: modelDraftToInput(draft) },
          link: connectionDraftToInput(item.link),
        });
      } else
        items.push({
          model: { kind: "existing", id: item.existingModelId },
          link: connectionDraftToInput(item.link),
        });
    }
    if (Object.keys(fieldErrors.value).length) {
      activeKey.value =
        selectedConnections.value[Number(Object.keys(fieldErrors.value)[0]!.split(".")[0])]!.key;
      return;
    }
    failure = undefined;
    const saved = await batchCall.execute({ items });
    if (saved) {
      result.value = saved;
      step.value = "result";
      return;
    }
    if (!(failure instanceof ApiError)) {
      uncertain.value = true;
      error.value = "保存结果待确认。请查看已配置连接或重新加载本地记录；不会自动重发创建。";
      return;
    }
    error.value = `模型连接未保存，请修正后重试；提供者 ${savedProvider.value.displayName || savedProvider.value.providerId} 已保存。${failure.message}`;
    const body = failure.body as
      | { itemIndex?: number; field?: string; modelName?: string }
      | undefined;
    if (body?.itemIndex !== undefined) {
      activeKey.value = selectedConnections.value[body.itemIndex]?.key ?? "";
      fieldErrors.value[`${body.itemIndex}.${body.field ?? "model"}`] = failure.message;
    }
    if (failure.status === 409 && failure.message === "model_name_exists") {
      const data = await loadCall.execute();
      if (data) {
        [providers.value, models.value] = data;
        conflictingModel.value =
          models.value.find((model) => model.modelName === body?.modelName) ?? null;
        conflictKey.value = selectedConnections.value[body?.itemIndex ?? 0]?.modelKey ?? "";
      }
    }
  }
  async function reloadLocalRecords() {
    if (saving.value || loading.value) return false;
    providerRecovery.value = null;
    providerRecoveryChecked.value = false;
    const data = await loadCall.execute();
    if (!data) {
      error.value = loadCall.error.value;
      return false;
    }
    [providers.value, models.value] = data;
    const recoveringProvider = step.value === "configuration" && uncertain.value;
    const currentProvider = providers.value.find((provider) =>
      savedProvider.value
        ? provider.id === savedProvider.value.id
        : recoveringProvider && provider.providerId === pendingProviderId,
    );
    const links = currentProvider ? await linksCall.execute(currentProvider.id) : [];
    if (!links) {
      error.value = linksCall.error.value;
      return false;
    }
    localLinks.value = links;
    if (recoveringProvider) {
      providerRecovery.value = currentProvider ?? null;
      providerRecoveryChecked.value = true;
      error.value = "";
      return true;
    }
    error.value = "已重新加载本地记录，请检查已配置连接后确认是否重试。";
    return true;
  }
  onBeforeUnmount(() => {
    providerDraft.value.apiKeys = [];
  });
  return {
    step,
    catalog,
    savedProvider,
    modelDrafts,
    selectedConnections,
    fieldErrors,
    saving,
    result,
    load,
    selectProvider,
    saveProvider,
    submitConnections,
    reset,
    providers,
    models,
    lockedModel,
    providerDraft,
    loading,
    dirty,
    contextError,
    error,
    uncertain,
    providerRecovery,
    providerRecoveryChecked,
    useRecoveredProvider,
    allowProviderRetry,
    activeKey,
    referenceProvider,
    catalogCall,
    loadCatalog,
    selectCatalogKeys,
    addLocal,
    addManual,
    localLinks,
    conflictingModel,
    conflictKey,
    reuseConflict,
    reloadLocalRecords,
  };
}
