<script setup lang="ts">
import { type UserResponse } from "@bindings/UserResponse";
import { Users, Shield } from "@lucide/vue";

import { useApiCall } from "~/composables/useApiCall";
import { getApi } from "~/lib/api";
import { SKELETON_ROWS } from "~/lib/constants";
import { useAuthStore } from "~/stores/auth";

const api = getApi();
const authStore = useAuthStore();
const { isAdmin } = storeToRefs(authStore);
const router = useRouter();
const confirm = useConfirm();
const changing = ref(false);

const users = ref<UserResponse[]>([]);

const { loading, error, execute: fetchUsers } = useApiCall(() => api.admin.listUsers());

async function loadUsers() {
  const result = await fetchUsers();
  if (result) users.value = result;
}

async function changeRole(userId: number, role: string) {
  if (changing.value || users.value.find((user) => user.id === userId)?.role === role) return;
  changing.value = true;
  try {
    if (
      !(await confirm({
        title: role === "admin" ? "授予管理员权限？" : "改为普通成员？",
        description:
          role === "admin"
            ? "管理员可以管理上游、模型与用户，并查看全站请求和用量。"
            : "成员只能使用模型、管理个人令牌并查看自己的请求和用量。修改自己的角色后将返回概览。",
        confirmText: "确认修改角色",
      }))
    )
      return;
    error.value = "";
    await api.admin.updateUserRole(String(userId), { role });
    if (authStore.user?.userId === userId) {
      await authStore.fetchMe();
      await router.replace("/dashboard");
    } else await loadUsers();
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : "角色修改失败";
  } finally {
    changing.value = false;
  }
}

watchEffect(() => {
  if (isAdmin.value) loadUsers();
});
</script>

<template>
  <PageShell>
    <SectionHeader
      title="用户"
      description="管理员可管理配置并查看全站数据；成员只访问个人用量与令牌。"
      :count="users.length"
      count-label="个用户"
      :icon="Users"
    />

    <ErrorState v-if="error" :error="error" inline @retry="loadUsers" />

    <div v-if="loading" class="flex flex-col gap-2">
      <Skeleton v-for="i in SKELETON_ROWS.users" :key="i" class="h-16 w-full rounded-lg" />
    </div>

    <EmptyState v-else-if="!error && users.length === 0" :icon="Users" title="暂无用户" />

    <div v-else-if="!error" class="flex flex-col gap-2">
      <div
        v-for="u in users"
        :key="u.id"
        class="flex flex-col justify-between gap-4 rounded-lg border border-border bg-card px-4 py-3 sm:flex-row sm:items-center"
      >
        <div class="flex min-w-0 items-center gap-3">
          <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted">
            <span class="font-mono text-sm font-medium text-foreground">{{
              u.name.charAt(0).toUpperCase()
            }}</span>
          </div>
          <div class="flex min-w-0 flex-col">
            <div class="flex items-center gap-2">
              <span class="font-medium text-foreground">{{ u.name }}</span>
              <Badge
                :variant="u.role === 'admin' ? 'default' : 'secondary'"
                class="font-mono text-xs"
                >{{ u.role === "admin" ? "管理员" : "成员" }}</Badge
              >
              <Badge v-if="!u.active" variant="destructive" class="text-xs">已禁用</Badge>
            </div>
            <span class="truncate text-xs text-muted-foreground">{{ u.email || u.oidcSub }}</span>
          </div>
        </div>
        <div class="flex shrink-0 items-center gap-2">
          <Shield class="h-4 w-4 text-muted-foreground" />
          <Select
            :model-value="u.role"
            :disabled="changing"
            @update:model-value="(v: unknown) => v && changeRole(u.id, v as string)"
          >
            <SelectTrigger class="w-32" :aria-label="`修改 ${u.name} 的角色`">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="admin">管理员</SelectItem>
              <SelectItem value="member">成员</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
    </div>
  </PageShell>
</template>
<route lang="json">
{
  "meta": { "requiresAdmin": true }
}
</route>
