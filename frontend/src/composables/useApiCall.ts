// 通用 API 调用 composable — 统一 loading / error 处理，消除各页面重复样板代码。
//
// 用法：
//   const { data, loading, error, execute } = useApiCall(() => api.admin.listProviders());
//   await execute();  // 首次手动调用
//   // 或者带参数：
//   const { data, loading, error, execute } = useApiCall((id: string) => api.admin.getProvider(id));
//   await execute("123");

export function useApiCall<T, A extends unknown[]>(
  fn: (...args: A) => Promise<T>,
) {
  const data = ref<T | null>(null);
  const loading = ref(false);
  const error = ref("");

  async function execute(...args: A): Promise<T | null> {
    loading.value = true;
    error.value = "";
    try {
      const result = await fn(...args);
      data.value = result;
      return result;
    } catch (e: any) {
      error.value = e.message;
      return null;
    } finally {
      loading.value = false;
    }
  }

  function clearError() {
    error.value = "";
  }

  return { data, loading, error, execute, clearError };
}
