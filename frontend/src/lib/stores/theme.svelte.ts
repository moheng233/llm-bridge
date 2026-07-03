// 主题状态 store — 亮（白天）/ 暗（黑夜）切换，Svelte 5 runes reactive。
//
// 优先级：
// 1. 用户显式选择（localStorage 'llm-bridge:theme'）—— 'light' | 'dark'
// 2. 系统偏好（prefers-color-scheme）—— 仅在用户未显式选择时
//
// 通过给 <html> 元素增删 `dark` class 激活 app.css 中的 `.dark { ... }` token 覆盖。

type ThemeMode = "light" | "dark";

const STORAGE_KEY = "llm-bridge:theme";

function readStoredTheme(): ThemeMode | null {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark") return v;
  } catch {
    // localStorage 可能在隐私模式下抛错 — 退化到系统偏好
  }
  return null;
}

function readSystemTheme(): ThemeMode {
  if (typeof window !== "undefined" && window.matchMedia) {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return "light";
}

function applyTheme(mode: ThemeMode) {
  const root = document.documentElement;
  if (mode === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
  root.style.colorScheme = mode;
}

function createThemeStore() {
  // 初始 mode：用户显式选择 > 系统偏好
  const initialExplicit = readStoredTheme();
  const initialMode: ThemeMode = initialExplicit ?? readSystemTheme();

  let mode = $state<ThemeMode>(initialMode);
  // 用户是否做过显式选择（决定是否跟随系统偏好实时变化）
  let explicit = $state<boolean>(initialExplicit !== null);

  // 应用到 DOM（使用初始值，不响应后续 state 变化 — DOM 更新由 setTheme/toggle 显式调用）
  applyTheme(initialMode);

  // 监听系统偏好变化（仅在用户未显式选择时跟随）
  if (typeof window !== "undefined" && window.matchMedia) {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", (e) => {
      if (!explicit) {
        mode = e.matches ? "dark" : "light";
        applyTheme(mode);
      }
    });
  }

  function toggle() {
    const next: ThemeMode = mode === "dark" ? "light" : "dark";
    setTheme(next);
  }

  function setTheme(next: ThemeMode) {
    mode = next;
    explicit = true;
    applyTheme(mode);
    try {
      localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // 忽略隐私模式写入失败
    }
  }

  return {
    get mode() { return mode; },
    get isDark() { return mode === "dark"; },
    toggle,
    setTheme,
  };
}

export const theme = createThemeStore();
