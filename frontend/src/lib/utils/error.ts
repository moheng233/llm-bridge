// 统一错误格式化 — 见 PLAN.md §9 决策②。
// 返回中文友好主文案，原始技术细节留作 detail（用于 tooltip / 展开）。
//
// 后端错误响应格式（client.ts 已解析）：
//   - ApiError.body 可能为: { error: "..." } | { message: "..." } | JSON 对象 | 纯文本字符串 | undefined
//   - ApiError.status: HTTP 状态码
//   - ApiError.message: client.ts 已 fallback 提取过的字符串（可能是 JSON.stringify body）
//
// 调用方：
//   import { formatApiError } from "$lib/utils/error";
//   const { title, detail } = formatApiError(e);
//   toast.error(title, { description: detail });

export interface FormattedError {
  /** 中文友好主文案，给 toast / alert 顶部展示 */
  title: string;
  /** 原始技术细节，给 tooltip / 展开（可能为空） */
  detail?: string;
}

/** 状态码 → 中文基础文案 */
function statusToTitle(status: number, fallback: string): string {
  switch (status) {
    case 400:
      return "请求参数有误";
    case 401:
      return "未登录或登录已过期";
    case 403:
      return "没有权限执行该操作";
    case 404:
      return "资源不存在或已被删除";
    case 409:
      return "操作冲突，可能资源已被他人修改";
    case 422:
      return "请求数据校验失败";
    case 429:
      return "请求过于频繁，请稍后再试";
    case 500:
      return "服务器内部错误";
    case 502:
    case 503:
    case 504:
      return "上游服务暂时不可用";
    default:
      return fallback || `请求失败（${status}）`;
  }
}

/**
 * 把任意 catch 到的错误格式化为 { title, detail }。
 * 兼容 ApiError（含 status/body）与原生 Error / 字符串 / 未知类型。
 */
export function formatApiError(e: unknown): FormattedError {
  // 1. ApiError 形态（来自生成的 client.ts）
  if (e && typeof e === "object" && "status" in e && typeof (e as any).status === "number") {
    const err = e as { status: number; message: string; body?: unknown; name?: string };
    const title = statusToTitle(err.status, err.message);
    // detail：优先用原始 message，message 与 title 重叠时省略
    const detail = err.message && err.message !== title ? err.message : undefined;
    return { title, detail };
  }

  // 2. 普通 Error
  if (e instanceof Error) {
    return { title: "操作失败", detail: e.message };
  }

  // 3. 字符串
  if (typeof e === "string") {
    return { title: "操作失败", detail: e };
  }

  // 4. 其他
  return { title: "操作失败", detail: "未知错误" };
}

/**
 * 提取用于 toast 的简洁文案。
 * 便利方法：直接 toast.error(toastErrorMessage(e))。
 */
export function toastErrorMessage(e: unknown): string {
  return formatApiError(e).title;
}
