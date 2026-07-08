// 注册 theme store 以激活系统偏好变化监听。
// 必须在 app.use(pinia) 之后调用，因此由 main.ts 在合适时机导入并调用。
import { useThemeStore } from "./stores/theme";

export function initTheme() {
  if (typeof window !== "undefined") {
    useThemeStore(); // 触发 store 初始化，注册系统偏好监听
  }
}
