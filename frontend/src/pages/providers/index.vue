<script setup lang="ts">
import { Cable, Check, Eye, Pause, Plus, Server } from "@lucide/vue";

import { getApi } from "~/lib/api";
import { protocolLabel } from "~/lib/constants";
const call = useApiCall(() => getApi().admin.listProviders());
const query = ref("");
const filtered = computed(() =>
  (call.data.value ?? []).filter((p) =>
    `${p.providerId} ${p.displayName}`.toLowerCase().includes(query.value.trim().toLowerCase()),
  ),
);
const { page, visible } = useListPagination(filtered, query);
onMounted(() => call.execute());
</script>
<template>
  <PageShell :reset-key="`${page}:${query}`">
    <template #header
      ><SectionHeader
        title="提供者"
        :icon="Server"
        :count="call.data.value?.length ?? null"
        count-label="个"
    /></template>
    <template #toolbar
      ><ListToolbar
        v-model="query"
        label="搜索提供者"
        placeholder="搜索实例 ID / 名称"
        :loading="call.loading.value"
        @refresh="call.execute"
        @clear="query = ''"
        ><template #actions
          ><Button as-child class="min-h-11 transition-colors md:min-h-9"
            ><RouterLink to="/admin/setup"
              ><Plus aria-hidden="true" />添加提供者</RouterLink
            ></Button
          ></template
        ></ListToolbar
      ></template
    >
    <p class="text-muted-foreground">本地上游接入实例；配置项齐备不代表上游可达。</p>
    <ErrorState v-if="call.error.value" :error="call.error.value" @retry="call.execute" />
    <div v-else-if="call.loading.value" role="status" aria-label="加载列表" class="space-y-3">
      <Skeleton v-for="index in 4" :key="index" class="h-24 rounded-md" />
    </div>
    <EmptyState
      v-else-if="!filtered.length"
      :icon="Server"
      :title="query ? '筛选无结果' : '尚未配置提供者'"
    >
      <template #actions
        ><Button v-if="query" variant="outline" @click="query = ''">清除筛选</Button
        ><Button v-else as-child
          ><RouterLink to="/admin/setup"><Plus aria-hidden="true" />添加提供者</RouterLink></Button
        ></template
      >
    </EmptyState>
    <ListItem v-for="provider in visible" v-else :key="provider.id">
      <template #title>
        <RouterLink :to="`/providers/${provider.id}`">{{
          provider.displayName || provider.providerId
        }}</RouterLink>
      </template>
      <template #status
        ><Badge :variant="provider.enabled ? 'secondary' : 'outline'"
          ><Check v-if="provider.enabled" aria-hidden="true" /><Pause v-else aria-hidden="true" />{{
            provider.enabled ? "已启用" : "已停用"
          }}</Badge
        ></template
      >
      <template #subtitle>{{ provider.providerId }}</template>
      <p class="text-sm text-muted-foreground">
        {{ provider.protocols.map((p) => protocolLabel(p.protocol)).join(" / ") || "缺协议" }} ·
        {{ provider.modelCount }} 个连接
      </p>
      <p class="text-sm">
        {{
          !provider.protocols.some((p) => p.enabled)
            ? "缺已启用协议"
            : !provider.apiKeys.length
              ? "缺 Key"
              : !provider.modelCount
                ? "未关联模型"
                : "配置项齐备（未验证真实性）"
        }}
      </p>
      <template #actions>
        <Button as-child variant="outline"
          ><RouterLink :to="`/providers/${provider.id}`"
            ><Eye aria-hidden="true" />查看</RouterLink
          ></Button
        ><Button as-child variant="outline"
          ><RouterLink :to="`/admin/setup?providerId=${provider.id}`"
            ><Cable aria-hidden="true" />添加模型</RouterLink
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
