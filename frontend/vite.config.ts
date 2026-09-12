import VueRouter from "vue-router/vite";

import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";
import path from "path";
import AutoImport from "unplugin-auto-import/vite";
import Components from "unplugin-vue-components/vite";
import { defineConfig } from "vite";

const uiPort = process.env.LLM_BRIDGE_UI_PORT ?? "5173";
if (!/^\d+$/.test(uiPort) || Number(uiPort) < 1 || Number(uiPort) > 65535) {
  throw new Error("LLM_BRIDGE_UI_PORT must be an integer between 1 and 65535");
}

// https://vite.dev/config/
export default defineConfig({
  clearScreen: false,
  plugins: [
    tailwindcss(),
    VueRouter({
      dts: "./node_modules/.types/typed-router.d.ts",
      experimental: {
        paramParsers: {
          dir: "src/params",
        },
      },
    }),
    vue(),
    AutoImport({
      dts: "./node_modules/.types/auto-imports.d.ts",
      imports: ["vue", "vue-router", "pinia"],
      dirs: ["./src/composables"],
      vueTemplate: true,
    }),
    Components({
      dts: "./node_modules/.types/components.d.ts",
      dirs: ["./src/components/ui", "./src/components/common", "./src/components/providers"],
      extensions: ["vue"],
      deep: true,
    }),
  ],
  server: {
    host: "127.0.0.1",
    port: Number(uiPort),
    strictPort: true,
    proxy: {
      "^/(api|auth|v1)(/.*)?$": {
        target:
          process.env.LLM_BRIDGE_DEV_BACKEND_URL ??
          `http://127.0.0.1:${process.env.LLM_BRIDGE_PORT ?? "3000"}`,
        changeOrigin: true,
        ws: true,
      },
    },
  },
  resolve: {
    alias: {
      "~": path.resolve("./src"),
      "@bindings": path.resolve("./src/bindings"),
    },
  },
});
