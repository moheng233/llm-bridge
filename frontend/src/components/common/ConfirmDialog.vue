<script setup lang="ts">
const state = getConfirmState();
</script>

<template>
  <Dialog
    v-if="state"
    :open="state?.open ?? false"
    @update:open="
      (v: boolean) => {
        if (!v && state) state.resolve(false);
      }
    "
  >
    <DialogContent class="sm:max-w-sm">
      <DialogHeader>
        <DialogTitle class="text-lg">
          {{ state?.title ?? "" }}
        </DialogTitle>
        <DialogDescription v-if="state?.description" class="text-sm text-muted-foreground">
          {{ state.description }}
        </DialogDescription>
      </DialogHeader>
      <div class="flex gap-2 pt-2">
        <Button variant="outline" class="flex-1 cursor-pointer" @click="state?.resolve(false)">
          {{ state?.cancelText ?? "取消" }}
        </Button>
        <Button
          :variant="state?.destructive ? 'destructive' : 'default'"
          class="flex-1 cursor-pointer"
          @click="state?.resolve(true)"
        >
          {{ state?.confirmText ?? "确认" }}
        </Button>
      </div>
    </DialogContent>
  </Dialog>
</template>
