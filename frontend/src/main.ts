import { mount } from "svelte";
import "./app.css";
// 导入 theme store 以注册系统偏好变化监听（DOM class 由 index.html 内联脚本预先应用）
import "./lib/stores/theme.svelte";
import App from "./App.svelte";

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
