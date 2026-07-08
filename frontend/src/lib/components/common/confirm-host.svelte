<script lang="ts">
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
  } from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { getConfirmState } from "$lib/hooks/useConfirm.svelte";

  const state = getConfirmState();
</script>

<Dialog open={state.open} onOpenChange={(v) => !v && state.resolve(false)}>
  <DialogContent class="sm:max-w-sm">
    <DialogHeader>
      <DialogTitle class="font-mono text-sm">
        {state.current?.title ?? ""}
      </DialogTitle>
      {#if state.current?.description}
        <DialogDescription class="text-sm text-muted-foreground">
          {@html state.current.description}
        </DialogDescription>
      {/if}
    </DialogHeader>
    <div class="flex gap-2 pt-2">
      <Button
        variant="outline"
        class="flex-1 cursor-pointer"
        onclick={() => state.resolve(false)}
      >
        {state.current?.cancelText ?? "取消"}
      </Button>
      <Button
        variant={state.current?.destructive ? "destructive" : "default"}
        class="flex-1 cursor-pointer"
        onclick={() => state.resolve(true)}
      >
        {state.current?.confirmText ?? "确认"}
      </Button>
    </div>
  </DialogContent>
</Dialog>
