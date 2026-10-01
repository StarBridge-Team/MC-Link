#!/usr/bin/env node
/**
 * 资源构建流程（替代手工改版本号 + 手工上传）
 * ------------------------------------------------------------------
 * 1. 从 node_modules 同步 Bootstrap Icons / Poppins 字体到 assets-server/Assets
 *    （镜像 Rust 侧 prepare_from_npm 的逻辑，使本地 Assets 始终最新）
 * 2. 准备适配器包：下载上游发布包到 Assets/adapter/，计算 SHA256 并生成
 *    adapter/manifest.json（客户端据此校验安装包，未通过校验不会安装）。
 *    该目录独立于 Assets 版本聚合，避免适配器升级触发客户端重下全部资源。
 * 3. 扫描 assets-server/Assets，重新生成 manifest.json：
 *      - version = 所有资源内容哈希（不含 adapter/ 与 update/，资源有变动才变）
 *      - assets  = 完整文件清单（path + size）
 * 4. 通过资源服务器新增的 POST /upload 接口上传全部文件（各级 manifest 最后传）
 *
 * 目录特例（都在 version 聚合之外，避免客户端因为发版而重下字体/图标）：
 *   - adapter/ 由 prepareAdapters() 生成，含第三方适配器包与其校验清单
 *   - update/  由 scripts/make-update.mjs 生成，含本应用的更新包与 latest.json
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
const ADAPTER_DIR = join(ASSETS_DIR, "adapter");

// 适配器包来源与版本：升级时改这里，再执行 pnpm sync:assets 发布新清单
const ADAPTER_PLATFORM = "windows-x86_64";
const ADAPTER_VERSION = "0.4.2";
const ADAPTER_FILE = `terracotta-${ADAPTER_VERSION}-${ADAPTER_PLATFORM}-pkg.tar.gz`;
const ADAPTER_URL = `https://gitee.com/burningtnt/Terracotta/releases/download/v${ADAPTER_VERSION}/${ADAPTER_FILE}`;

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

// 2) 准备适配器包与校验清单（须在扫描前完成，否则不会被收集与上传）
await prepareAdapters();

// 3) 扫描并生成 manifest；adapter/ 独立维护，不参与 Assets 版本聚合
const allFiles = [];
collectFiles(ASSETS_DIR, ASSETS_DIR, allFiles);
allFiles.sort((a, b) => a.rel.localeCompare(b.rel));

// 适配器包与更新包都**独立于 Assets 版本聚合**：
// 否则每次发版（或升级适配器）都会让所有客户端重下全部字体与图标。
const isAdapter = (f) => f.rel.startsWith("adapter/");
const isUpdate = (f) => f.rel.startsWith("update/");
// manifest.json 必须排除在资源集合之外，有两个原因：
//   1. 它会被算进版本哈希，而版本号就写在它自己里面 → 版本永远无法收敛，
//      每次 sync 都会生成新版本号，逼所有客户端全量重下；
//   2. 客户端无法自校验它（哈希不可能包含自身），列进去只会引入鸡生蛋问题。
// 它由客户端自己按远程清单重建本地副本。
const isManifest = (f) => f.rel === "manifest.json";
const adapterFiles = allFiles.filter(isAdapter);
const updateFiles = allFiles.filter(isUpdate);
const files = allFiles.filter((f) => !isAdapter(f) && !isUpdate(f) && !isManifest(f));

const UPDATE_MANIFEST = join(ASSETS_DIR, "update", "latest.json");
if (updateFiles.length > 0 && !existsSync(UPDATE_MANIFEST)) {
  console.warn(
    "[sync-assets] update/ 下有更新包但没有 latest.json：请先执行 node scripts/make-update.mjs",
  );
}

// 每个文件先算出自己的 sha256：客户端据此逐个校验本地缓存，
// 只重下缺失或损坏的那几个，而不是整包重来。
for (const f of files) {
  f.sha256 = createHash("sha256").update(readFileSync(f.full)).digest("hex");
}
const version = computeVersion(files);
const manifest = {
  version,
  server: SERVER_URL,
  assets: files.map((f) => ({ path: f.rel, size: f.size, sha256: f.sha256 })),
};
writeFileSync(
  join(ASSETS_DIR, "manifest.json"),
  JSON.stringify(manifest, null, 2) + "\n",
  "utf8",
);
console.log(
  `[sync-assets] manifest version=${version}，${files.length} 个文件（含 sha256）`,
);

if (SKIP) {
  console.log("[sync-assets] 已跳过上传（SKIP_ASSET_UPLOAD=1 或 --no-upload）");
  process.exit(0);
}

// 4) 上传：内容文件先传，各级 manifest 最后传，避免客户端拉到半更新状态
const ordered = [
  ...files,
  // 更新包必须先于 update/latest.json 可见，否则客户端会拿到指向不存在文件的清单
  ...updateFiles.filter((f) => f.rel !== "update/latest.json"),
  // 适配器包必须先于其清单可见，否则客户端会拿到指向不存在文件的清单
  ...adapterFiles.filter((f) => f.rel !== "adapter/manifest.json"),
  { rel: "adapter/manifest.json", full: join(ADAPTER_DIR, "manifest.json") },
  ...(existsSync(UPDATE_MANIFEST)
    ? [{ rel: "update/latest.json", full: UPDATE_MANIFEST }]
    : []),
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

// 5) 回收服务器上的旧版本更新包
//    没有这一步，每个版本会在服务器上留约 30MB 且永远无法回收
//   （服务器原先只有上传接口，没有任何删除能力）。
if (updateFiles.length > 0) {
  await pruneRemoteUpdates();
}

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

// 版本 = 全部资源「路径 + 内容哈希」的聚合。
// 由于 manifest.json 已被排除，同一批文件内容不变则版本不变（幂等）。
function computeVersion(files) {
  const h = createHash("sha256");
  for (const f of files) {
    h.update(`${f.rel}\u0000${f.sha256}\u0000`);
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

// 下载上游适配器发布包，计算 SHA256 并生成校验清单（客户端据此校验后才安装）
async function prepareAdapters() {
  mkdirSync(ADAPTER_DIR, { recursive: true });
  const pkgPath = join(ADAPTER_DIR, ADAPTER_FILE);
  const manifestPath = join(ADAPTER_DIR, "manifest.json");

  // 包已存在且与清单记录的哈希一致 → 跳过下载（避免每次 sync 都重下整个包）
  if (existsSync(pkgPath) && existsSync(manifestPath)) {
    try {
      const prev = JSON.parse(readFileSync(manifestPath, "utf8"));
      const rec = (prev.adapters || []).find((a) => a.platform === ADAPTER_PLATFORM);
      if (rec && rec.version === ADAPTER_VERSION) {
        const actual = createHash("sha256").update(readFileSync(pkgPath)).digest("hex");
        if (actual === rec.sha256) {
          console.log(`[sync-assets] 适配器包已是最新（${ADAPTER_FILE}），跳过下载`);
          return;
        }
      }
    } catch {
      // 清单损坏：落到下面重新生成
    }
  }

  console.log(`[sync-assets] 下载适配器包 ${ADAPTER_URL} ...`);
  const buf = await httpGetBuffer(ADAPTER_URL);
  const sha256 = createHash("sha256").update(buf).digest("hex");
  writeFileSync(pkgPath, buf);

  const manifest = {
    version: "1",
    adapters: [
      {
        platform: ADAPTER_PLATFORM,
        version: ADAPTER_VERSION,
        file: ADAPTER_FILE,
        sha256,
        size: buf.length,
        urls: [ADAPTER_URL],
      },
    ],
  };
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n", "utf8");
  console.log(
    `[sync-assets] 适配器清单已生成：${ADAPTER_FILE}（${buf.length}B，sha256=${sha256.slice(0, 16)}…）`,
  );
}

/**
 * 回收服务器上的旧版本更新包。
 *
 * 保留策略：最新**两个**版本。最新的必须在（清单指向它）；前一个留作缓冲——
 * 正在下载旧版本的客户端不会因为文件突然消失而中断。更早的一律删除。
 *
 * 依赖资源服务器的两个端点：`GET /update/list` 与 `POST /delete`。
 * 任一步失败都只打印警告：回收是维护动作，不该让发布流程失败。
 */
async function pruneRemoteUpdates() {
  let listing;
  try {
    listing = await httpGetJson(`${SERVER_URL}/update/list`);
  } catch (e) {
    console.warn(`[sync-assets] 无法获取远端更新目录清单，跳过旧包回收: ${e.message}`);
    return;
  }

  if (!listing || !Array.isArray(listing.files)) {
    console.warn("[sync-assets] 远端更新目录清单格式异常，跳过旧包回收");
    return;
  }

  const byVersion = new Map();
  for (const name of listing.files) {
    if (name === "latest.json") continue; // 清单本身永远保留
    const version = parsePackageVersion(name);
    if (!version) continue; // 认不出的文件名一律不动
    if (!byVersion.has(version)) byVersion.set(version, []);
    byVersion.get(version).push(name);
  }

  const versions = [...byVersion.keys()].sort(compareVersions).reverse();
  const keep = new Set(versions.slice(0, 2));
  const doomed = versions.slice(2).flatMap((v) => byVersion.get(v));
  if (doomed.length === 0) return;

  console.log(
    `[sync-assets] 回收旧版本更新包（保留 ${[...keep].join(", ")}）：${doomed.length} 个文件`,
  );
  for (const name of doomed) {
    try {
      await postDelete(`update/${name}`);
      console.log(`  ✗ 已回收 ${name}`);
    } catch (e) {
      console.warn(`  ! 回收失败 ${name}: ${e.message}`);
    }
  }
}

/** 从更新包文件名解析版本号：`MC-Link-0.4.1-windows-x86_64-setup.exe` → `0.4.1`。 */
function parsePackageVersion(name) {
  const rest = name.startsWith("MC-Link-") ? name.slice("MC-Link-".length) : null;
  if (!rest) return null;
  const version = rest.split("-")[0];
  const parts = version.split(".");
  if (parts.length !== 3) return null;
  if (!parts.every((p) => /^\d+$/.test(p))) return null;
  return version;
}

/** 版本号数字序比较（用于排序，不追求完整语义化版本支持）。 */
function compareVersions(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i += 1) {
    if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) - (pb[i] || 0);
  }
  return 0;
}

/** GET 一个 JSON 端点。 */
function httpGetJson(url) {
  return new Promise((resolve, reject) => {
    const u = new URL(url);
    const lib = u.protocol === "https:" ? httpsRequest : httpRequest;
    const req = lib(
      {
        method: "GET",
        hostname: u.hostname,
        port: u.port,
        path: u.pathname + u.search,
        headers: { "User-Agent": "mc-link-sync-assets" },
      },
      (res) => {
        let body = "";
        res.on("data", (d) => (body += d));
        res.on("end", () => {
          if (res.statusCode < 200 || res.statusCode >= 300) {
            reject(new Error(`HTTP ${res.statusCode}`));
            return;
          }
          try {
            resolve(JSON.parse(body));
          } catch (e) {
            reject(new Error(`响应不是合法 JSON: ${e.message}`));
          }
        });
      },
    );
    req.on("error", reject);
    req.end();
  });
}

/** 请求服务器删除一个已上传的文件；404 视为"已经清理过"。 */
function postDelete(rel) {
  return new Promise((resolve, reject) => {
    const url = new URL(
      `${SERVER_URL}/delete?path=${encodeURIComponent(rel)}` +
        (TOKEN ? `&token=${encodeURIComponent(TOKEN)}` : ""),
    );
    const lib = url.protocol === "https:" ? httpsRequest : httpRequest;
    const req = lib(
      {
        method: "POST",
        hostname: url.hostname,
        port: url.port,
        path: url.pathname + url.search,
        headers: { "Content-Length": 0 },
      },
      (res) => {
        res.resume();
        res.on("end", () => {
          if ((res.statusCode >= 200 && res.statusCode < 300) || res.statusCode === 404) {
            resolve(true);
          } else {
            reject(new Error(`HTTP ${res.statusCode}`));
          }
        });
      },
    );
    req.on("error", reject);
    req.end();
  });
}

// 跟随重定向的 GET，返回完整响应体（gitee 发布下载会 302 到 CDN）
function httpGetBuffer(url, redirects = 0) {
  return new Promise((resolve, reject) => {
    if (redirects > 5) {
      reject(new Error("重定向次数过多"));
      return;
    }
    const u = new URL(url);
    const lib = u.protocol === "https:" ? httpsRequest : httpRequest;
    const req = lib(
      {
        method: "GET",
        hostname: u.hostname,
        port: u.port,
        path: u.pathname + u.search,
        headers: { "User-Agent": "mc-link-sync-assets" },
      },
      (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          res.resume();
          resolve(
            httpGetBuffer(new URL(res.headers.location, url).toString(), redirects + 1),
          );
          return;
        }
        if (res.statusCode !== 200) {
          res.resume();
          reject(new Error(`HTTP ${res.statusCode}`));
          return;
        }
        const chunks = [];
        res.on("data", (d) => chunks.push(d));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      },
    );
    req.on("error", reject);
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
