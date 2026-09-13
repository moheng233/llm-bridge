<script setup lang="ts">
import { type UserResponse } from "@bindings/UserResponse";
import { Check, Pause, Users, Shield } from "@lucide/vue";

import { useApiCall } from "~/composables/useApiCall";
import { getApi } from "~/lib/api";
import { useAuthStore } from "~/stores/auth";

const api = getApi();
const authStore = useAuthStore();
const { isAdmin } = storeToRefs(authStore);
const router = useRouter();
const confirm = useConfirm();
const changing = ref(false);

const users = ref<UserResponse[]>([]);
const query = ref("");
const filtered = computed(() =>
  users.value.filter((user) =>
    `${user.name} ${user.email ?? ""} ${user.oidcSub}`
      .toLowerCase()
      .includes(query.value.trim().toLowerCase()),
  ),
);
const { page, visible } = useListPagination(filtered, query);

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
  <PageShell :reset-key="`${page}:${query}`">
    <template #header>
      <SectionHeader title="用户" :count="users.length" count-label="个" :icon="Users" />
    </template>
    <template #toolbar
      ><ListToolbar
        v-model="query"
        label="搜索用户"
        placeholder="搜索名称 / 邮箱 / 身份 ID"
        :loading="loading"
        :disabled="changing"
        @refresh="loadUsers"
        @clear="query = ''"
    /></template>
    <p class="text-muted-foreground">管理员可管理配置并查看全站数据；成员只访问个人用量与令牌。</p>
    <ErrorState v-if="error" :error="error" @retry="loadUsers" />

    <div v-else-if="loading" role="status" aria-label="加载列表" class="space-y-3">
      <Skeleton v-for="index in 4" :key="index" class="h-24 rounded-md" />
    </div>

    <EmptyState
      v-else-if="!filtered.length"
      :icon="Users"
      :title="query ? '筛选无结果' : '暂无用户'"
      ><template v-if="query" #actions
        ><Button variant="outline" @click="query = ''">清除筛选</Button></template
      ></EmptyState
    >

    <ListItem v-else v-for="user in visible" :key="user.id">
      <template #title>{{ user.name }}</template>
      <template #status
        ><Badge :variant="user.active ? 'secondary' : 'outline'"
          ><Check v-if="user.active" aria-hidden="true" /><Pause v-else aria-hidden="true" />{{
            user.active ? "已启用" : "已禁用"
          }}</Badge
        ></template
      >
      <template #subtitle>{{ user.email || user.oidcSub }}</template>
      <p>{{ user.role === "admin" ? "管理员" : "成员" }}</p>
      <template #actions>
        <Shield class="h-4 w-4 text-muted-foreground" />
        <Select
          :model-value="user.role"
          :disabled="changing"
          @update:model-value="(value: unknown) => value && changeRole(user.id, value as string)"
        >
          <SelectTrigger class="w-32" :aria-label="`修改 ${user.name} 的角色`">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="admin">管理员</SelectItem>
            <SelectItem value="member">成员</SelectItem>
          </SelectContent>
        </Select>
      </template>
    </ListItem>
    <template v-if="!error" #footer
      ><ListPagination v-model="page" :total="filtered.length" :disabled="loading || changing"
    /></template>
  </PageShell>
</template>
<route lang="json">
{
  "meta": { "requiresAdmin": true }
}
</route>
