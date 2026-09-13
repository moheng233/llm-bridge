<script setup lang="ts">
import { Toaster } from "vue-sonner";

import {
  ChevronRight,
  LogOut,
  Sun,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  Search,
  Menu,
} from "@lucide/vue";

import ConfirmDialog from "~/components/common/ConfirmDialog.vue";
import { providePageHeader } from "~/composables/usePageHeader";
import { NAV_ITEMS } from "~/lib/navigation";
import { useAuthStore } from "~/stores/auth";
import { useThemeStore } from "~/stores/theme";
const route = useRoute();
const pageHeader = providePageHeader();
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
const pageTitle = computed(() => {
  if (route.meta.requiresAdmin && !auth.isAdmin) return "无访问权限";
  if (pageHeader.value?.path === route.path) return pageHeader.value.title;
  if (route.path === "/admin/setup") return "接入模型";
  if (route.path === "/admin/models/new") return "添加模型定义";
  if (route.path.startsWith("/providers/")) return "提供者详情";
  if (route.path.startsWith("/admin/models/")) return "模型详情";
  if (route.path.startsWith("/traces/")) return "请求详情";
  if (route.path === "/auth/cli-verify") return "客户端授权";
  return current.value?.label ?? "LLM Bridge";
});
const parentPage = computed(() =>
  current.value && route.path !== current.value.path ? current.value : null,
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
  <div v-if="auth.loading" class="app-viewport flex items-center justify-center" role="status">
    正在加载…
  </div>
  <div v-else-if="!auth.isAuthenticated" class="app-viewport min-h-0 p-3">
    <RouterView v-slot="{ Component, route: viewRoute }">
      <Transition
        name="route-page"
        mode="out-in"
        @before-leave="(element) => element.setAttribute('inert', '')"
        @leave-cancelled="(element) => element.removeAttribute('inert')"
      >
        <div :key="viewRoute.path" class="route-page h-full min-h-0 min-w-0">
          <component :is="Component" />
        </div>
      </Transition>
    </RouterView>
  </div>
  <div v-else class="app-viewport flex">
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
      <nav
        aria-label="主导航"
        class="min-h-0 flex-1 space-y-1 overflow-y-auto overscroll-contain p-2"
      >
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
    <div class="flex min-h-0 min-w-0 flex-1 flex-col">
      <header
        data-slot="app-header"
        class="flex h-14 shrink-0 items-center gap-2 border-b px-3 sm:gap-3 md:px-4 lg:px-6"
      >
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
          ><PanelLeftOpen v-if="collapsed" /><PanelLeftClose v-else
        /></Button>
        <nav aria-label="面包屑" class="min-w-0 flex-1">
          <ol class="flex min-w-0 items-center gap-2 text-sm">
            <li v-if="parentPage" class="shrink-0">
              <RouterLink
                :to="parentPage.path"
                :aria-label="`返回${parentPage.label}列表`"
                class="text-muted-foreground hover:text-foreground hover:underline"
                >{{ parentPage.label }}</RouterLink
              >
            </li>
            <li v-if="parentPage" aria-hidden="true" class="shrink-0 text-muted-foreground">
              <ChevronRight class="size-4" />
            </li>
            <li aria-current="page" class="min-w-0">
              <h1 class="truncate text-sm leading-5 font-medium" :title="pageTitle">
                {{ pageTitle }}
              </h1>
            </li>
          </ol>
        </nav>
        <div
          id="page-header-actions"
          data-slot="app-header-actions"
          class="flex shrink-0 items-center gap-2 empty:hidden"
        />
        <Button
          variant="outline"
          class="ml-auto shrink-0"
          aria-label="全局搜索"
          title="全局搜索 (Ctrl K)"
          @click="searchOpen = true"
          ><Search /><span class="hidden sm:inline">搜索</span
          ><kbd class="hidden text-xs lg:inline">Ctrl K</kbd></Button
        >
      </header>
      <main id="main-content" class="min-h-0 flex-1 overflow-hidden p-3 md:p-4 lg:p-6">
        <div class="mx-auto h-full min-h-0 w-full max-w-[1280px] min-w-0">
          <RouterView v-slot="{ Component, route: viewRoute }">
            <Transition
              name="route-page"
              mode="out-in"
              @before-leave="(element) => element.setAttribute('inert', '')"
              @leave-cancelled="(element) => element.removeAttribute('inert')"
            >
              <div :key="viewRoute.path" class="route-page h-full min-h-0 min-w-0">
                <PageShell v-if="viewRoute.meta.requiresAdmin && !auth.isAdmin"
                  ><UnauthorizedPage /></PageShell
                ><component :is="Component" v-else />
              </div>
            </Transition>
          </RouterView>
        </div>
      </main>
    </div>
    <Sheet v-model:open="mobileOpen"
      ><SheetContent side="left" class="w-72 max-w-[90vw] bg-sidebar p-4"
        ><SheetHeader
          ><SheetTitle>LLM Bridge</SheetTitle
          ><SheetDescription>使用与管理导航</SheetDescription></SheetHeader
        >
        <nav
          class="min-h-0 flex-1 space-y-1 overflow-y-auto overscroll-contain"
          aria-label="移动导航"
        >
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

<style scoped>
.route-page-enter-active {
  transition:
    opacity 180ms ease-out,
    transform 180ms cubic-bezier(0.2, 0.65, 0.3, 1);
}

.route-page-leave-active {
  pointer-events: none;
  transition: opacity 80ms ease-in;
}

.route-page-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.route-page-leave-to {
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .route-page-enter-active,
  .route-page-leave-active {
    transition: none;
  }

  .route-page-enter-from,
  .route-page-leave-to {
    opacity: 1;
    transform: none;
  }
}
</style>
