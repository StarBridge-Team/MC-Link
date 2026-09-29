import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "node:path";
import fs from "node:fs";

// 页面片段独立构建配置
// - 入口：src/pages/*.html（每个文件一个页面片段）
// - 输出：assets-server/Pages/*.html（扁平结构）
// - 构建后自动生成 manifest.json 供客户端 get_page_manifest 拉取
const PAGES_SRC_DIR = path.resolve(__dirname, "src/pages");
const PAGES_OUT_DIR = path.resolve(__dirname, "assets-server/Pages");

function collectInputs(): Record<string, string> {
  if (!fs.existsSync(PAGES_SRC_DIR)) return {};
  const inputs: Record<string, string> = {};
  for (const file of fs.readdirSync(PAGES_SRC_DIR)) {
    if (!file.endsWith(".html")) continue;
    const name = path.basename(file, ".html");
    inputs[name] = path.join(PAGES_SRC_DIR, file);
  }
  return inputs;
}

// vite 对 HTML 入口会保留源文件相对路径结构（如 src/pages/welcome.html）
// 此插件在写盘后把嵌套的 .html 移动到 outDir 根，并清理空目录
function flattenAndManifest() {
  return {
    name: "flatten-and-manifest",
    closeBundle() {
      if (!fs.existsSync(PAGES_OUT_DIR)) return;

      // 递归收集所有 .html（排除 assets/ 子目录）
      const htmlFiles: string[] = [];
      const walk = (dir: string) => {
        for (const entry of fs.readdirSync(dir)) {
          const full = path.join(dir, entry);
          const rel = path.relative(PAGES_OUT_DIR, full);
          if (fs.statSync(full).isDirectory()) {
            if (entry !== "assets") walk(full);
          } else if (entry.endsWith(".html")) {
            htmlFiles.push(full);
          }
          // 忽略非 html 文件（如已生成的 manifest.json）
        }
      };
      walk(PAGES_OUT_DIR);

      // 移动到 outDir 根
      for (const src of htmlFiles) {
        const dest = path.join(PAGES_OUT_DIR, path.basename(src));
        if (src !== dest) {
          fs.renameSync(src, dest);
        }
      }

      // 清理空目录（排除 assets）
      for (const entry of fs.readdirSync(PAGES_OUT_DIR)) {
        const full = path.join(PAGES_OUT_DIR, entry);
        if (fs.statSync(full).isDirectory() && entry !== "assets") {
          fs.rmSync(full, { recursive: true, force: true });
        }
      }

      // 生成 manifest
      const pages: Array<{ name: string; path: string; sha256: string | null }> = [];
      for (const file of fs.readdirSync(PAGES_OUT_DIR)) {
        if (!file.endsWith(".html")) continue;
        const name = path.basename(file, ".html");
        pages.push({ name, path: file, sha256: null });
      }
      pages.sort((a, b) => a.name.localeCompare(b.name));
      fs.writeFileSync(
        path.join(PAGES_OUT_DIR, "manifest.json"),
        JSON.stringify({ base_url: null, pages }, null, 2),
        "utf-8"
      );
      console.log(`[pages] 生成 manifest.json，共 ${pages.length} 个页面`);
    },
  };
}

export default defineConfig({
  plugins: [vue(), flattenAndManifest()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  clearScreen: false,
  // 禁用 publicDir，避免 public/ 下文件污染 Pages 输出
  publicDir: false,
  // 资源全部内联，避免片段插入主应用后相对路径资源失效
  build: {
    outDir: PAGES_OUT_DIR,
    emptyOutDir: true,
    assetsInlineLimit: 100 * 1024 * 1024,
    cssCodeSplit: false,
    rollupOptions: {
      input: collectInputs(),
      output: {
        entryFileNames: "assets/[name].js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
});
