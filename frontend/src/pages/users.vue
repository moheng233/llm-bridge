<script setup lang="ts">
import { getApi } from "~/lib/api";
import { useAuthStore } from "~/stores/auth";
import { SKELETON_ROWS } from "~/lib/constants";
import { Users, Shield } from "@lucide/vue";
import { type UserResponse } from "@bindings/UserResponse";

const api = getApi();
const authStore = useAuthStore();
const { isAdmin } = storeToRefs(authStore);

const users = ref<UserResponse[]>([]);
const loading = ref(true);
const error = ref("");

async function loadUsers() {
  loading.value = true;
  error.value = "";
  try {
    users.value = await api.admin.listUsers();
  } catch (e: any) {
    error.value = e.message;
  } finally {
    loading.value = false;
  }
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
  <div class="flex h-full min-h-0 flex-col gap-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-bold font-mono text-foreground">用户管理</h2>
        <p class="text-sm text-muted-foreground mt-1">管理团队成员账户与角色</p>
      </div>
      <Badge variant="secondary" class="font-mono">{{ users.length }} 个用户</Badge>
    </div>

    <Alert v-if="error" class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-destructive text-sm">{{ error }}</AlertDescription>
    </Alert>

    <div v-if="loading" class="flex flex-col gap-2">
      <Skeleton v-for="i in SKELETON_ROWS.users" :key="i" class="h-16 w-full rounded-lg" />
    </div>

    <div
      v-else-if="users.length === 0"
      class="flex flex-1 items-center justify-center text-muted-foreground"
    >
      <div class="flex flex-col items-center gap-2">
        <Users class="h-8 w-8 opacity-30" />
        <p class="text-sm">暂无用户</p>
      </div>
    </div>

    <div v-else class="flex flex-col gap-2 overflow-auto">
      <div
        v-for="u in users"
        :key="u.id"
        class="flex items-center justify-between rounded-lg border border-border bg-card px-4 py-3"
      >
        <div class="flex items-center gap-3 min-w-0">
          <div class="flex h-9 w-9 items-center justify-center rounded-full bg-muted shrink-0">
            <span class="font-mono text-sm font-medium text-foreground">{{
              u.name.charAt(0).toUpperCase()
            }}</span>
          </div>
          <div class="flex flex-col min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-medium text-foreground">{{ u.name }}</span>
              <Badge
                :variant="u.role === 'admin' ? 'default' : 'secondary'"
                class="text-xs font-mono"
                >{{ u.role }}</Badge
              >
              <Badge v-if="!u.active" variant="destructive" class="text-xs">已禁用</Badge>
            </div>
            <span class="text-xs text-muted-foreground truncate">{{ u.email || u.oidcSub }}</span>
          </div>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <Shield class="h-4 w-4 text-muted-foreground" />
          <Select
            :model-value="u.role"
            @update:model-value="(v: unknown) => v && changeRole(u.id, v as string)"
          >
            <SelectTrigger class="w-28 h-8 text-xs">
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
  </div>
</template>
<route lang="json">
{
  "meta": { "requiresAdmin": true }
}
</route>
