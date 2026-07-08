// API client — wraps auto-generated client with auth token from session.
import { createApiClient } from "$bindings/client";
import type { ApiClient } from "$bindings/client";

let _client: ApiClient | null = null;

export function getApi(): ApiClient {
  if (!_client) {
    _client = createApiClient({
      baseUrl: "",
      credentials: "include",
      getToken: async () => null, // Session cookie handles auth
      onError: (err) => {
        // 401 由各页面自行处理（auth store 在 fetchMe 时也会感知）。
        // 这里仅做兜底：避免重复刷新到 /auth/login 造成死循环 —
        // 当 path 已经在 /auth/* 上时不再跳转。
        if (err.status === 401) {
          const path = window.location.pathname + window.location.hash;
          if (!path.startsWith("/auth/")) {
            window.location.href = "/auth/login";
          }
        }
      },
    });
  }
  return _client;
}

// Utility: format large numbers
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return n.toString();
}

// Utility: format price per 1M tokens
export function formatPrice(price: number | null | undefined): string {
  if (price == null) return "—";
  if (price === 0) return "free";
  return `$${price.toFixed(2)}`;
}

// Utility: format timestamp
export function formatTime(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
}

// Utility: format quota period — 已迁移至 $lib/constants.ts 的 quotaPeriodLabel。
// 保留 re-export 以兼容现有 import 路径（逐步迁移后可删除）。
export { quotaPeriodLabel as formatQuotaPeriod } from "$lib/constants";
