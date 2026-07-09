import VueRouter from "vue-router/vite";

import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";
import path from "path";
import AutoImport from "unplugin-auto-import/vite";
import Components from "unplugin-vue-components/vite";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
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
    proxy: {
      "^/(api|auth|v1)(/.*)?$": {
        target: "http://127.0.0.1:3000",
        changeOrigin: true,
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
