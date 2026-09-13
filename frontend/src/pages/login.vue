<script setup lang="ts">
// 登录中转页：重定向到后端 OIDC 端点
onMounted(() => {
  window.location.href = "/auth/login";
});
</script>
<template>
  <PageShell body-class="flex flex-col">
    <div
      class="m-auto w-full max-w-md shrink-0 space-y-4 rounded-lg border bg-card p-6 text-center"
      role="status"
    >
      <div
        class="mx-auto mb-3 h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent"
      />
      <p class="text-sm text-muted-foreground">正在跳转到登录页面...</p>
      <p class="text-sm text-muted-foreground">使用部署配置的 OIDC 身份服务登录。</p>
      <a href="/auth/login" class="inline-flex min-h-11 items-center text-primary underline"
        >未自动跳转？重新打开登录入口</a
      >
    </div>
  </PageShell>
</template>
