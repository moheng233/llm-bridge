<script setup lang="ts">
import { getApi } from "~/lib/api";
const call = useApiCall(() => getApi().admin.listAdminModels());
const query = ref("");
const page = ref(1);
const filtered = computed(() =>
  (call.data.value ?? []).filter((model) =>
    `${model.modelName} ${model.displayName} ${model.description ?? ""}`
      .toLowerCase()
      .includes(query.value.toLowerCase()),
  ),
);
watch(query, () => {
  page.value = 1;
});
onMounted(() => call.execute());
</script>
<template>
  <PageShell
    ><div class="flex flex-wrap justify-between gap-3">
      <div>
        <h1>模型定义</h1>
        <p class="mt-2 text-muted-foreground">
          共享网关模型 ID 与标称能力；价格和覆盖值属于提供者连接。
        </p>
      </div>
      <Button as-child><RouterLink to="/admin/models/new">添加模型定义</RouterLink></Button>
    </div>
    <Input v-model="query" placeholder="搜索网关 ID / 名称" aria-label="搜索模型定义" /><ErrorState
      v-if="call.error.value"
      :error="call.error.value"
      @retry="call.execute"
    />
    <div v-else-if="call.loading.value" class="space-y-3">
      <Skeleton v-for="i in 4" :key="i" class="h-20" />
    </div>
    <div v-else-if="!filtered.length" class="space-y-3 rounded border bg-card p-6">
      <p>{{ query ? "筛选无结果" : "尚未配置模型定义" }}</p>
      <Button v-if="query" variant="outline" @click="query = ''">清除筛选</Button
      ><Button v-else as-child><RouterLink to="/admin/setup">接入第一个模型</RouterLink></Button>
    </div>
    <article
      v-for="model in filtered.slice((page - 1) * 50, page * 50)"
      v-else
      :key="model.id"
      class="flex flex-col justify-between gap-4 rounded border bg-card p-4 md:flex-row"
    >
      <div class="min-w-0 space-y-2">
        <RouterLink
          :to="`/admin/models/${model.id}`"
          class="text-lg font-semibold text-primary underline"
          >{{ model.displayName || model.modelName }}</RouterLink
        >
        <p class="font-mono break-all">{{ model.modelName }}</p>
        <p>最大输入 {{ model.maxInputTokens }} / 最大输出 {{ model.maxOutputTokens }} Token</p>
        <p class="text-sm text-muted-foreground">
          {{ model.toolCalling ? "工具调用 · " : "" }}{{ model.vision ? "视觉 · " : ""
          }}{{ model.thinking ? "推理 · " : "" }}{{ model.adaptiveThinking ? "自适应推理 · " : ""
          }}{{
            model.providerCount ? `${model.providerCount} 条连接（非实时健康状态）` : "未关联提供者"
          }}
        </p>
      </div>
      <div class="flex shrink-0 flex-wrap items-center gap-2">
        <Button as-child variant="outline"
          ><RouterLink :to="`/admin/models/${model.id}`">查看</RouterLink></Button
        ><Button as-child variant="outline"
          ><RouterLink :to="`/admin/setup?modelId=${model.id}`">关联提供者</RouterLink></Button
        >
      </div>
    </article>
    <div v-if="filtered.length > 50" class="flex justify-between">
      <Button variant="outline" :disabled="page <= 1" @click="page--">上一页</Button
      ><span>{{ page }} / {{ Math.ceil(filtered.length / 50) }}</span
      ><Button variant="outline" :disabled="page * 50 >= filtered.length" @click="page++"
        >下一页</Button
      >
    </div></PageShell
  >
</template>
<route lang="json">
{ "meta": { "requiresAdmin": true } }
</route>
