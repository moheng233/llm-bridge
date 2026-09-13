<script setup lang="ts">
import { Cable, Eye, Layers, Plus } from "@lucide/vue";

import { getApi, formatTokens } from "~/lib/api";
const call = useApiCall(() => getApi().admin.listAdminModels());
const query = ref("");
const filtered = computed(() =>
  (call.data.value ?? []).filter((model) =>
    `${model.modelName} ${model.displayName} ${model.description ?? ""}`
      .toLowerCase()
      .includes(query.value.trim().toLowerCase()),
  ),
);
const { page, visible } = useListPagination(filtered, query);
onMounted(() => call.execute());
</script>
<template>
  <PageShell :reset-key="`${page}:${query}`">
    <template #header
      ><SectionHeader
        title="模型定义"
        :icon="Layers"
        :count="call.data.value?.length ?? null"
        count-label="个"
    /></template>
    <template #toolbar
      ><ListToolbar
        v-model="query"
        label="搜索模型定义"
        placeholder="搜索网关 ID / 名称 / 描述"
        :loading="call.loading.value"
        @refresh="call.execute"
        @clear="query = ''"
        ><template #actions
          ><Button as-child class="min-h-11 transition-colors md:min-h-9"
            ><RouterLink to="/admin/models/new"
              ><Plus aria-hidden="true" />添加模型定义</RouterLink
            ></Button
          ></template
        ></ListToolbar
      ></template
    >
    <p class="text-muted-foreground">共享网关模型 ID 与标称能力；价格和覆盖值属于提供者连接。</p>
    <ErrorState v-if="call.error.value" :error="call.error.value" @retry="call.execute" />
    <div v-else-if="call.loading.value" role="status" aria-label="加载列表" class="space-y-3">
      <Skeleton v-for="index in 4" :key="index" class="h-24 rounded-md" />
    </div>
    <EmptyState
      v-else-if="!filtered.length"
      :icon="Layers"
      :title="query ? '筛选无结果' : '尚未配置模型定义'"
      ><template #actions
        ><Button v-if="query" variant="outline" @click="query = ''">清除筛选</Button
        ><Button v-else as-child
          ><RouterLink to="/admin/models/new"
            ><Plus aria-hidden="true" />添加模型定义</RouterLink
          ></Button
        ></template
      ></EmptyState
    >
    <ListItem v-for="model in visible" v-else :key="model.id">
      <template #title>
        <RouterLink :to="`/admin/models/${model.id}`">{{
          model.displayName || model.modelName
        }}</RouterLink>
      </template>
      <template #status
        ><Badge :variant="model.providerCount ? 'secondary' : 'outline'">{{
          model.providerCount ? `${model.providerCount} 条连接` : "未关联"
        }}</Badge></template
      >
      <template #subtitle>{{ model.modelName }}</template>
      <p>
        最大输入 {{ formatTokens(model.maxInputTokens) }} / 最大输出
        {{ formatTokens(model.maxOutputTokens) }} Token
      </p>
      <p class="text-sm text-muted-foreground">
        {{ model.toolCalling ? "工具调用 · " : "" }}{{ model.vision ? "视觉 · " : ""
        }}{{ model.thinking ? "推理 · " : "" }}{{ model.adaptiveThinking ? "自适应推理 · " : ""
        }}{{
          model.providerCount ? `${model.providerCount} 条连接（非实时健康状态）` : "未关联提供者"
        }}
      </p>
      <template #actions>
        <Button as-child variant="outline"
          ><RouterLink :to="`/admin/models/${model.id}`"
            ><Eye aria-hidden="true" />查看</RouterLink
          ></Button
        ><Button as-child variant="outline"
          ><RouterLink :to="`/admin/setup?modelId=${model.id}`"
            ><Cable aria-hidden="true" />关联提供者</RouterLink
          ></Button
        >
      </template>
    </ListItem>
    <template v-if="call.data.value && !call.error.value" #footer
      ><ListPagination v-model="page" :total="filtered.length" :disabled="call.loading.value"
    /></template>
  </PageShell>
</template>
<route lang="json">
{ "meta": { "requiresAdmin": true } }
</route>
