<script lang="ts">
  import Router from "svelte-spa-router";
  import { push, router } from "svelte-spa-router";
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";
  import ModelsPage from "./lib/ModelsPage.svelte";
  import ProvidersPage from "./lib/ProvidersPage.svelte";
  import BindingsPage from "./lib/BindingsPage.svelte";

  type PagePath = "/models" | "/providers" | "/bindings";

  const routes = {
    "/": ModelsPage,
    "/models": ModelsPage,
    "/providers": ProvidersPage,
    "/bindings": BindingsPage,
  };

  const resolveCurrentPage = (path: string): PagePath => {
    if (path === "/" || path === "/models") return "/models";
    if (path === "/providers") return "/providers";
    if (path === "/bindings") return "/bindings";
    return "/models";
  };

  let currentPath = $derived(resolveCurrentPage(router.location));

  function navigate(path: PagePath) {
    push(path);
  }

  const navItems = [
    { path: "/models" as const, label: "模型目录", icon: "📦" },
    { path: "/providers" as const, label: "提供者管理", icon: "🔧" },
    { path: "/bindings" as const, label: "模型绑定", icon: "🔗" },
  ];
</script>

<Sidebar.Provider class="h-svh overflow-hidden">
  <Sidebar.Root collapsible="icon">
    <Sidebar.Header>
      <div
        class="flex items-center gap-2.5 px-2 py-1.5 group-data-[collapsible=icon]:gap-0 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
      >
        <div
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground text-sm font-bold"
        >
          LB
        </div>
        <span
          class="text-sm font-semibold overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
          >LLM Bridge</span
        >
      </div>
    </Sidebar.Header>
    <Sidebar.Content>
      <Sidebar.Group>
        <Sidebar.GroupLabel>管理</Sidebar.GroupLabel>
        <Sidebar.Menu>
          {#each navItems as item}
            <Sidebar.MenuItem>
              <Sidebar.MenuButton
                isActive={currentPath === item.path}
                onclick={() => navigate(item.path)}
                tooltipContent={item.label}
              >
                <span>{item.icon}</span>
                <span
                  class="overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
                  >{item.label}</span
                >
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          {/each}
        </Sidebar.Menu>
      </Sidebar.Group>
    </Sidebar.Content>
    <Sidebar.Footer>
      <div
        class="px-2 py-1.5 text-xs text-muted-foreground overflow-hidden whitespace-nowrap transition-[opacity,max-width] duration-200 ease-linear group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:max-w-0 group-data-[state=expanded]:opacity-100 group-data-[state=expanded]:max-w-[200px]"
      >
        llm-bridge admin
      </div>
    </Sidebar.Footer>
  </Sidebar.Root>
  <Sidebar.Inset class="overflow-hidden min-h-0">
    <header class="flex h-12 shrink-0 items-center gap-2 border-b px-4">
      <Sidebar.Trigger class="-ml-1" />
      <span class="text-sm font-medium text-muted-foreground">
        {navItems.find((n) => n.path === currentPath)?.label}
      </span>
    </header>
    <main class="flex-1 min-h-0 overflow-hidden p-6 flex flex-col">
      <Router {routes} />
    </main>
  </Sidebar.Inset>
</Sidebar.Provider>
