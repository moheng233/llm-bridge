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
        if (err.status === 401) {
          window.location.href = "/auth/login";
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

// Utility: format quota period
export function formatQuotaPeriod(period: string): string {
  switch (period) {
    case "daily": return "每天";
    case "monthly": return "每月";
    case "unlimited": return "不限制";
    default: return period;
  }
}
