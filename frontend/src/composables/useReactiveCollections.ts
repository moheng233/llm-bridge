// 响应式 Map / Set 辅助 composable — 消除手动 new Map(...) / new Set(...) 克隆的样板代码。
//
// 用法：
//   const cache = useReactiveMap<number, ModelLinkView[]>();
//   cache.set(1, links);       // 自动触发响应式
//   cache.get(1);              // 读取
//   cache.delete(1);           // 删除
//   cache.has(1);              // 检查
//
//   const loading = useReactiveSet<number>();
//   loading.add(1);            // 自动触发响应式
//   loading.delete(1);
//   loading.has(1);

import { shallowRef } from "vue";

/**
 * 响应式 Map — 包装 shallowRef，每次写入自动替换引用以触发响应式更新。
 * 适用于键值缓存（如 linksCache、modelsCache）。
 */
export function useReactiveMap<K, V>() {
  const inner = shallowRef(new Map<K, V>());

  function get(key: K): V | undefined {
    return inner.value.get(key);
  }

  function set(key: K, value: V): void {
    const m = new Map(inner.value);
    m.set(key, value);
    inner.value = m;
  }

  function del(key: K): void {
    const m = new Map(inner.value);
    m.delete(key);
    inner.value = m;
  }

  function has(key: K): boolean {
    return inner.value.has(key);
  }

  function clear(): void {
    inner.value = new Map();
  }

  return { get, set, delete: del, has, clear, raw: inner };
}

/**
 * 响应式 Set — 包装 shallowRef，每次写入自动替换引用以触发响应式更新。
 * 适用于加载状态追踪（如 linksLoading、modelsLoading）。
 */
export function useReactiveSet<T>() {
  const inner = shallowRef(new Set<T>());

  function add(value: T): void {
    const s = new Set(inner.value);
    s.add(value);
    inner.value = s;
  }

  function del(value: T): void {
    const s = new Set(inner.value);
    s.delete(value);
    inner.value = s;
  }

  function has(value: T): boolean {
    return inner.value.has(value);
  }

  function clear(): void {
    inner.value = new Set();
  }

  return { add, delete: del, has, clear, raw: inner };
}
