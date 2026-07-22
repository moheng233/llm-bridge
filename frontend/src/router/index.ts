import { createWebHashHistory } from "vue-router";
import { resolver, handleHotUpdate } from "vue-router/auto-resolver";
import { experimental_createRouter as createRouter } from "vue-router/experimental";

export const router = createRouter({
  history: createWebHashHistory(),
  resolver,
});

// 根路径统一跳转到用量仪表盘
router.beforeEach((to) => {
  if (to.path === "/") return { path: "/dashboard", replace: true };
});

if (import.meta.hot) {
  handleHotUpdate(router);
}
