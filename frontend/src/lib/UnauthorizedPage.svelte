<script lang="ts">
  import { Button } from "$lib/components/ui/button/index.js";
  import { ShieldAlert } from "@lucide/svelte";
  import { push } from "svelte-spa-router";

  function goHome() {
    push("/models");
  }
</script>

<div class="flex flex-1 items-center justify-center">
  <div class="flex flex-col items-center gap-4 text-center">
    <div class="flex h-16 w-16 items-center justify-center rounded-full bg-destructive/10">
      <ShieldAlert class="h-8 w-8 text-destructive" />
    </div>
    <div class="flex flex-col gap-1">
      <h2 class="text-xl font-bold font-mono text-foreground">无访问权限</h2>
      <p class="text-sm text-muted-foreground max-w-sm">
        该页面需要管理员权限。如果你认为这是误判，请联系管理员调整你的账户角色。
      </p>
    </div>
    <Button variant="outline" class="cursor-pointer" onclick={goHome}>
      返回模型目录
    </Button>
  </div>
</div>
