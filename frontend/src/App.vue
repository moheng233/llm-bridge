<script setup lang="ts">
import { Toaster } from "vue-sonner";

import { LogOut, Sun, Moon, PanelLeftClose, PanelLeftOpen, Search, Menu } from "@lucide/vue";

import ConfirmDialog from "~/components/common/ConfirmDialog.vue";
import { NAV_ITEMS } from "~/lib/navigation";
import { useAuthStore } from "~/stores/auth";
import { useThemeStore } from "~/stores/theme";
const route = useRoute();
const auth = useAuthStore();
const theme = useThemeStore();
const confirm = useConfirm();
const collapsed = ref(localStorage.getItem("llm-bridge:sidebar-collapsed") === "true");
const mobileOpen = ref(false);
const searchOpen = ref(false);
const items = computed(() => NAV_ITEMS.filter((item) => item.group === "use" || auth.isAdmin));
const activePath = computed(() =>
  route.path === "/admin/setup"
    ? route.query.modelId
      ? "/admin/models"
      : "/providers"
    : route.path,
);
const current = computed(() =>
  items.value.find(
    (item) => activePath.value === item.path || activePath.value.startsWith(`${item.path}/`),
  ),
);
function toggleSidebar() {
  collapsed.value = !collapsed.value;
  localStorage.setItem("llm-bridge:sidebar-collapsed", String(collapsed.value));
}
function onKey(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k" && auth.isAuthenticated) {
    event.preventDefault();
    searchOpen.value = !searchOpen.value;
  }
}
async function logout() {
  if (
    await confirm({
      title: "退出登录？",
      description: "当前未保存的输入将丢失。",
      confirmText: "退出",
    })
  )
    await auth.logout();
}
watch(
  () => route.fullPath,
  () => {
    mobileOpen.value = false;
  },
);
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>
<template>
  <div v-if="auth.loading" class="flex h-svh items-center justify-center" role="status">
    正在加载…
  </div>
  <div v-else-if="!auth.isAuthenticated" class="flex min-h-svh items-center justify-center p-3">
    <RouterView />
  </div>
  <div v-else class="flex h-svh overflow-hidden">
    <aside
      :class="['hidden shrink-0 flex-col border-r bg-sidebar md:flex', collapsed ? 'w-14' : 'w-56']"
    >
      <div class="flex h-14 shrink-0 items-center gap-2 border-b px-3">
        <img src="/favicon.svg" alt="" class="size-8" /><span
          v-if="!collapsed"
          class="font-semibold"
          >LLM Bridge</span
        >
      </div>
      <nav aria-label="主导航" class="flex-1 space-y-1 overflow-y-auto p-2">
        <template v-for="group in ['use', 'admin'] as const" :key="group">
          <p
            v-if="!collapsed && (group === 'use' || auth.isAdmin)"
            class="px-2 pt-4 pb-2 text-xs text-muted-foreground"
          >
            {{ group === "use" ? "使用" : "管理" }}
          </p>
          <RouterLink
            v-for="item in items.filter((i) => i.group === group)"
            :key="item.key"
            :to="item.path"
            :aria-label="item.label"
            :title="item.label"
            :aria-current="current?.key === item.key ? 'page' : undefined"
            :class="[
              'flex min-h-9 items-center gap-2 rounded px-2 py-2 text-sm',
              current?.key === item.key
                ? 'bg-accent font-semibold'
                : 'text-muted-foreground hover:bg-accent',
              collapsed ? 'justify-center' : '',
            ]"
            ><component :is="item.icon" class="size-4 shrink-0" /><span v-if="!collapsed">{{
              item.label
            }}</span></RouterLink
          >
        </template>
      </nav>
      <div class="space-y-2 border-t p-2">
        <p v-if="!collapsed" class="px-2 text-sm break-words">
          {{ auth.user?.name
          }}<span class="block text-xs text-muted-foreground">{{
            auth.isAdmin ? "管理员" : "成员"
          }}</span>
        </p>
        <Button
          variant="ghost"
          :class="collapsed ? 'w-full px-0' : 'w-full justify-start'"
          aria-label="切换明暗主题"
          title="切换明暗主题"
          @click="theme.toggle()"
          ><Sun v-if="theme.mode === 'dark'" /><Moon v-else /><span v-if="!collapsed">{{
            theme.mode === "dark" ? "亮色主题" : "暗色主题"
          }}</span></Button
        ><Button
          variant="ghost"
          :class="collapsed ? 'w-full px-0' : 'w-full justify-start'"
          aria-label="退出登录"
          title="退出登录"
          @click="logout"
          ><LogOut /><span v-if="!collapsed">退出</span></Button
        >
      </div>
    </aside>
    <div class="flex min-w-0 flex-1 flex-col">
      <header class="flex h-14 shrink-0 items-center gap-3 border-b px-3 md:px-4 lg:px-6">
        <Button
          variant="ghost"
          size="icon"
          class="md:hidden"
          aria-label="打开导航"
          title="打开导航"
          @click="mobileOpen = true"
          ><Menu /></Button
        ><Button
          variant="ghost"
          size="icon"
          class="hidden md:inline-flex"
          aria-label="折叠或展开导航"
          title="折叠或展开导航"
          @click="toggleSidebar"
          ><PanelLeftOpen v-if="collapsed" /><PanelLeftClose v-else /></Button
        ><span class="min-w-0 truncate text-sm"
          >{{ current?.label || (route.path === "/auth/cli-verify" ? "客户端授权" : "LLM Bridge")
          }}<span v-if="route.path === '/admin/setup'"> / 接入模型</span></span
        ><Button variant="outline" class="ml-auto" @click="searchOpen = true"
          ><Search /><span>搜索</span><kbd class="hidden text-xs sm:inline">Ctrl K</kbd></Button
        >
      </header>
      <main id="main-content" class="min-h-0 flex-1 overflow-y-auto p-3 md:p-4 lg:p-6">
        <div class="mx-auto w-full max-w-[1280px] min-w-0">
          <UnauthorizedPage v-if="route.meta.requiresAdmin && !auth.isAdmin" /><RouterView v-else />
        </div>
      </main>
    </div>
    <Sheet v-model:open="mobileOpen"
      ><SheetContent side="left" class="w-72 max-w-[90vw] bg-sidebar p-4"
        ><SheetHeader
          ><SheetTitle>LLM Bridge</SheetTitle
          ><SheetDescription>使用与管理导航</SheetDescription></SheetHeader
        >
        <nav class="flex-1 space-y-1 overflow-y-auto" aria-label="移动导航">
          <RouterLink
            v-for="item in items"
            :key="item.key"
            :to="item.path"
            class="flex min-h-11 items-center gap-3 rounded px-3 text-sm"
            :class="current?.key === item.key ? 'bg-accent font-semibold' : ''"
            ><component :is="item.icon" class="size-4" />{{ item.label }}</RouterLink
          >
        </nav>
        <p class="text-sm">{{ auth.user?.name }} · {{ auth.isAdmin ? "管理员" : "成员" }}</p>
        <Button variant="outline" @click="theme.toggle()">切换明暗主题</Button
        ><Button variant="ghost" @click="logout">退出登录</Button></SheetContent
      ></Sheet
    >
  </div>
  <GlobalSearch v-if="auth.isAuthenticated" v-model:open="searchOpen" /><ConfirmDialog /><Toaster />
</template>
