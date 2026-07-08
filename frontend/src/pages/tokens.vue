<script setup lang="ts">
import { getApi, formatTime } from "~/lib/api";
import { QUOTA_PERIOD_OPTIONS, quotaPeriodLabel, SKELETON_ROWS } from "~/lib/constants";
import { useAuthStore } from "~/stores/auth";
import { Plus, Trash2, Key, Copy, Check } from "@lucide/vue";
import { type TokenListItem } from "@bindings/TokenListItem";
import { type CreateTokenResponse } from "@bindings/CreateTokenResponse";

const api = getApi();
const authStore = useAuthStore();
const { isAuthenticated } = storeToRefs(authStore);

const tokens = ref<TokenListItem[]>([]);
const loading = ref(true);
const error = ref("");
const showCreate = ref(false);
const newName = ref("");
const newRequestQuota = ref(0);
const newTokenQuota = ref(0);
const newQuotaPeriod = ref("unlimited");
const createdToken = ref<CreateTokenResponse | null>(null);
const tokenCopied = ref(false);

async function loadTokens() {
  loading.value = true;
  error.value = "";
  try {
    tokens.value = await api.tokens.listTokens();
  } catch (e: any) {
    error.value = e.message;
  } finally {
    loading.value = false;
  }
}

watchEffect(() => {
  if (isAuthenticated.value) loadTokens();
});

async function handleCreate() {
  error.value = "";
  try {
    const result = await api.tokens.createToken({
      name: newName.value,
      allowedModels: [],
      requestQuota: newRequestQuota.value,
      tokenQuota: newTokenQuota.value,
      quotaPeriod: newQuotaPeriod.value,
    });
    createdToken.value = result;
    tokenCopied.value = false;
    loadTokens();
  } catch (e: any) {
    error.value = e.message;
  }
}

async function copyToken() {
  if (createdToken.value) {
    await navigator.clipboard.writeText(createdToken.value.token);
    tokenCopied.value = true;
  }
}

function closeCreate() {
  showCreate.value = false;
  newName.value = "";
  newRequestQuota.value = 0;
  newTokenQuota.value = 0;
  newQuotaPeriod.value = "unlimited";
  createdToken.value = null;
  tokenCopied.value = false;
}

async function handleDelete(id: number) {
  error.value = "";
  try {
    await api.tokens.deleteToken(String(id));
    loadTokens();
  } catch (e: any) {
    error.value = e.message;
  }
}

async function handleToggle(t: TokenListItem) {
  error.value = "";
  try {
    await api.tokens.updateToken(String(t.id), {
      name: t.name,
      allowedModels: t.allowedModels,
      requestQuota: t.requestQuota,
      tokenQuota: t.tokenQuota,
      quotaPeriod: t.quotaPeriod,
      active: !t.active,
    });
    loadTokens();
  } catch (e: any) {
    error.value = e.message;
  }
}

function quotaLabel(t: TokenListItem): string {
  const parts: string[] = [];
  if (t.requestQuota > 0) parts.push(`${t.requestQuota} 次请求`);
  if (t.tokenQuota > 0) parts.push(`${(t.tokenQuota / 1_000_000).toFixed(1)}M tokens`);
  if (parts.length === 0) return "不限制";
  return parts.join(" · ");
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col gap-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-bold font-mono text-foreground">API Token</h2>
        <p class="text-sm text-muted-foreground mt-1">管理你的 API Token，用于调用 LLM 接口</p>
      </div>
      <Dialog :open="showCreate" @update:open="(v: boolean) => (showCreate = v)">
        <DialogTrigger as-child>
          <Button
            class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium gap-2 cursor-pointer"
            @click="showCreate = true"
          >
            <Plus class="h-4 w-4" /> 创建 Token
          </Button>
        </DialogTrigger>
        <DialogContent class="sm:max-w-md">
          <DialogHeader>
            <DialogTitle class="font-mono">创建新 Token</DialogTitle>
          </DialogHeader>
          <!-- Token created -->
          <div v-if="createdToken" class="flex flex-col gap-4">
            <Alert class="border-[#22C55E]/30 bg-[#22C55E]/10">
              <AlertDescription class="text-[#22C55E] text-sm"
                >Token 创建成功！请立即复制保存，此 Token 仅显示一次。</AlertDescription
              >
            </Alert>
            <div class="flex items-center gap-2">
              <code class="flex-1 rounded-md bg-muted px-3 py-2 font-mono text-sm break-all">{{
                createdToken.token
              }}</code>
              <Button
                size="icon"
                variant="outline"
                class="cursor-pointer shrink-0"
                @click="copyToken"
              >
                <Check v-if="tokenCopied" class="h-4 w-4 text-[#22C55E]" />
                <Copy v-else class="h-4 w-4" />
              </Button>
            </div>
            <Button variant="secondary" class="cursor-pointer" @click="closeCreate">关闭</Button>
          </div>
          <!-- Create form -->
          <div v-else class="flex flex-col gap-4">
            <div class="flex flex-col gap-2">
              <Label for="tname">名称</Label>
              <Input id="tname" v-model="newName" placeholder="dev-machine" />
            </div>
            <div class="grid grid-cols-2 gap-3">
              <div class="flex flex-col gap-2">
                <Label for="rq">请求配额</Label>
                <Input
                  id="rq"
                  v-model.number="newRequestQuota"
                  type="number"
                  placeholder="0 = 不限制"
                />
              </div>
              <div class="flex flex-col gap-2">
                <Label for="tq">Token 配额</Label>
                <Input
                  id="tq"
                  v-model.number="newTokenQuota"
                  type="number"
                  placeholder="0 = 不限制"
                />
              </div>
            </div>
            <div class="flex flex-col gap-2">
              <Label>配额周期</Label>
              <Select v-model="newQuotaPeriod">
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem
                    v-for="opt in QUOTA_PERIOD_OPTIONS"
                    :key="opt.value"
                    :value="opt.value"
                    >{{ opt.label }}</SelectItem
                  >
                </SelectContent>
              </Select>
            </div>
            <Button
              class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium cursor-pointer"
              @click="handleCreate"
              :disabled="!newName.trim()"
            >
              创建
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>

    <Alert v-if="error" class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-destructive text-sm">{{ error }}</AlertDescription>
    </Alert>

    <!-- Loading -->
    <div v-if="loading" class="flex flex-col gap-3">
      <Skeleton v-for="i in SKELETON_ROWS.tokens" :key="i" class="h-20 w-full rounded-lg" />
    </div>

    <!-- Empty -->
    <div v-else-if="tokens.length === 0" class="flex flex-1 items-center justify-center">
      <div class="flex flex-col items-center gap-3 text-muted-foreground">
        <Key class="h-12 w-12 opacity-30" />
        <p class="text-sm">暂无 Token，点击上方按钮创建</p>
      </div>
    </div>

    <!-- List -->
    <div v-else class="flex flex-col gap-3 overflow-auto">
      <div
        v-for="t in tokens"
        :key="t.id"
        class="flex items-center justify-between rounded-lg border border-border bg-card p-4 transition-colors hover:border-border/80"
      >
        <div class="flex flex-col gap-1 min-w-0">
          <div class="flex items-center gap-2">
            <span class="font-mono font-semibold text-foreground">{{ t.name }}</span>
            <code class="text-xs text-muted-foreground font-mono">{{ t.tokenPrefix }}</code>
            <Badge :variant="t.active ? 'default' : 'secondary'" class="text-xs">{{
              t.active ? "启用" : "禁用"
            }}</Badge>
          </div>
          <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
            <span>配额: {{ quotaLabel(t) }}</span>
            <span>周期: {{ quotaPeriodLabel(t.quotaPeriod) }}</span>
            <span>创建: {{ formatTime(t.createdAt) }}</span>
            <span v-if="t.lastUsedAt">最近使用: {{ formatTime(t.lastUsedAt) }}</span>
          </div>
          <div v-if="t.allowedModels.length > 0" class="flex flex-wrap gap-1 mt-1">
            <Badge
              v-for="m in t.allowedModels"
              :key="m"
              variant="outline"
              class="text-xs font-mono"
              >{{ m }}</Badge
            >
          </div>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <Button
            size="icon"
            variant="ghost"
            class="cursor-pointer h-8 w-8"
            @click="handleToggle(t)"
          >
            <Checkbox :checked="t.active" class="pointer-events-none" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            class="cursor-pointer h-8 w-8 text-muted-foreground hover:text-destructive"
            @click="handleDelete(t.id)"
          >
            <Trash2 class="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  </div>
</template>
