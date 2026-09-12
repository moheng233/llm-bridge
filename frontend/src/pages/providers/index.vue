<script setup lang="ts">
import { getApi } from "~/lib/api";
import { protocolLabel } from "~/lib/constants";
const call = useApiCall(() => getApi().admin.listProviders());
const query = ref("");
const page = ref(1);
const filtered = computed(() =>
  (call.data.value ?? []).filter((p) =>
    `${p.providerId} ${p.displayName}`.toLowerCase().includes(query.value.toLowerCase()),
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
        <h1>提供者</h1>
        <p class="mt-2 text-muted-foreground">本地上游接入实例；配置项齐备不代表上游可达。</p>
      </div>
      <Button as-child><RouterLink to="/admin/setup">添加提供者</RouterLink></Button>
    </div>
    <Input v-model="query" placeholder="搜索实例 ID / 名称" aria-label="搜索提供者" /><ErrorState
      v-if="call.error.value"
      :error="call.error.value"
      @retry="call.execute"
    />
    <div v-else-if="call.loading.value" class="space-y-3">
      <Skeleton v-for="i in 4" :key="i" class="h-20" />
    </div>
    <div v-else-if="!filtered.length" class="space-y-3 rounded border bg-card p-6">
      <p>{{ query ? "筛选无结果" : "尚未配置提供者" }}</p>
      <Button v-if="query" variant="outline" @click="query = ''">清除筛选</Button
      ><Button v-else as-child><RouterLink to="/admin/setup">接入第一个模型</RouterLink></Button>
    </div>
    <article
      v-for="provider in filtered.slice((page - 1) * 50, page * 50)"
      v-else
      :key="provider.id"
      class="flex min-h-14 flex-col justify-between gap-4 rounded border bg-card p-4 md:flex-row"
    >
      <div class="min-w-0 space-y-2">
        <RouterLink
          :to="`/providers/${provider.id}`"
          class="text-lg font-semibold text-primary underline"
          >{{ provider.displayName || provider.providerId }}</RouterLink
        >
        <p class="font-mono break-all">{{ provider.providerId }}</p>
        <p class="text-sm text-muted-foreground">
          {{ provider.protocols.map((p) => protocolLabel(p.protocol)).join(" / ") || "缺协议" }} ·
          {{ provider.modelCount }} 个连接
        </p>
        <p class="text-sm">
          {{ provider.enabled ? "✓ 已启用" : "− 已停用" }} ·
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
      </div>
      <div class="flex shrink-0 flex-wrap items-center gap-2">
        <Button as-child variant="outline"
          ><RouterLink :to="`/providers/${provider.id}`">查看</RouterLink></Button
        ><Button as-child variant="outline"
          ><RouterLink :to="`/admin/setup?providerId=${provider.id}`">添加模型</RouterLink></Button
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
