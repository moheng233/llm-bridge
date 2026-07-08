<script lang="ts">
  import Router from "svelte-spa-router";
  import { push, router } from "svelte-spa-router";
  import { Toaster } from "svelte-sonner";
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { auth } from "$lib/stores/auth.svelte";
  import { theme } from "$lib/stores/theme.svelte";
  import ModelsPage from "./lib/ModelsPage.svelte";
  import TokensPage from "./lib/TokensPage.svelte";
  import ProvidersPage from "./lib/ProvidersPage.svelte";
  import UsersPage from "./lib/UsersPage.svelte";
  import LoginPage from "./lib/LoginPage.svelte";
  import AdminModelsPage from "./lib/AdminModelsPage.svelte";
  import { ConfirmHost } from "$lib/components/common/index.js";
  import { Cpu, Key, Globe, Users, LogOut, Sun, Moon, Boxes } from "@lucide/svelte";

  type PagePath = "/models" | "/tokens" | "/providers" | "/users" | "/admin/models";

  const routes = {
    "/": ModelsPage,
    "/models": ModelsPage,
    "/tokens": TokensPage,
    "/providers": ProvidersPage,
    "/users": UsersPage,
    "/admin/models": AdminModelsPage,
    "/login": LoginPage,
  };

  const resolveCurrentPage = (path: string): PagePath | "/login" => {
    if (path === "/login") return "/login";
    if (path === "/" || path === "/models") return "/models";
    if (path === "/tokens") return "/tokens";
    if (path === "/providers") return "/providers";
    if (path === "/users") return "/users";
    if (path === "/admin/models") return "/admin/models";
    return "/models";
  };

  let currentPath = $derived(resolveCurrentPage(router.location));

  function navigate(path: PagePath | "/login") {
    push(path);
  }

  // Auth loading state
  let showApp = $derived(!auth.loading);
  let isAuthenticated = $derived(auth.isAuthenticated);
  let isAdmin = $derived(auth.isAdmin);

  const memberNavItems = [
    { path: "/models" as const, label: "模型目录", icon: Cpu },
    { path: "/tokens" as const, label: "API Token", icon: Key },
  ];

  const adminNavItems = [
    { path: "/admin/models" as const, label: "模型管理", icon: Boxes },
    { path: "/providers" as const, label: "提供者管理", icon: Globe },
    { path: "/users" as const, label: "用户管理", icon: Users },
  ];

  function getCurrentLabel(path: string): string {
    return [...memberNavItems, ...adminNavItems].find((n) => n.path === path)?.label || "";
  }
</script>

{#if !showApp}
  <div class="flex h-screen items-center justify-center bg-background">
    <Spinner class="h-8 w-8 text-[#22C55E]" />
  </div>
{:else if !isAuthenticated}
  <LoginPage />
{:else}
  <Sidebar.Provider class="h-svh overflow-hidden">
    <Sidebar.Root collapsible="icon">
      <Sidebar.Header>
        <div
          class="flex items-center gap-2.5 px-2 py-1.5 group-data-[collapsible=icon]:gap-0 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
        >
          <div
            class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-[#22C55E] text-black text-sm font-bold font-mono"
          >
            LB
          </div>
          <span
            class="text-sm font-semibold font-mono overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
            >LLM Bridge</span
          >
        </div>
      </Sidebar.Header>
      <Sidebar.Content>
        <Sidebar.Group>
          <Sidebar.GroupLabel>菜单</Sidebar.GroupLabel>
          <Sidebar.Menu>
            {#each memberNavItems as item}
              <Sidebar.MenuItem>
                <Sidebar.MenuButton
                  isActive={currentPath === item.path}
                  onclick={() => navigate(item.path)}
                  tooltipContent={item.label}
                >
                  <item.icon class="h-4 w-4" />
                  <span
                    class="overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
                    >{item.label}</span
                  >
                </Sidebar.MenuButton>
              </Sidebar.MenuItem>
            {/each}
          </Sidebar.Menu>
        </Sidebar.Group>
        {#if isAdmin}
          <Sidebar.Group>
            <Sidebar.GroupLabel>管理</Sidebar.GroupLabel>
            <Sidebar.Menu>
              {#each adminNavItems as item}
                <Sidebar.MenuItem>
                  <Sidebar.MenuButton
                    isActive={currentPath === item.path}
                    onclick={() => navigate(item.path)}
                    tooltipContent={item.label}
                  >
                    <item.icon class="h-4 w-4" />
                    <span
                      class="overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
                      >{item.label}</span
                    >
                  </Sidebar.MenuButton>
                </Sidebar.MenuItem>
              {/each}
            </Sidebar.Menu>
          </Sidebar.Group>
        {/if}
      </Sidebar.Content>
      <Sidebar.Footer>
        <div class="flex flex-col gap-2 px-2 py-2">
          {#if auth.user}
            <div
              class="text-xs text-muted-foreground overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
            >
              <span class="font-mono text-foreground">{auth.user.name}</span>
            </div>
          {/if}
          <Button
            variant="ghost"
            size="sm"
            class="justify-start gap-2 text-muted-foreground hover:text-foreground cursor-pointer group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-1"
            onclick={() => theme.toggle()}
            aria-label={theme.isDark ? "切换到白天模式" : "切换到黑夜模式"}
            title={theme.isDark ? "切换到白天模式" : "切换到黑夜模式"}
          >
            {#if theme.isDark}
              <Sun class="h-4 w-4" />
            {:else}
              <Moon class="h-4 w-4" />
            {/if}
            <span
              class="overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
              >{theme.isDark ? "白天模式" : "黑夜模式"}</span
            >
          </Button>
          <Button
            variant="ghost"
            size="sm"
            class="justify-start gap-2 text-muted-foreground hover:text-foreground cursor-pointer group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-1"
            onclick={() => auth.logout()}
          >
            <LogOut class="h-4 w-4" />
            <span
              class="overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
              >退出登录</span
            >
          </Button>
        </div>
      </Sidebar.Footer>
    </Sidebar.Root>
    <Sidebar.Inset class="overflow-hidden min-h-0">
      <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
        <Sidebar.Trigger class="-ml-1" />
        <span class="text-sm font-medium text-muted-foreground font-mono">
          {getCurrentLabel(currentPath)}
        </span>
      </header>
      <main class="flex-1 min-h-0 overflow-hidden p-6 flex flex-col">
        <Router {routes} />
      </main>
    </Sidebar.Inset>
  </Sidebar.Provider>
{/if}

<!-- 全局 toast 容器 — 任意状态下可见（含登录态） -->
<Toaster
  theme={theme.isDark ? "dark" : "light"}
  position="top-right"
  richColors
  closeButton
  toastOptions={{
    style: "font-sans",
    class: "font-sans",
  }}
/>

<!-- 全局确认对话框宿主 — 由 useConfirm.svelte 驱动 -->
<ConfirmHost />
