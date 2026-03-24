<script lang="ts">
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import ModelsPage from "./lib/ModelsPage.svelte";
  import ProvidersPage from "./lib/ProvidersPage.svelte";
  import BindingsPage from "./lib/BindingsPage.svelte";

  type Page = "models" | "providers" | "bindings";
  const validPages = new Set<Page>(["models", "providers", "bindings"]);

  function getPageFromHash(): Page {
    const hash = window.location.hash.slice(1);
    return validPages.has(hash as Page) ? (hash as Page) : "models";
  }

  let page = $state<Page>(getPageFromHash());

  function setPage(p: Page) {
    page = p;
    window.location.hash = p;
  }

  $effect(() => {
    const onHashChange = () => { page = getPageFromHash(); };
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  });

  const navItems = [
    { id: "models" as const, label: "模型目录", icon: "📦" },
    { id: "providers" as const, label: "提供者管理", icon: "🔧" },
    { id: "bindings" as const, label: "模型绑定", icon: "🔗" },
  ];
</script>

<Sidebar.Provider class="h-svh overflow-hidden">
  <Sidebar.Root>
    <Sidebar.Header>
      <div class="flex items-center gap-2.5 px-2 py-1.5">
        <div
          class="flex h-8 w-8 items-center justify-center rounded-lg bg-primary text-primary-foreground text-sm font-bold"
        >
          LB
        </div>
        <span class="text-sm font-semibold">LLM Bridge</span>
      </div>
    </Sidebar.Header>
    <Sidebar.Content>
      <Sidebar.Group>
        <Sidebar.GroupLabel>管理</Sidebar.GroupLabel>
        <Sidebar.Menu>
          {#each navItems as item}
            <Sidebar.MenuItem>
              <Sidebar.MenuButton isActive={page === item.id} onclick={() => setPage(item.id)}>
                <span>{item.icon}</span>
                <span>{item.label}</span>
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          {/each}
        </Sidebar.Menu>
      </Sidebar.Group>
    </Sidebar.Content>
    <Sidebar.Footer>
      <div class="px-2 py-1.5 text-xs text-muted-foreground">llm-bridge admin</div>
    </Sidebar.Footer>
  </Sidebar.Root>
  <Sidebar.Inset class="overflow-hidden min-h-0">
    <header class="flex h-12 shrink-0 items-center gap-2 border-b px-4">
      <Sidebar.Trigger class="-ml-1" />
      <span class="text-sm font-medium text-muted-foreground">
        {navItems.find((n) => n.id === page)?.label}
      </span>
    </header>
    <main class="flex-1 min-h-0 overflow-hidden p-6 flex flex-col">
      {#if page === "models"}
        <ModelsPage />
      {:else if page === "providers"}
        <ProvidersPage />
      {:else if page === "bindings"}
        <BindingsPage />
      {/if}
    </main>
  </Sidebar.Inset>
</Sidebar.Provider>
