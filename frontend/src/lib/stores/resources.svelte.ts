// 资源 store 集中导出 — 见 PLAN.md §10 Phase B B.1。
// 每个资源一个 store，封装 fetch + cache + invalidate，基于 @tanstack/svelte-store。
//
// 组件读取约定（useSelector 必须在组件实例上下文中调用）：
//   import { useSelector } from "@tanstack/svelte-store";
//   const data = useSelector(tokensStore.source, (s) => s.data);
//   const loading = useSelector(tokensStore.source, (s) => s.loading);
//   // data.current / loading.current 读取
//
// 触发加载 / 失效：
//   $effect(() => { if (auth.isAuthenticated) tokensStore.load(); });
//   await api.tokens.createToken(...);
//   tokensStore.invalidate();
//   tokensStore.reset();                // 登出清理
//   tokensStore.mutate((d) => d?.filter(...) ?? null);  // 乐观更新

import { getApi } from "$lib/api";
import { ResourceStore, MapResourceStore } from "./resource.svelte";
import type { ModelResponse } from "$bindings/ModelResponse";
import type { TokenListItem } from "$bindings/TokenListItem";
import type { ProviderResponse } from "$bindings/ProviderResponse";
import type { ProviderModelResponse } from "$bindings/ProviderModelResponse";
import type { AdminModelResponse } from "$bindings/AdminModelResponse";
import type { ModelLinkView } from "$bindings/ModelLinkView";
import type { UserResponse } from "$bindings/UserResponse";

// ── 模型目录（普通用户） ──
// 仅可用过滤通过 fetcher 闭包读取实例状态；setOnlyAvailable 切换后调 invalidate。

class ModelsStore extends ResourceStore<ModelResponse[]> {
  private onlyAvailable = false;

  constructor() {
    super(() => {
      const api = getApi();
      return this.onlyAvailable
        ? api.models.listAvailableModels()
        : api.models.listAllModels();
    });
  }

  get isOnlyAvailable() {
    return this.onlyAvailable;
  }

  setOnlyAvailable(v: boolean) {
    if (this.onlyAvailable !== v) {
      this.onlyAvailable = v;
      this.invalidate();
    }
  }
}

export const modelsStore = new ModelsStore();

// ── API Tokens ──
export const tokensStore = new ResourceStore<TokenListItem[]>(
  () => getApi().tokens.listTokens(),
);

// ── Providers（管理员） ──
export const providersStore = new ResourceStore<ProviderResponse[]>(
  () => getApi().admin.listProviders(),
);

// ── Provider 下的 models（按 providerId 分组） ──
export const providerModelsStore = new MapResourceStore<ProviderModelResponse[]>(
  (providerId) => getApi().admin.listProviderModels(providerId),
);

// ── Admin 模型列表 ──
// 模型管理页同时需要 models 与 providers（providers 给 link 下拉用）。
// adminModelsStore 只承载 models 数据；providersStore 作为 providers 的单一数据源。
export const adminModelsStore = new ResourceStore<AdminModelResponse[]>(
  async () => {
    const [models, providers] = await Promise.all([
      getApi().admin.listAdminModels(),
      getApi().admin.listProviders(),
    ]);
    // 把 providers 同步到 providersStore（单一数据源），避免两个 store 各存一份
    providersStore.mutate(() => providers);
    return models;
  },
);

// ── Model 下的 provider 连接（按 modelId 分组） ──
export const modelLinksStore = new MapResourceStore<ModelLinkView[]>((modelId) =>
  getApi().admin.listModelProviders(modelId),
);

// ── 用户列表（管理员） ──
export const usersStore = new ResourceStore<UserResponse[]>(
  () => getApi().admin.listUsers(),
);

// ── 登出时清理所有 store ──
export function resetAllStores() {
  modelsStore.reset();
  tokensStore.reset();
  providersStore.reset();
  providerModelsStore.reset();
  adminModelsStore.reset();
  modelLinksStore.reset();
  usersStore.reset();
}
