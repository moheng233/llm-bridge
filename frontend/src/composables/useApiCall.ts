// 通用 API 调用 composable — 统一 loading / error 处理，消除各页面重复样板代码。
// 错误自动经 formatApiError 转为中文友好文案。
//
// 用法：
//   const { data, loading, error, errorDetail, execute } = useApiCall(() => api.admin.listProviders());
//   await execute();  // 首次手动调用
//   // 或者带参数：
//   const { data, loading, error, execute } = useApiCall((id: string) => api.admin.getProvider(id));
//   await execute("123");

import { formatApiError } from "~/lib/utils/error";

export function useApiCall<T, A extends unknown[]>(fn: (...args: A) => Promise<T>) {
  const data = ref<T | null>(null);
  const loading = ref(false);
  /** 中文友好主文案 */
  const error = ref("");
  /** 原始技术细节（tooltip / 调试用） */
  const errorDetail = ref("");

  async function execute(...args: A): Promise<T | null> {
    loading.value = true;
    error.value = "";
    errorDetail.value = "";
    try {
      const result = await fn(...args);
      data.value = result;
      return result;
    } catch (e: any) {
      const formatted = formatApiError(e);
      error.value = formatted.title;
      errorDetail.value = formatted.detail ?? "";
      return null;
    } finally {
      loading.value = false;
    }
  }

  function clearError() {
    error.value = "";
    errorDetail.value = "";
  }

  return { data, loading, error, errorDetail, execute, clearError };
}
