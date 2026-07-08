<script setup lang="ts">
import { Toaster } from "vue-sonner";
import { useAuthStore } from "~/stores/auth";
import { useThemeStore } from "~/stores/theme";
import {
  Cpu,
  Key,
  Globe,
  Users,
  LogOut,
  Sun,
  Moon,
  Boxes,
  PanelLeftClose,
  PanelLeftOpen,
} from "@lucide/vue";

const router = useRouter();
const route = useRoute();
const authStore = useAuthStore();
const themeStore = useThemeStore();
const { user, loading, isAdmin, isAuthenticated } = storeToRefs(authStore);

// Sidebar collapse state
const sidebarCollapsed = ref(localStorage.getItem("llm-bridge:sidebar-collapsed") === "true");

function toggleSidebar() {
  sidebarCollapsed.value = !sidebarCollapsed.value;
  localStorage.setItem("llm-bridge:sidebar-collapsed", String(sidebarCollapsed.value));
}

// Resolve current page path for active nav highlighting
const currentPath = computed(() => {
  const p = route.path;
  if (p === "/" || p === "/models") return "/models";
  return p;
});

const isAdminRoute = computed(() => {
  return route.meta.requiresAdmin === true;
});

const accessDenied = computed(() => isAdminRoute.value && !isAdmin.value);

const memberNavItems = [
  { path: "/models", label: "模型目录", icon: Cpu },
  { path: "/tokens", label: "API Token", icon: Key },
];

const adminNavItems = [
  { path: "/admin/models", label: "模型管理", icon: Boxes },
  { path: "/providers", label: "提供者管理", icon: Globe },
  { path: "/users", label: "用户管理", icon: Users },
];

const currentLabel = computed(() => {
  return (
    [...memberNavItems, ...adminNavItems].find((n) => n.path === currentPath.value)?.label || ""
  );
});

function navigate(path: string) {
  router.push(path);
}

function handleLogout() {
  authStore.logout();
}
</script>

<template>
  <!-- Loading state -->
  <div v-if="loading" class="flex h-screen items-center justify-center bg-background">
    <div class="h-8 w-8 animate-spin rounded-full border-2 border-[#22C55E] border-t-transparent" />
  </div>

  <!-- Not authenticated -->
  <div v-else-if="!isAuthenticated" class="flex h-screen items-center justify-center bg-background">
    <router-view />
  </div>

  <!-- Authenticated layout -->
  <div v-else class="flex h-svh overflow-hidden">
    <!-- Sidebar -->
    <aside
      :class="[
        'flex flex-col border-r border-border bg-sidebar transition-all duration-200 shrink-0',
        sidebarCollapsed ? 'w-[3.25rem]' : 'w-56',
      ]"
    >
      <!-- Header -->
      <div class="flex items-center gap-2.5 px-3 py-3 border-b border-border/50">
        <div
          :class="[
            'flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-[#22C55E] text-black text-sm font-bold font-mono',
            sidebarCollapsed ? 'mx-auto' : '',
          ]"
        >
          LB
        </div>
        <span
          v-if="!sidebarCollapsed"
          class="text-sm font-semibold font-mono whitespace-nowrap overflow-hidden"
          >LLM Bridge</span
        >
      </div>

      <!-- Nav -->
      <nav class="flex-1 overflow-y-auto py-2">
        <div class="px-2 mb-1">
          <span v-if="!sidebarCollapsed" class="text-xs font-medium text-muted-foreground px-2"
            >菜单</span
          >
        </div>
        <button
          v-for="item in memberNavItems"
          :key="item.path"
          @click="navigate(item.path)"
          :title="sidebarCollapsed ? item.label : ''"
          :class="[
            'flex items-center gap-2 w-full px-3 py-2 text-sm rounded-md transition-colors mb-0.5',
            currentPath === item.path
              ? 'bg-accent text-accent-foreground font-medium'
              : 'text-muted-foreground hover:text-foreground hover:bg-accent/50',
            sidebarCollapsed ? 'justify-center' : '',
          ]"
        >
          <component :is="item.icon" class="h-4 w-4 shrink-0" />
          <span v-if="!sidebarCollapsed" class="whitespace-nowrap">{{ item.label }}</span>
        </button>

        <template v-if="isAdmin">
          <div class="px-2 mt-4 mb-1">
            <span v-if="!sidebarCollapsed" class="text-xs font-medium text-muted-foreground px-2"
              >管理</span
            >
          </div>
          <button
            v-for="item in adminNavItems"
            :key="item.path"
            @click="navigate(item.path)"
            :title="sidebarCollapsed ? item.label : ''"
            :class="[
              'flex items-center gap-2 w-full px-3 py-2 text-sm rounded-md transition-colors mb-0.5',
              currentPath === item.path
                ? 'bg-accent text-accent-foreground font-medium'
                : 'text-muted-foreground hover:text-foreground hover:bg-accent/50',
              sidebarCollapsed ? 'justify-center' : '',
            ]"
          >
            <component :is="item.icon" class="h-4 w-4 shrink-0" />
            <span v-if="!sidebarCollapsed" class="whitespace-nowrap">{{ item.label }}</span>
          </button>
        </template>
      </nav>

      <!-- Footer -->
      <div class="border-t border-border/50 p-2 flex flex-col gap-1">
        <div v-if="user && !sidebarCollapsed" class="px-2">
          <span class="text-xs text-muted-foreground font-mono">{{ user.name }}</span>
        </div>
        <button
          @click="themeStore.toggle()"
          :title="themeStore.mode === 'dark' ? '切换到白天模式' : '切换到黑夜模式'"
          :class="[
            'flex items-center gap-2 w-full px-2 py-1.5 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-colors',
            sidebarCollapsed ? 'justify-center' : '',
          ]"
        >
          <Sun v-if="themeStore.mode === 'dark'" class="h-4 w-4" />
          <Moon v-else class="h-4 w-4" />
          <span v-if="!sidebarCollapsed">{{
            themeStore.mode === "dark" ? "白天模式" : "黑夜模式"
          }}</span>
        </button>
        <button
          @click="handleLogout"
          :class="[
            'flex items-center gap-2 w-full px-2 py-1.5 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-colors',
            sidebarCollapsed ? 'justify-center' : '',
          ]"
        >
          <LogOut class="h-4 w-4" />
          <span v-if="!sidebarCollapsed">退出登录</span>
        </button>
      </div>
    </aside>

    <!-- Main area -->
    <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
      <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
        <button
          @click="toggleSidebar"
          class="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
          aria-label="Toggle sidebar"
        >
          <PanelLeftOpen v-if="sidebarCollapsed" class="h-4 w-4" />
          <PanelLeftClose v-else class="h-4 w-4" />
        </button>
        <span class="text-sm font-medium text-muted-foreground font-mono">{{ currentLabel }}</span>
      </header>
      <main class="flex-1 min-h-0 overflow-hidden p-6 flex flex-col">
        <!-- Admin route guard -->
        <UnauthorizedPage v-if="accessDenied" />
        <router-view v-else />
      </main>
    </div>
  </div>

  <Toaster />
  <ConfirmDialog />
</template>
