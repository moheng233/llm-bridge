// 资源 store 基类 — 基于 @tanstack/svelte-store。
// 见 PLAN.md §10 Phase B B.1。
//
// 用 @tanstack/store 的 Store<T> 承载状态；组件通过 useSelector 订阅。
// 保留 load/invalidate/mutate/reset 等便利方法，隔离上层对底层库的感知。
//
// 用法（stores/resources.svelte.ts）：
//   export const tokensStore = new ResourceStore<TokenListItem[]>(() => api.tokens.listTokens());
//
// 组件内：
//   import { useSelector } from "@tanstack/svelte-store";
//   const data = useSelector(tokensStore.source, (s) => s.data);
//   const loading = useSelector(tokensStore.source, (s) => s.loading);
//   // data.current / loading.current 读取
//
//   $effect(() => { if (auth.isAuthenticated) tokensStore.load(); });
//   await api.tokens.createToken(...);
//   tokensStore.invalidate();

import { Store } from "@tanstack/svelte-store";

export interface ResourceState<T> {
  data: T | null;
  loading: boolean;
  error: unknown;
  /** 已加载过至少一次（区分初始态与加载完成态） */
  loaded: boolean;
}

export function emptyState<T>(): ResourceState<T> {
  return { data: null, loading: false, error: null, loaded: false };
}

/**
 * 单一资源 store（list / detail）。
 */
export class ResourceStore<T> {
  protected store: Store<ResourceState<T>>;
  private fetcher: () => Promise<T>;
  private promise: Promise<void> | null = null;

  constructor(fetcher: () => Promise<T>) {
    this.fetcher = fetcher;
    this.store = new Store<ResourceState<T>>(emptyState<T>());
  }

  /** 供 useSelector 订阅的源 */
  get source() {
    return this.store;
  }

  /** 直接读取当前状态（非响应式） */
  get state(): ResourceState<T> {
    return this.store.state;
  }
  get data() {
    return this.store.state.data;
  }
  get loading() {
    return this.store.state.loading;
  }
  get error() {
    return this.store.state.error;
  }
  get loaded() {
    return this.store.state.loaded;
  }
  get hasError() {
    return this.store.state.error !== null;
  }

  /**
   * 加载数据。若已在加载中，返回进行中的 promise（去重）。
   * 已有数据时不会自动清空，loading=true 期间旧数据仍在 UI 中。
   */
  async load(force = false): Promise<void> {
    if (this.promise && !force) return this.promise;
    this.store.setState((s: ResourceState<T>) => ({ ...s, loading: true, error: null }));
    this.promise = (async () => {
      try {
        const data = await this.fetcher();
        this.store.setState(() => ({ data, loading: false, error: null, loaded: true }));
      } catch (e) {
        this.store.setState((s: ResourceState<T>) => ({
          ...s,
          loading: false,
          error: e,
          loaded: true,
        }));
      } finally {
        this.promise = null;
      }
    })();
    return this.promise!;
  }

  /** 失效缓存并重新加载 */
  invalidate(): Promise<void> {
    return this.load(true);
  }

  /** 仅失效缓存（不立即重载，等下次 load） */
  markStale() {
    this.store.setState((s: ResourceState<T>) => ({ ...s, loaded: false }));
  }

  /** 清空数据与错误（如登出时） */
  reset() {
    this.store.setState(() => emptyState<T>());
    this.promise = null;
  }

  /** 乐观更新本地数据（不触发网络） */
  mutate(updater: (data: T | null) => T | null) {
    this.store.setState((s: ResourceState<T>) => ({ ...s, data: updater(s.data) }));
  }

  /** 直接替换 state（高级用，慎用） */
  protected set(next: ResourceState<T>) {
    this.store.setState(() => next);
  }
}

/**
 * 多资源 map store — 用于「按 key 分组加载」的场景，
 * 如 Provider 下的 models、Model 下的 provider links。
 *
 * 每个键独立一个 Store，组件用 useSelector(mapStore.get(key), s => s) 订阅。
 */
export class MapResourceStore<T> {
  private stores = new Map<string, Store<ResourceState<T>>>();
  // 暴露一个集合 store，用于响应式判断"某 key 是否已注册"
  private registry: Store<Set<string>> = new Store<Set<string>>(new Set<string>());

  constructor(private fetcher: (key: string) => Promise<T>) {}

  /** 获取或创建某 key 的 store（响应式源） */
  get(key: string): Store<ResourceState<T>> {
    let s = this.stores.get(key);
    if (!s) {
      s = new Store<ResourceState<T>>(emptyState<T>());
      this.stores.set(key, s);
      this.registry.setState((prev: Set<string>) => new Set(prev).add(key));
    }
    return s;
  }

  /** 直接读取某 key 的当前状态（非响应式） */
  state(key: string): ResourceState<T> {
    return this.get(key).state;
  }

  private promises = new Map<string, Promise<void>>();

  async load(key: string, force = false): Promise<void> {
    if (this.promises.has(key) && !force) return this.promises.get(key)!;
    const store = this.get(key);
    store.setState((s: ResourceState<T>) => ({ ...s, loading: true, error: null }));
    const p = (async () => {
      try {
        const data = await this.fetcher(key);
        store.setState(() => ({ data, loading: false, error: null, loaded: true }));
      } catch (e) {
        store.setState((s: ResourceState<T>) => ({ ...s, loading: false, error: e, loaded: true }));
      } finally {
        this.promises.delete(key);
      }
    })();
    this.promises.set(key, p);
    return p;
  }

  invalidate(key: string): Promise<void> {
    return this.load(key, true);
  }

  /** 失效所有已注册项 */
  invalidateAll() {
    for (const key of this.stores.keys()) {
      this.load(key, true);
    }
  }

  remove(key: string) {
    this.stores.delete(key);
    this.promises.delete(key);
    this.registry.setState((prev: Set<string>) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
  }

  reset() {
    this.stores.clear();
    this.promises.clear();
    this.registry.setState(() => new Set());
  }

  /** 已注册的所有 key（响应式源，少用） */
  get keys() {
    return this.registry;
  }
}
