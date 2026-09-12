import { type Component } from "vue";

import { LayoutDashboard, Cpu, Key, ScrollText, Globe, Boxes, Users } from "@lucide/vue";

export const NAV_ITEMS: {
  key: string;
  label: string;
  path: string;
  group: "use" | "admin";
  icon: Component;
}[] = [
  { key: "dashboard", label: "概览", path: "/dashboard", group: "use", icon: LayoutDashboard },
  { key: "models", label: "使用模型", path: "/models", group: "use", icon: Cpu },
  { key: "tokens", label: "访问令牌", path: "/tokens", group: "use", icon: Key },
  { key: "traces", label: "请求记录", path: "/traces", group: "use", icon: ScrollText },
  { key: "providers", label: "提供者", path: "/providers", group: "admin", icon: Globe },
  { key: "definitions", label: "模型定义", path: "/admin/models", group: "admin", icon: Boxes },
  { key: "users", label: "用户", path: "/users", group: "admin", icon: Users },
];
