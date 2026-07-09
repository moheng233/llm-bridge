import { createWebHashHistory } from "vue-router";
import { resolver, handleHotUpdate } from "vue-router/auto-resolver";
import { experimental_createRouter as createRouter } from "vue-router/experimental";

export const router = createRouter({
  history: createWebHashHistory(),
  resolver,
});

if (import.meta.hot) {
  handleHotUpdate(router);
}
