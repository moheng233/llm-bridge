<script setup lang="ts">
import { ShieldAlert } from "@lucide/vue";

const router = useRouter();
function goHome() {
  router.push("/models");
}
</script>

<template>
  <div class="flex flex-1 items-center justify-center">
    <div class="flex flex-col items-center gap-4 text-center">
      <div class="flex h-16 w-16 items-center justify-center rounded-full bg-destructive/10">
        <ShieldAlert class="h-8 w-8 text-destructive" />
      </div>
      <div class="flex flex-col gap-1">
        <h2 class="font-mono text-xl font-bold text-foreground">无访问权限</h2>
        <p class="max-w-sm text-sm text-muted-foreground">
          该页面需要管理员权限。如果你认为这是误判，请联系管理员调整你的账户角色。
        </p>
      </div>
      <Button variant="outline" class="cursor-pointer" @click="goHome"> 返回模型目录 </Button>
    </div>
  </div>
</template>
