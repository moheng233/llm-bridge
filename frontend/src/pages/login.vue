<script setup lang="ts">
// 登录中转页：重定向到后端 OIDC 端点
onMounted(() => {
  window.location.href = "/auth/login";
});
</script>
<template>
  <div class="flex items-center justify-center">
    <div class="text-center">
      <div
        class="mx-auto mb-3 h-8 w-8 animate-spin rounded-full border-2 border-[#22C55E] border-t-transparent"
      />
      <p class="text-sm text-muted-foreground">正在跳转到登录页面...</p>
    </div>
  </div>
</template>
