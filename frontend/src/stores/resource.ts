// 通用 ResourceStore composable — 封装 API 调用的加载/错误/缓存逻辑
// 替代原 @tanstack/svelte-store 的 ResourceStore<T> 类
// 被各 Pinia store 的 setup 函数内部调用

import { ref, shallowRef, type Ref, type ShallowRef } from "vue";

export interface ResourceState<T> {
  data: ShallowRef<T | null>;
  loading: Ref<boolean>;
  error: Ref<unknown>;
  loaded: Ref<boolean>;
}

/**
 * createResourceStore(fetcher) → { data, loading, error, loaded, load(), invalidate(), reset(), mutate() }
 *
 * - data: shallowRef<T | null> — 使用 shallowRef 提高大数据列表性能
 * - load(): 并发去重，若已有进行中的请求则返回同一 promise
 * - invalidate(): 强制重新加载
 * - mutate(): 乐观更新本地数据（不触发网络请求）
 * - reset(): 清空所有状态
 */
export function createResourceStore<T>(fetcher: () => Promise<T>) {
  const data = shallowRef<T | null>(null) as ShallowRef<T | null>;
  const loading = ref(false);
  const error = ref<unknown>(null);
  const loaded = ref(false);

  let promise: Promise<void> | null = null;

  async function load(force = false): Promise<void> {
    if (promise && !force) return promise;
    loading.value = true;
    error.value = null;
    promise = (async () => {
      try {
        data.value = await fetcher();
        loaded.value = true;
      } catch (e) {
        error.value = e;
        loaded.value = true;
      } finally {
        loading.value = false;
        promise = null;
      }
    })();
    return promise;
  }

  function invalidate(): Promise<void> {
    return load(true);
  }

  function markStale() {
    loaded.value = false;
  }

  function reset() {
    data.value = null;
    loading.value = false;
    error.value = null;
    loaded.value = false;
    promise = null;
  }

  function mutate(updater: (current: T | null) => T | null) {
    data.value = updater(data.value);
  }

  return {
    data: data as Readonly<ShallowRef<T | null>>,
    loading: loading as Readonly<Ref<boolean>>,
    error: error as Readonly<Ref<unknown>>,
    loaded: loaded as Readonly<Ref<boolean>>,
    load,
    invalidate,
    markStale,
    reset,
    mutate,
  };
}

/**
 * createMapResourceStore(fetcher) — 按 key 分组的资源 store
 * 如 Provider 下的 models、Model 下的 provider links
 */
export function createMapResourceStore<T>(fetcher: (key: string) => Promise<T>) {
  const stores = new Map<string, ReturnType<typeof createResourceStore<T>>>();
  const promises = new Map<string, Promise<void>>();

  function get(key: string) {
    let s = stores.get(key);
    if (!s) {
      s = createResourceStore(() => fetcher(key));
      stores.set(key, s);
    }
    return s;
  }

  async function load(key: string, force = false) {
    if (promises.has(key) && !force) return promises.get(key)!;
    const store = get(key);
    const p = store.load(force);
    promises.set(key, p);
    return p;
  }

  function invalidate(key: string) {
    return load(key, true);
  }

  function invalidateAll() {
    for (const key of stores.keys()) {
      load(key, true);
    }
  }

  function remove(key: string) {
    stores.delete(key);
    promises.delete(key);
  }

  function reset() {
    stores.clear();
    promises.clear();
  }

  return { get, load, invalidate, invalidateAll, remove, reset };
}
