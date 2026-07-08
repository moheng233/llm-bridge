<script lang="ts">
  import { getApi } from "$lib/api";
  import { auth } from "$lib/stores/auth.svelte";
  import { SKELETON_ROWS } from "$lib/constants";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
  import { Skeleton } from "$lib/components/ui/skeleton/index.js";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from "$lib/components/ui/select/index.js";
  import { Users, Shield } from "@lucide/svelte";
  import type { UserResponse } from "$bindings/UserResponse";

  const api = getApi();

  let users = $state<UserResponse[]>([]);
  let loading = $state(true);
  let error = $state("");

  async function loadUsers() {
    loading = true;
    error = "";
    try {
      users = await api.admin.listUsers();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function changeRole(userId: number, role: string) {
    error = "";
    try {
      await api.admin.updateUserRole(String(userId), { role });
      loadUsers();
    } catch (e: any) {
      error = e.message;
    }
  }

  $effect(() => {
    if (auth.isAdmin) loadUsers();
  });
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-xl font-bold font-mono text-foreground">用户管理</h2>
      <p class="text-sm text-muted-foreground mt-1">管理团队成员账户与角色</p>
    </div>
    <Badge variant="secondary" class="font-mono">{users.length} 个用户</Badge>
  </div>

  {#if error}
    <Alert class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-destructive text-sm">{error}</AlertDescription>
    </Alert>
  {/if}

  {#if loading}
    <div class="flex flex-col gap-2">
      {#each Array(SKELETON_ROWS.users) as _}
        <Skeleton class="h-16 w-full rounded-lg" />
      {/each}
    </div>
  {:else if users.length === 0}
    <div class="flex flex-1 items-center justify-center text-muted-foreground">
      <div class="flex flex-col items-center gap-2">
        <Users class="h-8 w-8 opacity-30" />
        <p class="text-sm">暂无用户</p>
      </div>
    </div>
  {:else}
    <div class="flex flex-col gap-2 overflow-auto">
      {#each users as u}
        <div class="flex items-center justify-between rounded-lg border border-border bg-card px-4 py-3">
          <div class="flex items-center gap-3 min-w-0">
            <div class="flex h-9 w-9 items-center justify-center rounded-full bg-muted shrink-0">
              <span class="font-mono text-sm font-medium text-foreground">
                {u.name.charAt(0).toUpperCase()}
              </span>
            </div>
            <div class="flex flex-col min-w-0">
              <div class="flex items-center gap-2">
                <span class="font-medium text-foreground">{u.name}</span>
                <Badge variant={u.role === "admin" ? "default" : "secondary"} class="text-xs font-mono">
                  {u.role}
                </Badge>
                {#if !u.active}
                  <Badge variant="destructive" class="text-xs">已禁用</Badge>
                {/if}
              </div>
              <span class="text-xs text-muted-foreground truncate">{u.email || u.oidcSub}</span>
            </div>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <Shield class="h-4 w-4 text-muted-foreground" />
            <Select type="single" value={u.role} onValueChange={(v) => v && changeRole(u.id, v)}>
              <SelectTrigger class="w-28 h-8 text-xs">
                <span class="text-xs">{u.role}</span>
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="admin">admin</SelectItem>
                <SelectItem value="member">member</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
