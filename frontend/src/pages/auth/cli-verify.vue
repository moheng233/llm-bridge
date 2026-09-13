<script setup lang="ts">
import { useApiCall } from "~/composables/useApiCall";
import { getApi } from "~/lib/api";

const userCode = ref("");
const approved = ref(false);
const validCode = computed(() => /^[2-9]{6}$/.test(userCode.value));
const { loading, error, execute } = useApiCall(() =>
  getApi().cliAuth.confirmSession({ userCode: userCode.value }),
);
async function confirm() {
  if (!validCode.value || loading.value || approved.value) return;
  const result = await execute();
  if (result) approved.value = true;
}
</script>

<template>
  <PageShell>
    <template #header><SectionHeader title="客户端授权" /></template>
    <p>仅为你刚刚在支持网关设备码协议的客户端中发起的登录操作授权。</p>
    <div class="max-w-lg rounded-xl border border-border bg-card p-6">
      <div v-if="approved" role="status" class="space-y-3">
        <h2 class="text-lg font-semibold text-primary">已确认授权</h2>
        <p class="text-muted-foreground">
          请返回发起请求的客户端完成登录。Token 由该客户端一次性领取，本页不会展示凭据。
        </p>
        <RouterLink to="/tokens" class="text-primary hover:underline">管理访问令牌</RouterLink>
      </div>
      <form v-else id="cli-confirm-form" class="space-y-5" @submit.prevent="confirm">
        <p class="text-sm text-muted-foreground">
          请输入客户端显示的六位用户码（数字 2–9）。确认后会签发全模型、无限额的 vscode
          Token，并吊销你之前的 vscode Token，使用旧 Token
          的客户端需要重新登录。请勿替他人输入用户码。
        </p>
        <div class="space-y-2">
          <Label for="device-code">用户码</Label>
          <Input
            id="device-code"
            v-model="userCode"
            inputmode="numeric"
            autocomplete="off"
            pattern="[2-9]{6}"
            minlength="6"
            maxlength="6"
            required
            placeholder="输入六位用户码"
            class="font-mono tracking-widest"
            :disabled="loading"
          />
        </div>
        <p v-if="error" role="alert" class="text-sm text-destructive">{{ error }}</p>
      </form>
    </div>
    <template v-if="!approved" #footer
      ><Button type="submit" form="cli-confirm-form" :disabled="!validCode || loading">{{
        loading ? "正在确认…" : "确认授权"
      }}</Button></template
    >
  </PageShell>
</template>
