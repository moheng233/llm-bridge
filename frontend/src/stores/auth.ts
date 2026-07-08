// Auth Pinia store — 用户认证状态管理
import { type UserResponse } from "@bindings/UserResponse";

export const useAuthStore = defineStore("auth", () => {
  const user = ref<UserResponse | null>(null);
  const loading = ref(true);
  const error = ref("");
  const isAdmin = ref(false);

  const isAuthenticated = computed(() => user.value !== null);

  async function fetchMe() {
    loading.value = true;
    error.value = "";
    try {
      const resp = await fetch("/auth/me", { credentials: "include" });
      if (!resp.ok) {
        if (resp.status === 401) {
          user.value = null;
          isAdmin.value = false;
          return;
        }
        throw new Error(`Auth error: ${resp.status}`);
      }
      const u: UserResponse = await resp.json();
      user.value = u;
      isAdmin.value = u.role === "admin";
    } catch (e: any) {
      error.value = e.message;
      user.value = null;
      isAdmin.value = false;
    } finally {
      loading.value = false;
    }
  }

  async function login() {
    window.location.href = "/auth/login";
  }

  async function logout() {
    await fetch("/auth/logout", { method: "POST", credentials: "include" });
    user.value = null;
    isAdmin.value = false;
  }

  // 初始化时获取用户信息
  fetchMe();

  return { user, loading, error, isAdmin, isAuthenticated, login, logout, fetchMe };
});
