#!/usr/bin/env node
/**
 * 生成应用更新包与更新清单（在 `tauri build` 之后运行，由 tauri-build.mjs 自动调用）
 * ------------------------------------------------------------------
 * 产出（全部落在 assets-server/Assets/update/，随后由 sync-assets.mjs 上传）：
 *
 *   MC-Link-<ver>-<platform>-setup.exe          安装版更新包（NSIS 安装器，带界面）
 *   MC-Link-<ver>-<platform>-portable.exe       便携版更新包（单文件，直接替换自身）
 *   MC-Link-<ver>-<platform>-portable.zip       便携整包（exe + portable.txt，供新用户首次下载）
 *   latest.json                                 更新清单
 *
 * 为什么便携版要出两份：
 *   - 自动更新只需要**单个 exe**：客户端把它覆盖到自己的路径上即可，无需解压；
 *   - 但首次下载的用户需要一个压缩包，里面带上 `portable.txt` 才会以便携模式运行
 *     （否则数据目录会落到系统 AppData 里）。
 *
 * 清单是**带 platform + kind 的多资产格式**：同一份 latest.json 同时服务便携版与安装版，
 * 客户端按自身安装形态挑一份。多次执行（例如 x64 与 arm64 各跑一次）会把同一版本的
 * 资产**合并**进同一份清单，因此 CI 里可以分平台增量追加。
 *
 * 用法：
 *   node scripts/make-update.mjs [--platform windows-x86_64] [--mandatory]
 * 约定：
 *   - 版本号取自 package.json（tauri-build.mjs 已自增）
 *   - 更新说明取自仓库根目录的 release-notes.md（可选）
 *
 * 注意：旧版本的包不会自动清理，`Assets/update/` 会随版本累积；
 * 确认没有客户端还在下载旧版本后，可手动删除旧文件。
 */
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
  copyFileSync,
} from "node:fs";
import { join, dirname } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const TARGET_DIR = join(ROOT, "src-tauri", "target");
const OUT_DIR = join(ROOT, "assets-server", "Assets", "update");
const MANIFEST_PATH = join(OUT_DIR, "latest.json");

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const valueOf = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};

/** 与客户端 `assets::adapter::current_platform()` 的取值保持一致。 */
const PLATFORM = valueOf("--platform") || "windows-x86_64";
const MANDATORY = flag("--mandatory");

const version = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")).version;
const setupName = `MC-Link-${version}-${PLATFORM}-setup.exe`;
const portableName = `MC-Link-${version}-${PLATFORM}-portable.exe`;
const portableZipName = `MC-Link-${version}-${PLATFORM}-portable.zip`;
/** 便携整包里可执行文件的名字（仅影响首次下载的观感，自动更新按原路径覆盖）。 */
const portableInnerName = "MC Link.exe";

const releaseDir = findReleaseDir();
if (!releaseDir) {
  console.error(
    `[make-update] 未找到构建产物目录（${join(TARGET_DIR, "release")}）。请先执行 pnpm build:release`,
  );
  process.exit(1);
}

mkdirSync(OUT_DIR, { recursive: true });

const produced = [];
const installer = findInstaller(releaseDir);
const appExe = findAppExe(releaseDir);

if (installer) {
  copyFileSync(installer, join(OUT_DIR, setupName));
  produced.push({ file: setupName, kind: "installer" });
  console.log(`[make-update] 安装版更新包: ${setupName}`);
} else {
  console.warn("[make-update] 未找到 NSIS 安装器，安装版用户将只能手动下载（本次不产出 installer 资产）");
}

if (appExe) {
  copyFileSync(appExe, join(OUT_DIR, portableName));
  produced.push({ file: portableName, kind: "portable" });
  console.log(`[make-update] 便携版更新包: ${portableName}`);

  if (makePortableZip(appExe, join(OUT_DIR, portableZipName))) {
    produced.push({ file: portableZipName, kind: "portable-zip" });
    console.log(`[make-update] 便携整包: ${portableZipName}`);
  }
} else {
  console.warn("[make-update] 未找到应用可执行文件，本次不产出便携版资产");
}

if (produced.length === 0) {
  console.error("[make-update] 未产出任何更新包，清单未更新");
  process.exit(1);
}

writeManifest(produced);
pruneLocalOldVersions();
console.log(`[make-update] 清单已更新: ${MANIFEST_PATH}`);
console.log("[make-update] 下一步: pnpm sync:assets（上传到资源服务器并回收旧包）");

// ------------------------------------------------------------------
// helpers
// ------------------------------------------------------------------

/** 兼容 `target/release` 与带 `--target` 的 `target/<triple>/release`。 */
function findReleaseDir() {
  const direct = join(TARGET_DIR, "release");
  if (existsSync(direct)) return direct;
  if (!existsSync(TARGET_DIR)) return null;
  for (const name of readdirSync(TARGET_DIR)) {
    const candidate = join(TARGET_DIR, name, "release");
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

function findInstaller(dir) {
  const nsis = join(dir, "bundle", "nsis");
  if (!existsSync(nsis)) return null;

  const exes = readdirSync(nsis).filter((f) => f.toLowerCase().endsWith(".exe"));
  if (exes.length === 0) return null;

  // 不能简单取第一个：构建目录里会残留历次版本的安装包（且 rename-build 会给
  // 部分文件反复追加版本后缀）。优先选名字里带本次版本号的那个，否则取最新的一个。
  const newest = (list) =>
    list
      .map((f) => ({ f, mtime: statSync(join(nsis, f)).mtimeMs }))
      .sort((a, b) => b.mtime - a.mtime)[0].f;

  const matching = exes.filter(
    (f) => f.includes(`_${version}_`) || f.includes(`-${version}-`),
  );
  if (matching.length > 1) {
    console.warn(`[make-update] 有多个匹配版本 ${version} 的安装包，取最新的一个: ${matching.join(", ")}`);
  }

  const chosen = matching.length > 0 ? newest(matching) : newest(exes);
  if (matching.length === 0) {
    console.warn(
      `[make-update] 未找到版本 ${version} 的安装包，改用目录中最新的 ${chosen}（请确认本次是否已重新构建）`,
    );
  }
  return join(nsis, chosen);
}

function findAppExe(dir) {
  const exes = readdirSync(dir).filter((f) => f.toLowerCase().endsWith(".exe"));
  const exact = exes.find((f) => f.toLowerCase() === "mc-link.exe");
  if (exact) return join(dir, exact);
  const loose = exes.find((f) => /mc-link/i.test(f));
  return loose ? join(dir, loose) : null;
}

/** 生成便携整包：`MC Link.exe` + `portable.txt`（缺了它就不是便携模式）。 */
function makePortableZip(appExe, zipPath) {
  // 暂存目录放在系统临时目录：避免被打包上传脚本当成资源收走
  const staging = join(tmpdir(), `mc-link-portable-${process.pid}`);
  try {
    rmSync(staging, { recursive: true, force: true });
    mkdirSync(staging, { recursive: true });
    copyFileSync(appExe, join(staging, portableInnerName));
    // 便携模式的判定依据之一就是这个文件
    writeFileSync(join(staging, "portable.txt"), "");

    rmSync(zipPath, { force: true });
    // Node 标准库没有 zip 能力，借用 PowerShell 的 Compress-Archive
    const ps = `Compress-Archive -Path '${psEscape(join(staging, "*"))}' -DestinationPath '${psEscape(zipPath)}' -Force`;
    execFileSync("powershell", ["-NoProfile", "-NonInteractive", "-Command", ps], {
      stdio: "inherit",
    });
    return existsSync(zipPath);
  } catch (e) {
    console.warn(`[make-update] 生成便携整包失败，将省略该资产: ${e.message}`);
    return false;
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

function writeManifest(items) {
  const fresh = items.map(({ file, kind }) => {
    const full = join(OUT_DIR, file);
    return {
      platform: PLATFORM,
      kind,
      file,
      sha256: sha256(full),
      size: statSync(full).size,
    };
  });

  // 同版本的多次执行（x64 / arm64）合并进同一份清单
  let assets = [];
  if (existsSync(MANIFEST_PATH)) {
    try {
      const prev = JSON.parse(readFileSync(MANIFEST_PATH, "utf8"));
      if (prev.version === version && Array.isArray(prev.assets)) assets = prev.assets;
    } catch {
      // 旧清单损坏：直接以本次生成的为准
    }
  }

  for (const asset of fresh) {
    assets = assets.filter(
      (a) => !(a.platform === asset.platform && a.kind === asset.kind),
    );
    assets.push(asset);
  }
  assets.sort((a, b) =>
    a.platform === b.platform
      ? a.kind.localeCompare(b.kind)
      : a.platform.localeCompare(b.platform),
  );

  const notesPath = join(ROOT, "release-notes.md");
  const releaseNotes = existsSync(notesPath)
    ? readFileSync(notesPath, "utf8").trim()
    : "";

  const manifest = {
    manifest_version: 1,
    version,
    release_date: new Date().toISOString().slice(0, 10),
    release_notes: releaseNotes,
    mandatory: MANDATORY,
    assets,
  };

  writeFileSync(MANIFEST_PATH, JSON.stringify(manifest, null, 2) + "\n", "utf8");
}

function psEscape(p) {
  return p.replace(/'/g, "''");
}

/**
 * 本地目录只保留**本次版本**的包（含其他架构，它们可能由另一次执行生成）。
 *
 * 为什么不删其他架构：清单会把同一版本的各平台资产合并在一起，
 * 删掉 arm64 的包会让 arm64 客户端拿到指向不存在文件的清单。
 *
 * 远端由 sync-assets.mjs 另行回收（保留最新两个版本，给正在下载旧版的客户端留缓冲）。
 * 本地"只留当前版本"与远端"留两个版本"策略不同是刻意的：
 * 本地是上传源（留着只会每次重传），远端是分发点（需要一点冗余）。
 */
function pruneLocalOldVersions() {
  for (const name of readdirSync(OUT_DIR)) {
    if (name === "latest.json" || !name.startsWith("MC-Link-")) continue;
    // 形如 MC-Link-<版本>-<平台>-<类型>.<扩展名>
    if (name.includes(`-${version}-`)) continue;
    rmSync(join(OUT_DIR, name), { force: true });
    console.log(`[make-update] 已清理本地旧版本包: ${name}`);
  }
}
