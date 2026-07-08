// 主题状态 Pinia store — 亮（白天）/ 暗（黑夜）切换
//
// 优先级：
// 1. 用户显式选择（localStorage 'llm-bridge:theme'）
// 2. 系统偏好（prefers-color-scheme）
//
// 通过给 <html> 增删 `dark` class 激活 CSS 变量覆盖。

type ThemeMode = "light" | "dark";
const STORAGE_KEY = "llm-bridge:theme";

function readStoredTheme(): ThemeMode | null {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark") return v;
  } catch {
    /* privacy mode */
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

export const useThemeStore = defineStore("theme", () => {
  const initialExplicit = readStoredTheme();
  const mode = ref<ThemeMode>(initialExplicit ?? readSystemTheme());
  const explicit = ref(initialExplicit !== null);

  // 应用初始主题
  applyTheme(mode.value);

  // 监听系统偏好变化
  if (typeof window !== "undefined" && window.matchMedia) {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", (e) => {
      if (!explicit.value) {
        mode.value = e.matches ? "dark" : "light";
        applyTheme(mode.value);
      }
    });
  }

  function toggle() {
    setTheme(mode.value === "dark" ? "light" : "dark");
  }

  function setTheme(next: ThemeMode) {
    mode.value = next;
    explicit.value = true;
    applyTheme(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* privacy mode */
    }
  }

  return { mode, explicit, toggle, setTheme };
});
