import { createPinia } from "pinia";
import { createApp } from "vue";
import { RouterLink, RouterView } from "vue-router";

import "./assets/main.css";
import App from "./App.vue";
import { router } from "./router";
import { initTheme } from "./theme";

const app = createApp(App);

// 实验路由器需手动注册全局组件
app.component("RouterLink", RouterLink);
app.component("RouterView", RouterView);

app.use(createPinia());
app.use(router);
app.mount("#app");

// Pinia 初始化后注册 theme store（激活系统偏好监听）
initTheme();

// 注册路由器类型以获得类型化的 useRouter() / useRoute()
declare module "vue-router" {
  export interface TypesConfig {
    Router: typeof router;
  }
}
