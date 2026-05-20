// Auth state store — Svelte 5 runes reactive state.
import type { UserResponse } from "$bindings/UserResponse";

interface AuthState {
  user: UserResponse | null;
  loading: boolean;
  error: string;
  isAdmin: boolean;
}

function createAuthStore() {
  let state = $state<AuthState>({
    user: null,
    loading: true,
    error: "",
    isAdmin: false,
  });

  async function fetchMe() {
    state.loading = true;
    state.error = "";
    try {
      const resp = await fetch("/auth/me", { credentials: "include" });
      if (!resp.ok) {
        if (resp.status === 401) {
          state.user = null;
          state.isAdmin = false;
          return;
        }
        throw new Error(`Auth error: ${resp.status}`);
      }
      const user: UserResponse = await resp.json();
      state.user = user;
      state.isAdmin = user.role === "admin";
    } catch (e: any) {
      state.error = e.message;
      state.user = null;
      state.isAdmin = false;
    } finally {
      state.loading = false;
    }
  }

  async function login() {
    window.location.href = "/auth/login";
  }

  async function logout() {
    await fetch("/auth/logout", { method: "POST", credentials: "include" });
    state.user = null;
    state.isAdmin = false;
  }

  // Initialize on import
  fetchMe();

  return {
    get user() { return state.user; },
    get loading() { return state.loading; },
    get error() { return state.error; },
    get isAdmin() { return state.isAdmin; },
    get isAuthenticated() { return state.user !== null; },
    login,
    logout,
    fetchMe,
  };
}

export const auth = createAuthStore();
