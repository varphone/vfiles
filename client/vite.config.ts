import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  css: {
    preprocessorOptions: {
      scss: {
        quietDeps: true,
      },
    },
  },
  server: {
    host: '0.0.0.0',
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:3000',
        changeOrigin: true,
        secure: false,
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        // Tabler 图标按组件拆成几十个 0.5 KB 的小 chunk，HTTP/2 下仍是几十次
        // 请求；合并成一个按需加载的图标 chunk。
        manualChunks(id) {
          if (id.includes("@tabler/icons-vue")) return "icons";
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: false,
  },
});
