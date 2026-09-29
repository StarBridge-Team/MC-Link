#!/usr/bin/env node
/**
 * 资源构建流程（替代手工改版本号 + 手工上传）
 * ------------------------------------------------------------------
 * 1. 从 node_modules 同步 Bootstrap Icons / Poppins 字体到 assets-server/Assets
 *    （镜像 Rust 侧 prepare_from_npm 的逻辑，使本地 Assets 始终最新）
 * 2. 扫描 assets-server/Assets，重新生成 manifest.json：
 *      - version = 所有资源内容哈希（资源有变动才变，客户端据此判定是否重下）
 *      - assets  = 完整文件清单（path + size）
 * 3. 通过资源服务器新增的 POST /upload 接口上传全部文件（manifest 最后传）
 *
 * 环境变量 / 参数：
 *   ASSET_SERVER_URL      目标资源服务器（默认 http://localhost:54789）
 *   ASSET_UPLOAD_TOKEN    上传令牌（与服务器 upload_token / ASSET_UPLOAD_TOKEN 一致）
 *   ASSET_UPLOAD_REQUIRED=1 上传失败时以非零码退出（CI 中强制校验）
 *   SKIP_ASSET_UPLOAD=1 | --no-upload  跳过上传（仅重新生成 manifest）
 *   --prod               使用生产资源服务器地址
 */
import {
  readdirSync,
  readFileSync,
  writeFileSync,
  statSync,
  existsSync,
  mkdirSync,
  cpSync,
} from "node:fs";
import { join, relative, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const ASSETS_DIR = join(ROOT, "assets-server", "Assets");

const PROD_URL = "https://mclinkassets.xigo.top:54789";
const SERVER_URL =
  process.env.ASSET_SERVER_URL ||
  (process.argv.includes("--prod") ? PROD_URL : "http://localhost:54789");
const TOKEN = process.env.ASSET_UPLOAD_TOKEN || "";
const SKIP =
  process.env.SKIP_ASSET_UPLOAD === "1" || process.argv.includes("--no-upload");
const REQUIRED = process.env.ASSET_UPLOAD_REQUIRED === "1";

// 1) 从 npm 同步字体/图标，确保 Assets 是最新的
syncFromNpm();

// 2) 扫描并生成 manifest
const files = [];
collectFiles(ASSETS_DIR, ASSETS_DIR, files);
files.sort((a, b) => a.rel.localeCompare(b.rel));

const version = computeVersion(files);
const manifest = {
  version,
  server: SERVER_URL,
  assets: files.map((f) => ({ path: f.rel, size: f.size })),
};
writeFileSync(
  join(ASSETS_DIR, "manifest.json"),
  JSON.stringify(manifest, null, 2) + "\n",
  "utf8",
);
console.log(
  `[sync-assets] manifest version=${version}，${files.length} 个文件`,
);

if (SKIP) {
  console.log("[sync-assets] 已跳过上传（SKIP_ASSET_UPLOAD=1 或 --no-upload）");
  process.exit(0);
}

// 3) 上传：资源文件先传，manifest 最后传，避免客户端拉到半更新状态
const ordered = [
  ...files.filter((f) => f.rel !== "manifest.json"),
  { rel: "manifest.json", full: join(ASSETS_DIR, "manifest.json") },
];

let ok = 0;
let fail = 0;
for (const f of ordered) {
  const buf = readFileSync(f.full);
  const uploaded = await uploadFile(f.rel, buf);
  if (uploaded) ok++;
  else fail++;
}

if (fail > 0) {
  console.error(`[sync-assets] 上传完成，但有 ${fail} 个失败`);
  if (REQUIRED) process.exit(1);
  process.exit(0);
}
console.log(`[sync-assets] 上传完成：${ok}/${ordered.length}`);

// ------------------------------------------------------------------
// helpers
// ------------------------------------------------------------------
function collectFiles(dir, base, out) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const st = statSync(full);
    if (st.isDirectory()) {
      collectFiles(full, base, out);
    } else {
      out.push({
        rel: relative(base, full).split("\\").join("/"),
        full,
        size: st.size,
      });
    }
  }
}

function computeVersion(files) {
  const h = createHash("sha256");
  for (const f of files) {
    const content = readFileSync(f.full);
    const fileHash = createHash("sha256").update(content).digest("hex");
    h.update(`${f.rel}\u0000${fileHash}\u0000`);
  }
  return h.digest("hex").slice(0, 16);
}

function uploadFile(rel, buf) {
  return new Promise((resolve) => {
    const url = new URL(
      `${SERVER_URL}/upload?path=${encodeURIComponent(rel)}` +
        (TOKEN ? `&token=${encodeURIComponent(TOKEN)}` : ""),
    );
    const lib = url.protocol === "https:" ? httpsRequest : httpRequest;
    const req = lib(
      {
        method: "POST",
        hostname: url.hostname,
        port: url.port,
        path: url.pathname + url.search,
        headers: {
          "Content-Type": "application/octet-stream",
          "Content-Length": buf.length,
        },
      },
      (res) => {
        let body = "";
        res.on("data", (d) => (body += d));
        res.on("end", () => {
          if (res.statusCode >= 200 && res.statusCode < 300) {
            console.log(`  ✓ ${rel} (${buf.length}B)`);
            resolve(true);
          } else {
            console.error(`  ✗ ${rel} -> HTTP ${res.statusCode} ${body}`);
            resolve(false);
          }
        });
      },
    );
    req.on("error", (e) => {
      console.error(`  ✗ ${rel} -> ${e.message}`);
      resolve(false);
    });
    req.write(buf);
    req.end();
  });
}

// 镜像 Rust 侧 prepare_from_npm：从 npm 包同步字体/图标到 Assets/
function syncFromNpm() {
  const npm = join(ROOT, "node_modules");
  if (!existsSync(npm)) {
    console.log("[sync-assets] 无 node_modules，跳过 npm 同步");
    return;
  }

  // Bootstrap Icons
  const biSrc = join(npm, "bootstrap-icons", "font");
  if (existsSync(biSrc)) {
    const dst = join(ASSETS_DIR, "bootstrap-icons");
    mkdirSync(dst, { recursive: true });
    for (const ext of ["woff2", "woff"]) {
      const s = join(biSrc, "fonts", `bootstrap-icons.${ext}`);
      if (existsSync(s)) cpSync(s, join(dst, `bootstrap-icons.${ext}`));
    }
    const css = readFileSync(join(biSrc, "bootstrap-icons.css"), "utf8")
      .replace(
        "./fonts/bootstrap-icons.woff2?e34853135f9e39acf64315236852cd5a",
        "./bootstrap-icons.woff2",
      )
      .replace(
        "./fonts/bootstrap-icons.woff?e34853135f9e39acf64315236852cd5a",
        "./bootstrap-icons.woff",
      );
    writeFileSync(join(dst, "bootstrap-icons.css"), css);
  }

  // Poppins：读取 latin-*.css，复制 woff2/woff 并合并为 poppins.css
  const popSrc = join(npm, "@fontsource", "poppins");
  if (existsSync(popSrc)) {
    const dst = join(ASSETS_DIR, "fonts");
    mkdirSync(dst, { recursive: true });
    const weights = ["300", "400", "500", "600", "700"];
    let combined = "";
    for (const w of weights) {
      const cssPath = join(popSrc, `latin-${w}.css`);
      if (!existsSync(cssPath)) continue;
      const css = readFileSync(cssPath, "utf8");
      let rewritten = css;
      let idx = 0;
      while (true) {
        const pos = css.slice(idx).indexOf("url(");
        if (pos < 0) break;
        const start = idx + pos + 4;
        const end = css.indexOf(")", start);
        if (end < 0) break;
        const raw = css.slice(start, end);
        const url = raw.replace(/['"]/g, "");
        if (url.startsWith("./files/")) {
          const filename = url.slice("./files/".length);
          const srcFile = join(popSrc, "files", filename);
          if (existsSync(srcFile)) cpSync(srcFile, join(dst, filename));
          rewritten = rewritten.split(url).join(`./${filename}`);
        }
        idx = end + 1;
      }
      combined += rewritten + "\n";
    }
    if (combined) writeFileSync(join(dst, "poppins.css"), combined);
  }

  console.log("[sync-assets] 已从 node_modules 同步字体/图标到 Assets/");
}
