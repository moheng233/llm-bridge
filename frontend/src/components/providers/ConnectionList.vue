<script setup lang="ts">
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";
import { Cable, Check, Pause, Pencil, Play, Trash2, FlaskConical } from "@lucide/vue";

import ModelLinkEditForm from "./ModelLinkEditForm.vue";
import { getApi, formatPrice } from "~/lib/api";
import { connectionToDraft, connectionDraftToInput } from "~/lib/connection-draft";
import { protocolLabel } from "~/lib/constants";
import { useAuthStore } from "~/stores/auth";
import { useConnectionTestsStore } from "~/stores/connection-tests";
const props = defineProps<{
  entries: { modelId: number; modelName: string; link: ModelLinkView; linkCreated?: boolean }[];
  providers: ProviderResponse[];
  readOnly?: boolean;
  fill?: boolean;
  loading?: boolean;
}>();
const emit = defineEmits<{ changed: [] }>();
const tests = useConnectionTestsStore();
const auth = useAuthStore();
const confirm = useConfirm();
const query = ref("");
const editing = ref<(typeof props.entries)[number] | null>(null);
const editOpen = ref(false);
const busy = ref<number | null>(null);
useUnsavedChanges(
  computed(() => false),
  computed(() => busy.value !== null),
);
const error = ref("");
const list = ref<HTMLElement>();
const filtered = computed(() =>
  props.entries.filter((entry) =>
    `${entry.modelName} ${entry.link.providerModelId} ${entry.link.providerDisplayName}`
      .toLowerCase()
      .includes(query.value.trim().toLowerCase()),
  ),
);
const { page, visible } = useListPagination(filtered, query);
watch([query, page], async () => {
  await nextTick();
  if (list.value) list.value.scrollTop = 0;
});
const mutation = useApiCall(async (entry: (typeof props.entries)[number], remove: boolean) =>
  remove
    ? getApi().admin.deleteModelProvider(String(entry.modelId), String(entry.link.id))
    : getApi().admin.updateModelProvider(String(entry.modelId), String(entry.link.id), {
        ...connectionDraftToInput(connectionToDraft(entry.link)),
        enabled: !entry.link.enabled,
      }),
);
async function mutate(entry: (typeof props.entries)[number], remove: boolean) {
  if (busy.value !== null || props.loading) return;
  if (
    remove &&
    !(await confirm({
      title: "移除此连接？",
      description: `${entry.modelName} → ${entry.link.providerModelId} 将解除关联；共享模型定义和其他连接不会删除。`,
      destructive: true,
      confirmText: "确认移除",
    }))
  )
    return;
  busy.value = entry.link.id;
  error.value = "";
  await mutation.execute(entry, remove);
  if (!mutation.error.value) {
    tests.invalidateLink(entry.link.id);
    emit("changed");
  } else error.value = mutation.error.value;
  busy.value = null;
}
async function test(entry: (typeof props.entries)[number]) {
  if (
    busy.value !== null ||
    props.loading ||
    !(await confirm({
      title: "测试上游？",
      description:
        "将发送一次真实请求，可能产生上游费用。这只验证当前连接，不验证个人 Token、配额或客户端配置。",
      confirmText: "发送测试请求",
    }))
  )
    return;
  busy.value = entry.link.id;
  const userId = auth.user?.userId;
  const call = useApiCall(() =>
    getApi().admin.testModelProviderReply(String(entry.modelId), String(entry.link.id), {}),
  );
  const response = await call.execute();
  if (userId === auth.user?.userId)
    tests.set({
      modelId: entry.modelId,
      providerId: entry.link.providerId,
      linkId: entry.link.id,
      testedAt: Date.now(),
      status: response?.success ? "success" : "failure",
      latencyMs: response?.latencyMs ?? null,
      message: response?.error ?? (response ? null : call.errorDetail.value || call.error.value),
    });
  busy.value = null;
}
function saved() {
  editing.value = null;
  emit("changed");
}
</script>
<template>
  <section
    :class="
      fill ? 'flex min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-hidden' : 'min-w-0 space-y-4'
    "
  >
    <ListToolbar
      v-model="query"
      class="shrink-0"
      label="搜索模型连接"
      placeholder="搜索网关 ID / 上游 ID / 提供者"
      :refreshable="!readOnly"
      :loading="loading"
      :disabled="busy !== null"
      @refresh="emit('changed')"
      @clear="query = ''"
    />
    <div
      ref="list"
      :data-scroll-area="fill ? '' : undefined"
      :tabindex="fill ? 0 : undefined"
      :class="
        fill ? 'min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain p-1' : 'space-y-4'
      "
    >
      <p class="text-xs text-muted-foreground">
        最近手动检测是本浏览会话的结果，非实时健康状态；刷新后回到未检测。价格单位：USD / 百万
        Token。
      </p>
      <ErrorState v-if="error" :error="error" inline />
      <EmptyState
        v-if="!filtered.length"
        :icon="Cable"
        :title="query ? '筛选无结果' : '尚未关联模型连接'"
        ><template v-if="query" #actions
          ><Button variant="outline" @click="query = ''">清除筛选</Button></template
        ></EmptyState
      >
      <ListItem v-for="entry in visible" :key="entry.link.id">
        <template #title>
          <RouterLink :to="`/admin/models/${entry.modelId}`">{{ entry.modelName }}</RouterLink>
        </template>
        <template #status
          ><Badge :variant="entry.link.enabled ? 'secondary' : 'outline'"
            ><Check v-if="entry.link.enabled" aria-hidden="true" /><Pause
              v-else
              aria-hidden="true"
            />{{ entry.link.enabled ? "已启用" : "已停用" }}</Badge
          ><Badge v-if="entry.linkCreated !== undefined" variant="outline">{{
            entry.linkCreated ? "已创建" : "已存在"
          }}</Badge></template
        >
        <template #subtitle>{{ entry.link.providerModelId }}</template>
        <p class="break-all">
          →
          <RouterLink :to="`/providers/${entry.link.providerId}`" class="text-primary underline">{{
            entry.link.providerDisplayName
          }}</RouterLink>
          / <code>{{ entry.link.providerModelId }}</code>
        </p>
        <p class="text-xs break-all text-muted-foreground">
          {{ protocolLabel(entry.link.protocol) }} · {{ entry.link.baseUrl }}
        </p>
        <p
          v-if="providers.find((p) => p.id === entry.link.providerId)?.enabled === false"
          class="text-warning"
        >
          提供者已停用，此连接不会参与路由。
        </p>
        <p
          v-if="
            providers
              .find((p) => p.id === entry.link.providerId)
              ?.protocols.find((p) => p.id === entry.link.protocolId)?.enabled === false
          "
          class="text-warning"
        >
          协议已停用，此连接不会参与路由。
        </p>
        <div class="grid gap-2 text-sm sm:grid-cols-3">
          <p>
            输入 / 输出价格：{{ formatPrice(entry.link.inputPricePer1m) }} /
            {{ formatPrice(entry.link.outputPricePer1m) }}
          </p>
          <p>
            输入 / 输出 Token 覆盖：{{ entry.link.maxInputTokens ?? "继承" }} /
            {{ entry.link.maxOutputTokens ?? "继承" }}
          </p>
          <p>连接优先级：{{ entry.link.priority }}（越小越优先）</p>
        </div>
        <p class="text-xs text-muted-foreground">
          工具
          {{
            entry.link.toolCalling === null ? "继承" : entry.link.toolCalling ? "支持" : "不支持"
          }}
          · 视觉 {{ entry.link.vision === null ? "继承" : entry.link.vision ? "支持" : "不支持" }} ·
          推理
          {{ entry.link.thinking === null ? "继承" : entry.link.thinking ? "支持" : "不支持" }} ·
          自适应推理
          {{
            entry.link.adaptiveThinking === null
              ? "继承"
              : entry.link.adaptiveThinking
                ? "支持"
                : "不支持"
          }}
        </p>
        <p
          v-if="tests.get(entry.link.id)"
          :class="tests.get(entry.link.id)?.status === 'success' ? 'text-success' : 'text-failure'"
        >
          {{
            tests.get(entry.link.id)?.status === "success"
              ? "✓ 最近手动检测成功"
              : "× 最近手动检测失败"
          }}
          · {{ new Date(tests.get(entry.link.id)!.testedAt).toLocaleString() }} ·
          {{ tests.get(entry.link.id)?.latencyMs ?? "—" }} ms
          <span class="break-all">{{ tests.get(entry.link.id)?.message }}</span>
        </p>
        <p v-else class="text-muted-foreground">最近手动检测：未检测</p>
        <template #actions
          ><div class="flex flex-wrap gap-2 md:max-w-64 md:justify-end">
            <template v-if="!readOnly"
              ><Button
                variant="outline"
                :disabled="busy !== null || loading"
                @click="
                  editing = entry;
                  editOpen = true;
                "
                ><Pencil aria-hidden="true" />编辑连接</Button
              ><Button
                variant="outline"
                :disabled="busy !== null || loading"
                @click="mutate(entry, false)"
                ><Pause v-if="entry.link.enabled" aria-hidden="true" /><Play
                  v-else
                  aria-hidden="true"
                />{{ entry.link.enabled ? "停用" : "启用" }}</Button
              ></template
            >
            <Button variant="outline" :disabled="busy !== null || loading" @click="test(entry)"
              ><FlaskConical aria-hidden="true" />{{
                busy === entry.link.id ? "处理中…" : "测试上游"
              }}</Button
            >
            <Button
              v-if="!readOnly"
              variant="ghost"
              class="text-destructive hover:text-destructive"
              :disabled="busy !== null || loading"
              @click="mutate(entry, true)"
              ><Trash2 aria-hidden="true" />移除连接</Button
            >
          </div></template
        >
      </ListItem>
    </div>
    <ListPagination
      v-model="page"
      class="shrink-0 border-t pt-3"
      :total="filtered.length"
      :disabled="busy !== null || loading"
    />
    <ModelLinkEditForm
      v-if="editing"
      v-model:open="editOpen"
      :model-id="editing.modelId"
      :model-name="editing.modelName"
      :editing-link="editing.link"
      :providers="providers"
      @saved="saved"
    />
  </section>
</template>
