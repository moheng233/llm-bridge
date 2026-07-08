// 全局 toast 便捷封装 — 见 PLAN.md §9 决策①。
// 用法：
//   import { toast } from "$lib/utils/toast";
//   toast.success("Token 已创建");
//   toast.error("创建失败", { description: detail });
//   try { ... } catch (e) { toast.error(formatApiError(e).title, { description: formatApiError(e).detail }); }
//
// svelte-sonner 通过模块内单例管理 toast 列表，先 import 再 re-export 以便本模块内也可调用。

import { toast as sonnerToast } from "svelte-sonner";
import { formatApiError } from "$lib/utils/error";

export const toast = sonnerToast;

/** 成功后刷新数据的便利组合（不强制使用） */
export function toastSuccess(message: string, description?: string) {
  if (description) {
    toast.success(message, { description });
  } else {
    toast.success(message);
  }
}

/** 操作失败的便利组合，自动从 catch 的 error 中提取文案 */
export function toastError(e: unknown) {
  const { title, detail } = formatApiError(e);
  if (detail && detail !== title) {
    toast.error(title, { description: detail });
  } else {
    toast.error(title);
  }
}
