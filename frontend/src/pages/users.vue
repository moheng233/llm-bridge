<script setup lang="ts">
import { type UserResponse } from "@bindings/UserResponse";
import { Users, Shield } from "@lucide/vue";

import { getApi } from "~/lib/api";
import { SKELETON_ROWS } from "~/lib/constants";
import { useApiCall } from "~/composables/useApiCall";
import { useAuthStore } from "~/stores/auth";

const api = getApi();
const authStore = useAuthStore();
const { isAdmin } = storeToRefs(authStore);

const users = ref<UserResponse[]>([]);

const { loading, error, execute: fetchUsers } = useApiCall(() => api.admin.listUsers());

async function loadUsers() {
  const result = await fetchUsers();
  if (result) users.value = result;
}

async function changeRole(userId: number, role: string) {
  error.value = "";
  try {
    await api.admin.updateUserRole(String(userId), { role });
    loadUsers();
  } catch (e: any) {
    error.value = e.message;
  }
}

watchEffect(() => {
  if (isAdmin.value) loadUsers();
});
</script>

<template>
  <PageShell>
    <SectionHeader title="用户管理" description="管理团队成员账户与角色" :count="users.length" count-label="个用户" :icon="Users" />

    <ErrorState v-if="error" :error="error" inline @retry="loadUsers" />

    <div v-if="loading" class="flex flex-col gap-2">
      <Skeleton v-for="i in SKELETON_ROWS.users" :key="i" class="h-16 w-full rounded-lg" />
    </div>

    <EmptyState v-else-if="users.length === 0" :icon="Users" title="暂无用户" />

    <div v-else class="flex flex-col gap-2 overflow-auto">
      <div
        v-for="u in users"
        :key="u.id"
        class="flex items-center justify-between rounded-lg border border-border bg-card px-4 py-3"
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
                >{{ u.role }}</Badge
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
            @update:model-value="(v: unknown) => v && changeRole(u.id, v as string)"
          >
            <SelectTrigger class="h-8 w-28 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="admin">admin</SelectItem>
              <SelectItem value="member">member</SelectItem>
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
