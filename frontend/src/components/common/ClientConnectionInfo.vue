<script setup lang="ts">
import { toast } from "vue-sonner";

import { ArrowUpRight, Copy, KeyRound, Terminal } from "@lucide/vue";

import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "~/components/ui/tooltip";

const props = defineProps<{ modelName?: string }>();
const baseUrl = `${window.location.origin}/v1`;
const quote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;
const example = computed(() =>
  props.modelName
    ? `curl ${quote(`${baseUrl}/chat/completions`)} \\\n  -H "Authorization: Bearer $LLM_BRIDGE_TOKEN" \\\n  -H 'Content-Type: application/json' \\\n  -d ${quote(JSON.stringify({ model: props.modelName, messages: [{ role: "user", content: "你好" }] }))}`
    : "",
);
async function copy(value: string, label: string) {
  try {
    if (!navigator.clipboard) throw new Error("clipboard_unavailable");
    await navigator.clipboard.writeText(value);
    toast.success(`已复制 ${label}`);
  } catch {
    toast.error("复制失败", { description: "请选中文本手动复制。" });
  }
}
</script>
<template>
  <section class="min-w-0 space-y-4">
    <div class="space-y-1">
      <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
        <h2 class="text-base font-semibold">接入方式</h2>
        <RouterLink
          class="inline-flex min-h-11 items-center gap-1.5 text-sm font-medium text-primary underline-offset-4 hover:underline md:min-h-9"
          :to="modelName ? `/tokens?model=${encodeURIComponent(modelName)}` : '/tokens'"
        >
          <KeyRound class="size-4 shrink-0" aria-hidden="true" />
          创建访问令牌
          <ArrowUpRight class="size-3.5 shrink-0" aria-hidden="true" />
        </RouterLink>
      </div>
      <p class="text-sm leading-6 text-muted-foreground">
        使用个人访问令牌调用 LLM Bridge；不是提供者的上游 API Key。
      </p>
    </div>
    <TooltipProvider :delay-duration="200">
      <dl class="grid min-w-0 gap-4">
        <div class="min-w-0 space-y-2">
          <dt class="text-xs font-medium text-muted-foreground">OpenAI 兼容 Base URL</dt>
          <dd class="flex min-w-0 items-center gap-3 rounded-md border bg-muted/40 py-1 pr-1 pl-3">
            <code class="min-w-0 flex-1 text-sm leading-5 break-all select-text">{{
              baseUrl
            }}</code>
            <Tooltip>
              <TooltipTrigger as-child>
                <Button
                  size="icon"
                  variant="ghost"
                  class="size-11 transition-colors md:size-9"
                  aria-label="复制 Base URL"
                  @click="copy(baseUrl, 'Base URL')"
                >
                  <Copy class="size-4" aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>复制 Base URL</TooltipContent>
            </Tooltip>
          </dd>
          <dd class="text-xs leading-5 text-muted-foreground">
            当前访问入口；若客户端使用另一域名请替换。
          </dd>
        </div>
        <div v-if="modelName" class="min-w-0 space-y-2">
          <dt class="text-xs font-medium text-muted-foreground">客户端模型 ID</dt>
          <dd class="flex min-w-0 items-center gap-3 rounded-md border bg-muted/40 py-1 pr-1 pl-3">
            <code class="min-w-0 flex-1 text-sm leading-5 break-all select-text">{{
              modelName
            }}</code>
            <Tooltip>
              <TooltipTrigger as-child>
                <Button
                  size="icon"
                  variant="ghost"
                  class="size-11 transition-colors md:size-9"
                  aria-label="复制模型 ID"
                  @click="copy(modelName, '模型 ID')"
                >
                  <Copy class="size-4" aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>复制模型 ID</TooltipContent>
            </Tooltip>
          </dd>
        </div>
      </dl>
      <div v-if="modelName" class="min-w-0 space-y-2">
        <div class="overflow-hidden rounded-md border bg-muted/40">
          <div class="flex items-center justify-between gap-3 border-b py-1 pr-1 pl-3">
            <div class="flex items-center gap-2 text-xs font-medium text-muted-foreground">
              <Terminal class="size-4" aria-hidden="true" />
              <span>cURL 示例</span>
            </div>
            <Tooltip>
              <TooltipTrigger as-child>
                <Button
                  size="icon"
                  variant="ghost"
                  class="size-11 transition-colors md:size-9"
                  aria-label="复制示例"
                  @click="copy(example, '示例')"
                >
                  <Copy class="size-4" aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>复制示例</TooltipContent>
            </Tooltip>
          </div>
          <pre
            class="max-w-full overflow-x-auto p-3 text-xs leading-6 select-text"
          ><code>{{ example }}</code></pre>
        </div>
        <p class="text-xs leading-5 text-muted-foreground">
          先在客户端环境中设置 LLM_BRIDGE_TOKEN，再执行示例。
        </p>
      </div>
    </TooltipProvider>
  </section>
</template>
