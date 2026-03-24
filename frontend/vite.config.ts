import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'

import path from 'path'

export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
      },
    },
  },
  resolve: {
    alias: {
      $lib: path.resolve("./src/lib"),
      $bindings: path.resolve("./src/bindings"),
    },
  },
})
