// 业务资源 Pinia stores — 集中导出
// 每个 resource 一个 store，封装 fetch + cache + invalidate
//
// 组件中使用：
//   import { storeToRefs } from 'pinia'
//   const store = useTokensStore()
//   const { data, loading } = storeToRefs(store)
//   store.load()

import { getApi } from "~/lib/api";
import { createResourceStore, createMapResourceStore } from "./resource";
import { type ModelResponse } from "@bindings/ModelResponse";
import { type TokenListItem } from "@bindings/TokenListItem";
import { type ProviderResponse } from "@bindings/ProviderResponse";
import { type ProviderModelResponse } from "@bindings/ProviderModelResponse";
import { type AdminModelResponse } from "@bindings/AdminModelResponse";
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type UserResponse } from "@bindings/UserResponse";

// ── 模型目录（普通用户） ──
export const useModelsStore = defineStore("models", () => {
  let onlyAvailable = false;

  const store = createResourceStore<ModelResponse[]>(() => {
    const api = getApi();
    return onlyAvailable ? api.models.listAvailableModels() : api.models.listAllModels();
  });

  function isOnlyAvailable() {
    return onlyAvailable;
  }

  function setOnlyAvailable(v: boolean) {
    if (onlyAvailable !== v) {
      onlyAvailable = v;
      store.invalidate();
    }
  }

  return { ...store, isOnlyAvailable, setOnlyAvailable };
});

// ── API Tokens ──
export const useTokensStore = defineStore("tokens", () => {
  return createResourceStore<TokenListItem[]>(() => getApi().tokens.listTokens());
});

// ── Providers（管理员） ──
export const useProvidersStore = defineStore("providers", () => {
  return createResourceStore<ProviderResponse[]>(() => getApi().admin.listProviders());
});

// ── Provider 下的 models（按 providerId 分组） ──
export const useProviderModelsStore = defineStore("providerModels", () => {
  return createMapResourceStore<ProviderModelResponse[]>((providerId) =>
    getApi().admin.listProviderModels(providerId),
  );
});

// ── Admin 模型列表 ──
export const useAdminModelsStore = defineStore("adminModels", () => {
  const providersStore = useProvidersStore();

  return createResourceStore<AdminModelResponse[]>(async () => {
    const [models, providers] = await Promise.all([
      getApi().admin.listAdminModels(),
      getApi().admin.listProviders(),
    ]);
    // 同步 providers 到 providersStore（单一数据源）
    providersStore.mutate(() => providers);
    return models;
  });
});

// ── Model 下的 provider 连接（按 modelId 分组） ──
export const useModelLinksStore = defineStore("modelLinks", () => {
  return createMapResourceStore<ModelLinkView[]>((modelId) =>
    getApi().admin.listModelProviders(modelId),
  );
});

// ── 用户列表（管理员） ──
export const useUsersStore = defineStore("users", () => {
  return createResourceStore<UserResponse[]>(() => getApi().admin.listUsers());
});
