<script setup lang="ts">
import { Toaster } from "vue-sonner";

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

import { useAuthStore } from "~/stores/auth";
import { useThemeStore } from "~/stores/theme";

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
        'flex shrink-0 flex-col border-r border-border bg-sidebar transition-all duration-200',
        sidebarCollapsed ? 'w-[3.25rem]' : 'w-56',
      ]"
    >
      <!-- Header -->
      <div class="flex items-center gap-2.5 border-b border-border/50 px-3 py-3">
        <img
          src="/favicon.svg"
          alt="LLM Bridge"
          :class="['h-8 w-8 shrink-0', sidebarCollapsed ? 'mx-auto' : '']"
        />
        <span
          v-if="!sidebarCollapsed"
          class="overflow-hidden font-mono text-sm font-semibold whitespace-nowrap"
          >LLM Bridge</span
        >
      </div>

      <!-- Nav -->
      <nav class="flex-1 overflow-y-auto py-2">
        <div class="mb-1 px-2">
          <span v-if="!sidebarCollapsed" class="px-2 text-xs font-medium text-muted-foreground"
            >菜单</span
          >
        </div>
        <button
          v-for="item in memberNavItems"
          :key="item.path"
          @click="navigate(item.path)"
          :title="sidebarCollapsed ? item.label : ''"
          :class="[
            'mb-0.5 flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm transition-colors',
            currentPath === item.path
              ? 'bg-accent font-medium text-accent-foreground'
              : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
            sidebarCollapsed ? 'justify-center' : '',
          ]"
        >
          <component :is="item.icon" class="h-4 w-4 shrink-0" />
          <span v-if="!sidebarCollapsed" class="whitespace-nowrap">{{ item.label }}</span>
        </button>

        <template v-if="isAdmin">
          <div class="mt-4 mb-1 px-2">
            <span v-if="!sidebarCollapsed" class="px-2 text-xs font-medium text-muted-foreground"
              >管理</span
            >
          </div>
          <button
            v-for="item in adminNavItems"
            :key="item.path"
            @click="navigate(item.path)"
            :title="sidebarCollapsed ? item.label : ''"
            :class="[
              'mb-0.5 flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm transition-colors',
              currentPath === item.path
                ? 'bg-accent font-medium text-accent-foreground'
                : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
              sidebarCollapsed ? 'justify-center' : '',
            ]"
          >
            <component :is="item.icon" class="h-4 w-4 shrink-0" />
            <span v-if="!sidebarCollapsed" class="whitespace-nowrap">{{ item.label }}</span>
          </button>
        </template>
      </nav>

      <!-- Footer -->
      <div class="flex flex-col gap-1 border-t border-border/50 p-2">
        <div v-if="user && !sidebarCollapsed" class="px-2">
          <span class="font-mono text-xs text-muted-foreground">{{ user.name }}</span>
        </div>
        <button
          @click="themeStore.toggle()"
          :title="themeStore.mode === 'dark' ? '切换到白天模式' : '切换到黑夜模式'"
          :class="[
            'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground',
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
            'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground',
            sidebarCollapsed ? 'justify-center' : '',
          ]"
        >
          <LogOut class="h-4 w-4" />
          <span v-if="!sidebarCollapsed">退出登录</span>
        </button>
      </div>
    </aside>

    <!-- Main area -->
    <div class="flex min-w-0 flex-1 flex-col overflow-hidden">
      <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
        <button
          @click="toggleSidebar"
          class="rounded-md p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          aria-label="Toggle sidebar"
        >
          <PanelLeftOpen v-if="sidebarCollapsed" class="h-4 w-4" />
          <PanelLeftClose v-else class="h-4 w-4" />
        </button>
        <span class="font-mono text-sm font-medium text-muted-foreground">{{ currentLabel }}</span>
      </header>
      <main class="flex min-h-0 flex-1 flex-col overflow-hidden p-6">
        <!-- Admin route guard -->
        <UnauthorizedPage v-if="accessDenied" />
        <router-view v-else v-slot="{ Component, route }">
          <Transition name="page" mode="out-in">
            <div class="mx-auto w-full max-w-5xl" :key="route.path">
              <component :is="Component" />
            </div>
          </Transition>
        </router-view>
      </main>
    </div>
  </div>

  <Toaster />
  <ConfirmDialog />
</template>
