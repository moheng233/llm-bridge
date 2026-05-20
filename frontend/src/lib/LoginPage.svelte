<script lang="ts">
  import { auth } from "$lib/stores/auth.svelte";
  import { Loader2 } from "@lucide/svelte";

  // Auto-initiate OIDC login
  $effect(() => {
    auth.login();
  });
</script>

<div class="flex h-screen items-center justify-center bg-background">
  <div class="flex flex-col items-center gap-6">
    <div class="flex items-center gap-3">
      <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[#22C55E] text-black text-sm font-bold font-mono">
        LB
      </div>
      <h1 class="text-2xl font-bold font-mono text-foreground">LLM Bridge</h1>
    </div>
    <p class="text-muted-foreground">正在跳转到身份认证...</p>
    <Loader2 class="h-6 w-6 animate-spin text-[#22C55E]" />
  </div>
</div>
