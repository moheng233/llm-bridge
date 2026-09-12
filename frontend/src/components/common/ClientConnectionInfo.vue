<script setup lang="ts">
const props = defineProps<{ modelName?: string }>();
const baseUrl = `${window.location.origin}/v1`;
const error = ref("");
const copied = ref("");
const quote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;
const example = computed(() =>
  props.modelName
    ? `curl ${quote(`${baseUrl}/chat/completions`)} \\\n  -H "Authorization: Bearer $LLM_BRIDGE_TOKEN" \\\n  -H 'Content-Type: application/json' \\\n  -d ${quote(JSON.stringify({ model: props.modelName, messages: [{ role: "user", content: "你好" }] }))}`
    : "",
);
async function copy(value: string, label: string) {
  error.value = "";
  try {
    if (!navigator.clipboard) throw new Error("clipboard_unavailable");
    await navigator.clipboard.writeText(value);
    copied.value = label;
  } catch {
    error.value = "复制失败，请选择下方文本手动复制。";
  }
}
</script>
<template>
  <section class="min-w-0 space-y-4 rounded-lg border bg-card p-4">
    <h2 class="text-lg font-semibold">接入方式</h2>
    <p class="text-sm text-muted-foreground">
      使用个人访问令牌调用 LLM Bridge；不是提供者的上游 API Key。
    </p>
    <div class="space-y-2">
      <Label>OpenAI 兼容 Base URL</Label>
      <div class="flex flex-wrap gap-2">
        <code class="min-w-0 break-all select-text">{{ baseUrl }}</code
        ><Button size="sm" variant="outline" @click="copy(baseUrl, 'Base URL')"
          >复制 Base URL</Button
        >
      </div>
      <p class="text-xs text-muted-foreground">当前访问入口；若客户端使用另一域名请替换。</p>
    </div>
    <div v-if="modelName" class="space-y-2">
      <Label>客户端模型 ID</Label>
      <div class="flex flex-wrap gap-2">
        <code class="break-all select-text">{{ modelName }}</code
        ><Button size="sm" variant="outline" @click="copy(modelName, '模型 ID')"
          >复制模型 ID</Button
        >
      </div>
      <p class="text-xs text-muted-foreground">
        先在客户端环境中设置 LLM_BRIDGE_TOKEN，再执行示例。
      </p>
      <pre
        class="max-w-full overflow-x-auto rounded border bg-muted p-3 text-xs leading-5 select-text"
        >{{ example }}</pre>
      <Button size="sm" variant="outline" @click="copy(example, '示例')">复制示例</Button>
    </div>
    <p v-if="error" role="alert" class="text-sm text-destructive">{{ error }}</p>
    <p v-else-if="copied" role="status" class="text-sm">已复制{{ copied }}</p>
    <RouterLink
      class="inline-flex min-h-9 items-center text-primary underline"
      :to="modelName ? `/tokens?model=${encodeURIComponent(modelName)}` : '/tokens'"
      >创建访问令牌</RouterLink
    >
  </section>
</template>
