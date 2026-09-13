<script setup lang="ts">
import { type CreateTokenRequest } from "@bindings/CreateTokenRequest";
import { type CreateTokenResponse } from "@bindings/CreateTokenResponse";
import { type TokenListItem } from "@bindings/TokenListItem";
import { Check, KeyRound, Pause, Pencil, Play, Plus, Trash2 } from "@lucide/vue";

import { getApi, formatTime } from "~/lib/api";
import { QUOTA_PERIOD_OPTIONS, quotaPeriodLabel } from "~/lib/constants";
import { focusInScrollArea } from "~/lib/utils";
import { useAuthStore } from "~/stores/auth";
const api = getApi();
const route = useRoute();
const confirm = useConfirm();
const auth = useAuthStore();
const list = useApiCall(() => api.tokens.listTokens());
const models = useApiCall(() => api.models.listAllModels());
const listQuery = ref("");
const filteredTokens = computed(() => {
  const query = listQuery.value.trim().toLowerCase();
  return (list.data.value ?? []).filter((token) =>
    `${token.name} ${token.tokenPrefix} ${token.allowedModels.join(" ")}`
      .toLowerCase()
      .includes(query),
  );
});
const { page, visible } = useListPagination(filteredTokens, listQuery);
const showCreate = ref(false);
const editing = ref<TokenListItem | null>(null);
const empty = (): CreateTokenRequest => ({
  name: "",
  allowedModels: [],
  requestQuota: 0,
  tokenQuota: 0,
  quotaPeriod: "unlimited",
});
const draft = ref(empty());
const allModels = ref(true);
const search = ref("");
const baseline = ref("");
const createdToken = ref<CreateTokenResponse | null>(null);
const tokenCopied = ref(false);
const copyError = ref("");
const fieldErrors = ref<Record<string, string>>({});
const scopeError = ref("");
const actionError = ref("");
const rowBusy = ref(false);
const closing = ref(false);
const snapshot = () => JSON.stringify({ draft: draft.value, allModels: allModels.value });
const dirty = computed(
  () => showCreate.value && (createdToken.value !== null || snapshot() !== baseline.value),
);
const save = useApiCall(async () => {
  const input = {
    ...draft.value,
    name: draft.value.name.trim(),
    allowedModels: allModels.value ? [] : draft.value.allowedModels,
  };
  if (editing.value)
    return api.tokens.updateToken(String(editing.value.id), {
      ...input,
      active: editing.value.active,
    });
  const result = await api.tokens.createToken(input);
  createdToken.value = result;
  return result;
});
const busy = computed(() => save.loading.value || rowBusy.value);
const choices = computed(() => {
  const rows = new Map(
    (models.data.value ?? []).map((model) => [model.modelName, model.displayName]),
  );
  for (const name of draft.value.allowedModels)
    if (!rows.has(name)) rows.set(name, "已不在本地模型列表中");
  const query = search.value.toLowerCase();
  return [...rows].filter(([name, label]) => `${name} ${label}`.toLowerCase().includes(query));
});
function clearSensitive() {
  createdToken.value = null;
  tokenCopied.value = false;
  copyError.value = "";
  showCreate.value = false;
  editing.value = null;
  draft.value = empty();
  allModels.value = true;
  scopeError.value = "";
  fieldErrors.value = {};
  search.value = "";
}
async function closeCreate(): Promise<boolean> {
  if (busy.value || closing.value) return false;
  if (!showCreate.value) return true;
  closing.value = true;
  try {
    if (createdToken.value && !tokenCopied.value) {
      if (
        !(await confirm({
          title: "关闭一次性令牌？",
          description: "关闭后无法再次查看明文。若已手动复制并妥善保存，可以继续关闭。",
          confirmText: "已保存，关闭",
        }))
      )
        return false;
    } else if (
      !createdToken.value &&
      dirty.value &&
      !(await confirm({
        title: "放弃未保存的修改？",
        description: "尚未保存的令牌名称、范围和配额修改将丢失。",
        confirmText: "放弃修改",
      }))
    )
      return false;
    clearSensitive();
    return true;
  } finally {
    closing.value = false;
  }
}
function openCreate(token: TokenListItem | null = null) {
  if (busy.value || showCreate.value) return;
  clearSensitive();
  save.clearError();
  actionError.value = "";
  editing.value = token;
  if (token)
    draft.value = {
      name: token.name,
      allowedModels: [...token.allowedModels],
      requestQuota: token.requestQuota,
      tokenQuota: token.tokenQuota,
      quotaPeriod: token.quotaPeriod,
    };
  allModels.value = draft.value.allowedModels.length === 0;
  baseline.value = snapshot();
  showCreate.value = true;
}
async function prefill() {
  if (typeof route.query.model !== "string") return;
  openCreate();
  if (!showCreate.value || editing.value) return;
  allModels.value = false;
  const model = models.data.value?.find((model) => model.modelName === route.query.model);
  if (model) {
    draft.value.allowedModels = [model.modelName];
    scopeError.value = "";
  } else
    scopeError.value = models.error.value
      ? "模型列表加载失败，请重试后明确选择范围；不会自动授权全部模型。"
      : "指定模型不存在或已删除，请重新选择模型范围；不会自动授权全部模型。";
  baseline.value = snapshot();
}
function toggleModel(name: string, checked: boolean) {
  draft.value.allowedModels = checked
    ? [...new Set([...draft.value.allowedModels, name])]
    : draft.value.allowedModels.filter((value) => value !== name);
  scopeError.value = "";
}
async function submit() {
  if (busy.value || createdToken.value) return;
  fieldErrors.value = {};
  if (!draft.value.name.trim()) fieldErrors.value.name = "请输入令牌名称";
  for (const key of ["requestQuota", "tokenQuota"] as const)
    if (!Number.isSafeInteger(draft.value[key]) || draft.value[key] < 0)
      fieldErrors.value[key] = "请输入非负安全整数；0 表示不限制";
  if (scopeError.value || (!allModels.value && !draft.value.allowedModels.length))
    fieldErrors.value.scope = scopeError.value || "请选择至少一个模型，或明确选择全部模型";
  if (Object.keys(fieldErrors.value).length) {
    await nextTick();
    focusInScrollArea(
      document.querySelector<HTMLElement>('[data-slot="sheet-content"] [aria-invalid="true"]'),
    );
    return;
  }
  const result = await save.execute();
  if (!result) return;
  baseline.value = snapshot();
  if (editing.value) clearSensitive();
  await list.execute();
}
async function copyToken() {
  const value = createdToken.value?.token;
  if (!value) return;
  copyError.value = "";
  try {
    if (!navigator.clipboard) throw new Error("当前浏览器不支持剪贴板 API");
    await navigator.clipboard.writeText(value);
    if (createdToken.value?.token === value) tokenCopied.value = true;
  } catch {
    copyError.value = "复制失败，请选中上方令牌手动复制并妥善保存。";
  }
}
async function mutate(token: TokenListItem, remove: boolean) {
  if (busy.value) return;
  rowBusy.value = true;
  actionError.value = "";
  try {
    if (
      remove &&
      !(await confirm({
        title: `删除访问令牌「${token.name}」？`,
        description:
          "使用此 Token 的客户端将无法继续调用。此操作不可撤销；暂时停用请使用停用操作。",
        destructive: true,
        confirmText: "确认删除",
      }))
    )
      return;
    if (remove) await api.tokens.deleteToken(String(token.id));
    else
      await api.tokens.updateToken(String(token.id), {
        name: null,
        allowedModels: null,
        requestQuota: null,
        tokenQuota: null,
        quotaPeriod: null,
        active: !token.active,
      });
    await list.execute();
  } catch (error) {
    actionError.value = error instanceof Error ? error.message : "操作失败";
  } finally {
    rowBusy.value = false;
  }
}
function beforeUnload(event: BeforeUnloadEvent) {
  if (dirty.value || busy.value) {
    event.preventDefault();
    event.returnValue = "";
  }
}
onBeforeRouteLeave(closeCreate);
onBeforeRouteUpdate(async (to, from) =>
  to.query.model !== from.query.model ? closeCreate() : true,
);
watch(
  () => route.query.model,
  () => prefill(),
);
watch(() => auth.user?.userId, clearSensitive);
onMounted(async () => {
  window.addEventListener("beforeunload", beforeUnload);
  await Promise.all([list.execute(), models.execute()]);
  await prefill();
});
onBeforeUnmount(() => {
  window.removeEventListener("beforeunload", beforeUnload);
  clearSensitive();
});
</script>

<template>
  <PageShell :reset-key="`${page}:${listQuery}`">
    <template #header>
      <SectionHeader
        title="访问令牌"
        :icon="KeyRound"
        :count="list.data.value?.length ?? null"
        count-label="个"
      />
    </template>
    <template #toolbar
      ><ListToolbar
        v-model="listQuery"
        label="搜索访问令牌"
        placeholder="搜索名称 / 令牌前缀 / 模型范围"
        :loading="list.loading.value"
        :disabled="busy"
        @refresh="list.execute"
        @clear="listQuery = ''"
        ><template #actions
          ><Button
            class="min-h-11 transition-colors md:min-h-9"
            :disabled="busy"
            @click="openCreate()"
            ><Plus aria-hidden="true" />创建访问令牌</Button
          >
        </template></ListToolbar
      ></template
    >
    <p class="text-muted-foreground">
      客户端访问 LLM Bridge 的个人凭据；不是提供者的上游 API Key。
    </p>
    <ErrorState v-if="actionError" :error="actionError" inline />
    <ErrorState v-if="list.error.value" :error="list.error.value" @retry="list.execute" />
    <div v-else-if="list.loading.value" role="status" aria-label="加载列表" class="space-y-3">
      <Skeleton v-for="index in 4" :key="index" class="h-24 rounded-md" />
    </div>
    <EmptyState
      v-else-if="!filteredTokens.length"
      :icon="KeyRound"
      :title="listQuery ? '筛选无结果' : '尚未创建个人访问令牌'"
    >
      <template #actions
        ><Button v-if="listQuery" variant="outline" @click="listQuery = ''">清除筛选</Button
        ><Button v-else :disabled="busy" @click="openCreate()"
          ><Plus aria-hidden="true" />创建访问令牌</Button
        ></template
      >
    </EmptyState>
    <ListItem v-for="token in visible" v-else :key="token.id">
      <template #title
        ><button type="button" :disabled="busy" @click="openCreate(token)">
          {{ token.name }}
        </button></template
      >
      <template #status
        ><Badge :variant="token.active ? 'secondary' : 'outline'"
          ><Check v-if="token.active" aria-hidden="true" /><Pause v-else aria-hidden="true" />{{
            token.active ? "已启用" : "已停用"
          }}</Badge
        ></template
      >
      <template #subtitle>{{ token.tokenPrefix }}…</template>
      <p class="text-sm break-words">
        个人范围：{{ token.allowedModels.length ? token.allowedModels.join("、") : "全部模型" }}
      </p>
      <p class="text-sm text-muted-foreground">
        个人自限：{{ token.requestQuota ? `${token.requestQuota} 次请求` : "请求不限" }} ·
        {{ token.tokenQuota ? `${token.tokenQuota} Token` : "Token 不限" }} ·
        {{ quotaPeriodLabel(token.quotaPeriod) }}
      </p>
      <p class="text-sm text-muted-foreground">
        最近使用：{{ token.lastUsedAt === null ? "尚未使用" : formatTime(token.lastUsedAt) }}
      </p>
      <template #actions>
        <Button variant="outline" :disabled="busy" @click="openCreate(token)"
          ><Pencil aria-hidden="true" />编辑</Button
        ><Button variant="outline" :disabled="busy" @click="mutate(token, false)"
          ><Pause v-if="token.active" aria-hidden="true" /><Play v-else aria-hidden="true" />{{
            token.active ? "停用" : "启用"
          }}</Button
        ><Button
          variant="ghost"
          class="text-destructive hover:text-destructive"
          :disabled="busy"
          @click="mutate(token, true)"
          ><Trash2 aria-hidden="true" />删除</Button
        >
      </template>
    </ListItem>
    <template v-if="list.data.value && !list.error.value" #footer
      ><ListPagination
        v-model="page"
        :total="filteredTokens.length"
        :disabled="list.loading.value || busy"
    /></template>
    <Sheet
      :open="showCreate"
      @update:open="
        (value) => {
          if (!value) closeCreate();
        }
      "
    >
      <SheetContent
        class="w-full max-w-full gap-0 sm:max-w-2xl"
        @escape-key-down="
          (event) => {
            event.preventDefault();
            closeCreate();
          }
        "
      >
        <SheetHeader class="shrink-0 border-b p-4 pr-12"
          ><SheetTitle>{{
            createdToken ? "访问令牌已创建" : editing ? "编辑访问令牌" : "创建访问令牌"
          }}</SheetTitle
          ><SheetDescription>{{
            createdToken
              ? "明文仅显示这一次；请妥善保存，不要将令牌发给他人。"
              : "模型范围与配额是你可修改的个人自限，不是管理员强制预算。"
          }}</SheetDescription></SheetHeader
        >
        <div
          v-if="createdToken"
          data-scroll-area
          class="min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain p-4"
        >
          <Label for="issued-token">一次性访问令牌</Label
          ><Input
            id="issued-token"
            :model-value="createdToken.token"
            readonly
            class="font-mono"
            @focus="($event.target as HTMLInputElement).select()"
          />
          <div class="flex flex-wrap gap-2">
            <Button @click="copyToken">{{ tokenCopied ? "已复制" : "复制令牌" }}</Button
            ><Button variant="outline" @click="tokenCopied = true">我已手动复制并保存</Button>
          </div>
          <p v-if="copyError" role="alert" class="text-destructive">{{ copyError }}</p>
          <ClientConnectionInfo
            :model-name="
              createdToken.allowedModels.length === 1 ? createdToken.allowedModels[0] : undefined
            "
          />
        </div>
        <form
          v-else
          id="token-form"
          data-scroll-area
          class="min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain p-4"
          @submit.prevent="submit"
        >
          <fieldset :disabled="busy" class="space-y-4">
            <div class="space-y-2">
              <Label for="token-name">名称</Label
              ><Input id="token-name" v-model="draft.name" :aria-invalid="!!fieldErrors.name" />
              <p v-if="fieldErrors.name" class="text-destructive">{{ fieldErrors.name }}</p>
            </div>
            <details
              :open="
                !allModels ||
                !!scopeError ||
                !!fieldErrors.scope ||
                !!fieldErrors.requestQuota ||
                !!fieldErrors.tokenQuota
              "
            >
              <summary class="cursor-pointer py-2">模型范围与个人配额</summary>
              <div class="space-y-4 pt-3">
                <label class="flex min-h-11 items-center gap-2"
                  ><Checkbox
                    :model-value="allModels"
                    :aria-invalid="!!fieldErrors.scope"
                    @update:model-value="
                      (value) => {
                        allModels = value === true;
                        scopeError = '';
                      }
                    "
                  />全部模型</label
                >
                <div v-if="!allModels" class="space-y-2">
                  <Input
                    v-model="search"
                    aria-label="搜索模型范围"
                    placeholder="搜索模型 ID / 名称"
                  /><ErrorState
                    v-if="models.error.value"
                    :error="models.error.value"
                    @retry="models.execute"
                  />
                  <div
                    data-scroll-area
                    class="max-h-52 overflow-y-auto overscroll-contain rounded border p-2"
                  >
                    <label
                      v-for="[name, label] in choices"
                      :key="name"
                      class="flex min-h-11 items-center gap-2"
                      ><Checkbox
                        :model-value="draft.allowedModels.includes(name)"
                        @update:model-value="(value) => toggleModel(name, value === true)"
                      /><span class="min-w-0 break-all"
                        ><code>{{ name }}</code> · {{ label }}</span
                      ></label
                    >
                    <p v-if="!choices.length" class="p-2 text-muted-foreground">
                      {{ models.loading.value ? "正在加载模型…" : "没有匹配模型" }}
                    </p>
                  </div>
                </div>
                <p v-if="scopeError || fieldErrors.scope" role="alert" class="text-destructive">
                  {{ scopeError || fieldErrors.scope }}
                </p>
                <div class="grid gap-4 sm:grid-cols-2">
                  <div class="space-y-2">
                    <Label for="request-quota">请求次数自限（0 不限制）</Label
                    ><Input
                      id="request-quota"
                      v-model.number="draft.requestQuota"
                      type="number"
                      min="0"
                      step="1"
                      :aria-invalid="!!fieldErrors.requestQuota"
                    />
                    <p v-if="fieldErrors.requestQuota" class="text-destructive">
                      {{ fieldErrors.requestQuota }}
                    </p>
                  </div>
                  <div class="space-y-2">
                    <Label for="token-quota">Token 用量自限（0 不限制）</Label
                    ><Input
                      id="token-quota"
                      v-model.number="draft.tokenQuota"
                      type="number"
                      min="0"
                      step="1"
                      :aria-invalid="!!fieldErrors.tokenQuota"
                    />
                    <p v-if="fieldErrors.tokenQuota" class="text-destructive">
                      {{ fieldErrors.tokenQuota }}
                    </p>
                  </div>
                </div>
                <div class="space-y-2">
                  <Label for="quota-period">配额周期</Label
                  ><Select v-model="draft.quotaPeriod"
                    ><SelectTrigger id="quota-period"><SelectValue /></SelectTrigger
                    ><SelectContent
                      ><SelectItem
                        v-for="option in QUOTA_PERIOD_OPTIONS"
                        :key="option.value"
                        :value="option.value"
                        >{{ option.label }}</SelectItem
                      ></SelectContent
                    ></Select
                  >
                </div>
              </div>
            </details>
          </fieldset>
          <ErrorState
            v-if="save.error.value"
            :error="save.errorDetail.value || save.error.value"
            inline
          />
        </form>
        <div class="flex shrink-0 justify-end gap-2 border-t p-4">
          <Button variant="outline" :disabled="busy || closing" @click="closeCreate">{{
            createdToken ? "关闭" : "取消"
          }}</Button
          ><Button v-if="!createdToken" form="token-form" type="submit" :disabled="busy">{{
            busy ? "保存中…" : editing ? "保存修改" : "创建访问令牌"
          }}</Button>
        </div>
      </SheetContent>
    </Sheet>
  </PageShell>
</template>
