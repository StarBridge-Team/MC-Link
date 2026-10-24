import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    vue({
      template: {
        compilerOptions: {
          // @m3e/web 是 Web Components，标签以 m3e- 开头，放行让 Vue 当作原生元素处理
          isCustomElement: (tag) => tag.startsWith("m3e-"),
        },
      },
    }),
  ],
  
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // 多页入口：主窗口与房间窗口各自一份 HTML。
  // 房间窗口不加载导航壳与主路由，冷启动更快；共用同一份 chunk 与组件库注册。
  build: {
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        room: path.resolve(__dirname, "room.html"),
      },
    },
  },

  clearScreen: false,

  assetsInclude: ["**/*.woff", "**/*.woff2"],
  
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
