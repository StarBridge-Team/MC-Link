#!/usr/bin/env node
/**
 * 生成应用更新包与更新清单（在 `tauri build` 之后运行，由 tauri-build.mjs 自动调用）
 * ------------------------------------------------------------------
 * 产出（全部落在 assets-server/Assets/update/，随后由 sync-assets.mjs 上传）：
 *
 *   MC-Link-<ver>-<platform>-setup.exe       安装版更新包（NSIS 安装器）
 *   MC-Link-<ver>-<platform>-portable.exe    便携版更新包（单文件，直接替换自身）
 *   MC-Link-<ver>-<platform>-portable.zip    便携整包（exe + portable.txt，供新用户首次下载）
 *   <...>.AppImage / <...>.app.tar.gz        Linux / macOS 产物（仅在该平台构建时存在）
 *   latest.json                              我们的更新清单（版本判定以此为准）
 *   tauri.json                               Tauri 官方更新插件的清单（需签名私钥）
 *
 * 插件清单的格式与签名机制由插件自己规定（`platforms` + minisign `.sig`），
 * 无法与我们的 latest.json 合并；但两者由同一次发布生成，版本与说明不会漂移。
 * **没有 `TAURI_SIGNING_PRIVATE_KEY` 时不会生成 tauri.json**——插件的签名校验不可关闭，
 * 生成一份通不过校验的清单只会让人误以为已经配好；此时 Windows 安装版走自研兜底。
 *
 * # 两份清单必须同步（否则用户先看到一次失败）
 *
 * 界面读的是 `latest.json`，官方插件读的是 `tauri.json`。两者一旦不同步，
 * 用户就会先看到"有新版本"，点下去却失败：
 *
 * - **没有签名私钥** → 不生成 `tauri.json`。此时 `latest.json` 会带上
 *   `official_plugin: false`，明确告诉界面"本次不走官方插件"，
 *   免得让用户点一次注定失败的"一键更新"。
 * - **`tauri.json` 的版本必须与 `latest.json` 一致**：残留的旧版本清单会让插件
 *   报"未发现可用更新"（它比界面看到的版本旧），于是界面提示有更新、
 *   插件却说没有 —— 这正是本次修掉的症状。发现版本对不上时直接删掉那份清单。
 * - **平台必须被插件清单覆盖**：latest.json 里出现了某个平台、而 tauri.json 里
 *   没有它时（例：x64 签了、arm64 没签），那个平台的官方插件路径不可用，
 *   `official_plugin` 会置为 false。
 *
 * # 平台标识：两套命名，必须分清
 *
 * | 清单 | 平台键 | 架构命名 |
 * |---|---|---|
 * | `latest.json` | `windows-x86_64` | **客户端的** `current_platform()`：`aarch64 → arm64`、`arm → armv7` |
 * | `tauri.json` | `windows-x86_64` | **插件的** `updater_arch()`：Rust 架构名 `aarch64` / `armv7` / `x86_64` |
 *
 * 命令行传的 `--platform` 按**客户端**那套归一化（`aarch64 → arm64`），
 * 写进 latest.json 与产物文件名；写进 tauri.json 时再转回插件那套。
 * 两边对不上会让 ARM 用户收不到更新（客户端按 `platform` 精确匹配资产）。
 *
 * # 两条落地路径，两份清单
 *
 * | 场景 | 谁落地 | 读哪份清单 |
 * |---|---|---|
 * | Windows 便携版 | 本仓库自研（替换 exe） | latest.json |
 * | Windows 安装版 / Linux / macOS | Tauri 官方更新插件 | tauri.json |
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
 *   --platform 的架构名按**客户端**的命名写：`x86_64` / `aarch64`（= arm64） / `arm`（= armv7）。
 *   写成 `--platform windows-aarch64` 也会被自动归一化成 `windows-arm64`，
 *   与客户端 `current_platform()` 完全一致（详见上面"平台标识"一节）。
 * 环境变量：
 *   TAURI_SIGNING_PRIVATE_KEY  插件签名私钥（不设则跳过 tauri.json）
 *   GITHUB_RELEASE_BASE        插件清单下载地址前缀（默认 GitHub 发行版直链）
 *   （latest.json 不带绝对地址：客户端按自身资源服务器地址推导 `update/<file>`）
 * 约定：
 *   - 版本号取自 package.json（tauri-build.mjs 已自增）
 *   - 更新说明取自仓库根目录的 release-notes.md（可选）
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

import { signingEnv as loadSigningEnv } from "./signer-env.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const TARGET_DIR = join(ROOT, "src-tauri", "target");
const OUT_DIR = join(ROOT, "assets-server", "Assets", "update");
const MANIFEST_PATH = join(OUT_DIR, "latest.json");
const PLUGIN_MANIFEST_PATH = join(OUT_DIR, "tauri.json");

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const valueOf = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};

/**
 * 命令行传入的平台标识（未归一化）。
 *
 * 历史上这里写的是 `--platform windows-aarch64`，而客户端
 * `src-tauri/src/assets/adapter.rs` 的 `current_platform()` 用的是上游产物的架构命名
 * （`aarch64 → arm64`、`arm → armv7`）。客户端按 `platform` **精确匹配**资产，
 * 两边对不上 → ARM 用户在清单里永远找不到自己的平台，收不到更新。
 */
const PLATFORM_RAW = valueOf("--platform") || "windows-x86_64";
/** 归一化后的平台标识：与客户端 `current_platform()` 同一套命名。 */
const PLATFORM = normalizePlatform(PLATFORM_RAW);
if (PLATFORM !== PLATFORM_RAW) {
  console.log(
    `[make-update] 平台标识已归一化: ${PLATFORM_RAW} → ${PLATFORM}` +
      "（与客户端 current_platform() 一致，否则该平台匹配不到更新）",
  );
}
const MANDATORY = flag("--mandatory");
/** GitHub 仓库（`owner/repo`），与客户端 `update/fetch.rs` 的常量一致。 */
const GITHUB_REPO = "StarBridge-Team/MC-Link";

/**
 * 把平台标识归一化成**客户端使用的形式**（`aarch64 → arm64`、`arm → armv7`）。
 *
 * 纯函数：命令行怎么写都能落到客户端认得的键上，避免"脚本写一种、客户端认另一种"。
 */
function normalizePlatform(platform) {
  const parts = String(platform).split("-");
  const os = parts.shift() || "";
  let arch = parts.join("-");
  if (arch === "aarch64") arch = "arm64";
  else if (arch === "arm") arch = "armv7";
  return [os, arch].filter(Boolean).join("-");
}

/**
 * 插件清单（tauri.json）的下载地址：指向 GitHub 发行版直链。
 *
 * 与客户端更新优先级一致（GitHub Release 第一）。连不上 GitHub 的用户会
 * 让插件下载失败，`commands.rs` 随即回退自研路径——那里有代理择优与
 * 资源服务器兜底（多镜像、断点续传、SHA256 校验），失败不降级。
 * 可用环境变量覆盖（本地演练或换仓库时用）。
 */
const RELEASE_BASE = (
  process.env.GITHUB_RELEASE_BASE || `https://github.com/${GITHUB_REPO}/releases/download`
).replace(/\/+$/, "");

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

// Linux / macOS 的产物：只有在对应平台上构建时才存在
for (const extra of collectPlatformArtifacts(releaseDir)) {
  copyFileSync(extra.source, join(OUT_DIR, extra.file));
  produced.push({ file: extra.file, kind: extra.kind });
  console.log(`[make-update] ${extra.kind} 产物: ${extra.file}`);
}

if (produced.length === 0) {
  console.error("[make-update] 未产出任何更新包，清单未更新");
  process.exit(1);
}

// 合并同版本的资产：多次执行（x64 / arm64 各跑一次）会按平台增量追加。
const merged = mergeAssets(describeAssets(produced));

// 插件清单先写：latest.json 里要带上"本次能不能走官方插件"的标记。
writePluginManifest(produced);
const plugin = pluginAvailability(merged);

// 插件装不上的资产（只有插件能装的那些）必须从清单里剔掉：
// 留着只会让界面给出一次注定失败的"一键更新"。
const assets = dropUninstallableAssets(merged, plugin);

writeManifest(assets, plugin);
pruneLocalOldVersions();
console.log(`[make-update] 清单已更新: ${MANIFEST_PATH}`);
if (plugin.available) {
  console.log(
    "[make-update] 官方插件清单可用：tauri.json 与 latest.json 版本一致" +
      `（平台：${plugin.coveredPlatforms.join(", ")}）`,
  );
} else {
  console.warn(
    `[make-update] 官方插件清单不可用：${plugin.reason}；` +
      "latest.json 已标记 official_plugin=false（界面据此不要提供注定失败的“一键更新”）",
  );
}
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

/**
 * 收集 Linux / macOS 的产物（只有在该平台上构建时才存在）。
 *
 * 插件能自动安装的形态：Linux 仅 AppImage（deb/rpm 不支持），macOS 为 `.app.tar.gz`。
 * Flatpak 与 deb 只作为**手动下载**项（见各自的注释）。
 */
function collectPlatformArtifacts(dir) {
  const found = [];
  const arch = PLATFORM.split("-").slice(1).join("-") || "x86_64";

  const appimageDir = join(dir, "bundle", "appimage");
  if (existsSync(appimageDir)) {
    const hit = readdirSync(appimageDir).find((f) => f.endsWith(".AppImage"));
    if (hit) {
      found.push({
        source: join(appimageDir, hit),
        file: `MC-Link-${version}-linux-${arch}-appimage.AppImage`,
        kind: "appimage",
      });
    }
  }

  // deb / rpm：Tauri 直接产出，**只作手动下载**。
  // 它们由系统包管理器安装，客户端既不能自动替换（`/usr` 归 dpkg 管），
  // 官方更新插件也不支持 —— 所以不进 auto_kinds，只让用户能拿到文件。
  for (const [bundleDir, ext, kind] of [
    ["deb", ".deb", "deb"],
    ["rpm", ".rpm", "rpm"],
  ]) {
    const dirPath = join(dir, "bundle", bundleDir);
    if (!existsSync(dirPath)) continue;
    const hit = readdirSync(dirPath).find((f) => f.endsWith(ext));
    if (!hit) continue;
    found.push({
      source: join(dirPath, hit),
      file: `MC-Link-${version}-linux-${arch}-${kind}${ext}`,
      kind,
    });
  }

  const macosDir = join(dir, "bundle", "macos");
  if (existsSync(macosDir)) {
    const hit = readdirSync(macosDir).find((f) => f.endsWith(".app.tar.gz"));
    if (hit) {
      found.push({
        source: join(macosDir, hit),
        file: `MC-Link-${version}-macos-${arch}-app.tar.gz`,
        kind: "macos-app",
      });
    }
  }

  // Flatpak：由 flatpak-builder 产出（Tauri 的 bundle targets 不含 flatpak），
  // 通常落在 `build-aux/flatpak/` 或 `target/release/bundle/flatpak/`。
  //
  // **只作为手动下载项**：Flatpak 是只读沙箱，应用自身（以及官方更新插件）
  // 都无法替换它，必须由 `flatpak update` 完成。客户端 `auto_kinds` 刻意
  // 不包含 `flatpak`，见 update/model.rs 的 KIND_FLATPAK 说明。
  for (const flatpakDir of [
    join(dir, "bundle", "flatpak"),
    join(ROOT, "build-aux", "flatpak"),
  ]) {
    if (!existsSync(flatpakDir)) continue;
    const hit = readdirSync(flatpakDir).find((f) => f.endsWith(".flatpak"));
    if (!hit) continue;
    found.push({
      source: join(flatpakDir, hit),
      file: `MC-Link-${version}-linux-${arch}-flatpak.flatpak`,
      kind: "flatpak",
    });
    break; // 两处都可能有，取先命中的一处即可，避免重复条目
  }

  return found;
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

function releaseNotes() {
  const notesPath = join(ROOT, "release-notes.md");
  return existsSync(notesPath) ? readFileSync(notesPath, "utf8").trim() : "";
}

/** 把本次产出的包转成清单里的资产条目（算 sha256 与大小）。 */
function describeAssets(items) {
  return items.map(({ file, kind }) => {
    const full = join(OUT_DIR, file);
    return {
      platform: PLATFORM,
      kind,
      file,
      sha256: sha256(full),
      size: statSync(full).size,
    };
  });
}

/**
 * 合并同版本的资产：读旧清单 → 覆盖"同平台 + 同类型"的条目 → 排序。
 *
 * 多次执行（x64 与 arm64 各跑一次）会把同一版本的资产合并进同一份清单，
 * 因此 CI 里可以分平台增量追加。
 */
function mergeAssets(fresh) {
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
  return assets;
}

/**
 * 写我们的更新清单（`latest.json`）。
 *
 * `official_plugin` 告诉界面"本次发布能不能走官方插件路径"：
 * 为 false 时，界面应当只给手动下载（或干脆不提示"一键更新"），
 * 免得用户点一次注定失败的更新 —— 那正是两份清单不同步时的症状。
 *
 * 老客户端不认识这些字段会直接忽略（serde 默认忽略未知字段），
 * 因此 `manifest_version` 保持 1 不变：客户端见到更高的版本号会**直接拒绝**这份清单。
 */
function writeManifest(assets, plugin) {
  const manifest = {
    manifest_version: 1,
    version,
    release_date: new Date().toISOString().slice(0, 10),
    release_notes: releaseNotes(),
    mandatory: MANDATORY,
    // 本次发布能否走官方插件路径（见 pluginAvailability）。
    official_plugin: plugin.available,
    // 被 tauri.json 覆盖的平台（客户端平台标识）。
    official_plugin_platforms: plugin.coveredPlatforms,
    // 不可用时的原因（人可读）。
    official_plugin_reason: plugin.reason,
    assets,
  };

  writeFileSync(MANIFEST_PATH, JSON.stringify(manifest, null, 2) + "\n", "utf8");
}

/**
 * 生成 Tauri 官方更新插件所需的清单（`Assets/update/tauri.json`）。
 *
 * 没有 `TAURI_SIGNING_PRIVATE_KEY` 时直接跳过：插件签名校验不可关闭，
 * 生成一份无法通过校验的清单只会造成"以为配好了却更新不了"。
 *
 * 返回值描述"这次插件清单怎么样"，供 latest.json 标注 `official_plugin`：
 * `{ written: boolean, reason: string, platforms: string[] }`。
 */
function writePluginManifest(items) {
  if (!signingConfigured()) {
    const reason = "未配置签名私钥（TAURI_SIGNING_PRIVATE_KEY 或 _PATH），没有生成 tauri.json";
    console.log(`[make-update] ${reason}（官方插件路径不可用；Windows 安装版仍走自研兜底）`);
    dropStalePluginManifest(reason);
    return { written: false, reason, platforms: [] };
  }
  const env = signingEnv();

  // 插件只认安装器 / AppImage / .app.tar.gz，便携版单文件不在其中
  const manageable = items.find(({ file }) =>
    /-setup\.exe$|\.msi$|\.AppImage$|\.app\.tar\.gz$/.test(file),
  );
  if (!manageable) {
    const reason = "没有可交给官方插件的产物，tauri.json 未生成";
    console.warn(`[make-update] ${reason}`);
    dropStalePluginManifest(reason);
    return { written: false, reason, platforms: [] };
  }

  const full = join(OUT_DIR, manageable.file);
  try {
    signFile(full, env);
  } catch (e) {
    const reason = `签名失败，tauri.json 未生成：${e.message}`;
    console.warn(`[make-update] ${reason}`);
    dropStalePluginManifest(reason);
    return { written: false, reason, platforms: [] };
  }

  const signature = readFileSync(`${full}.sig`, "utf8").trim();

  // 与 latest.json 一样**合并**同版本的其它平台键。
  // 不合并的话，按文档先跑 x64、再跑 `--platform windows-aarch64` 时，
  // 第二次会把 x86_64 那段整块抹掉——那个平台的用户就再也收不到更新，
  // 而且清单本身看起来完全正常。
  let platforms = {};
  if (existsSync(PLUGIN_MANIFEST_PATH)) {
    try {
      const prev = JSON.parse(readFileSync(PLUGIN_MANIFEST_PATH, "utf8"));
      if (prev.version === version && prev.platforms) {
        platforms = { ...prev.platforms };
      }
    } catch {
      // 旧清单损坏：以本次生成的为准
    }
  }

  const manifest = {
    version,
    notes: releaseNotes(),
    pub_date: new Date().toISOString(),
    platforms: {
      ...platforms,
      [pluginPlatformKey(PLATFORM)]: {
        signature,
        url: `${RELEASE_BASE}/v${version}/${manageable.file}`,
      },
    },
  };

  writeFileSync(
    PLUGIN_MANIFEST_PATH,
    JSON.stringify(manifest, null, 2) + "\n",
    "utf8",
  );
  console.log(`[make-update] 插件清单已生成: ${PLUGIN_MANIFEST_PATH}`);
  return {
    written: true,
    reason: "",
    platforms: Object.keys(manifest.platforms),
  };
}

/**
 * 删掉**版本对不上**的旧插件清单。
 *
 * 残留的旧清单会让官方插件报"未发现可用更新"（它比界面看到的版本旧），
 * 于是界面提示有更新、插件却说没有 —— 用户先看到一次失败/回退。
 *
 * 同版本的清单**保留**：它可能带着另一个平台（例如先跑的 x64）已经签好的条目。
 */
function dropStalePluginManifest(reason) {
  if (!existsSync(PLUGIN_MANIFEST_PATH)) return;
  let prev = null;
  try {
    prev = JSON.parse(readFileSync(PLUGIN_MANIFEST_PATH, "utf8"));
  } catch {
    // 读不出来就当它坏了：留着只会让插件拿到一份看不懂的清单
  }
  if (prev && prev.version === version) return;
  const found = (prev && prev.version) || "无法解析";
  rmSync(PLUGIN_MANIFEST_PATH, { force: true });
  console.warn(
    `[make-update] 已删除版本对不上的插件清单 tauri.json（${found} != ${version}）：${reason}`,
  );
}

/**
 * 官方插件路径是否真的可用。
 *
 * 判定依据是**清单文件的最终状态**（而不是"这次运行有没有写它"）：
 * CI 里可能 x64 那次带签名密钥、arm64 那次没有，两次跑完磁盘上的
 * tauri.json 才是发布出去的那一份。
 *
 * 两个条件都要满足，否则界面就不该给"一键更新"：
 * 1. `tauri.json` 存在，且版本与 `latest.json` 一致
 *    （版本旧了插件会报"未发现可用更新"）；
 * 2. `tauri.json` 覆盖了清单里出现的**每一个**平台
 *    （例：x64 签了、arm64 没签 → arm64 的插件路径不可用）。
 */
function pluginAvailability(assets) {
  const plugin = readPluginManifest();
  if (!plugin) {
    return {
      available: false,
      reason: "没有 tauri.json（官方插件路径不可用）",
      coveredPlatforms: [],
      uncoveredPlatforms: [...new Set(assets.map((a) => a.platform))],
    };
  }
  if (plugin.version !== version) {
    return {
      available: false,
      reason: `tauri.json 的版本是 ${plugin.version}，与 latest.json（${version}）不一致`,
      coveredPlatforms: [],
      uncoveredPlatforms: [...new Set(assets.map((a) => a.platform))],
    };
  }

  const keys = Object.keys(plugin.platforms || {});
  // 插件键 → 客户端平台标识（反向映射：清单按客户端平台写）
  const covered = [];
  const uncovered = [];
  for (const platform of new Set(assets.map((a) => a.platform))) {
    if (keys.includes(pluginPlatformKey(platform))) covered.push(platform);
    else uncovered.push(platform);
  }
  if (uncovered.length > 0) {
    return {
      available: false,
      reason: `tauri.json 未覆盖平台：${uncovered.join(", ")}`,
      coveredPlatforms: covered,
      uncoveredPlatforms: uncovered,
    };
  }
  return {
    available: true,
    reason: "",
    coveredPlatforms: covered,
    uncoveredPlatforms: [],
  };
}

/** 读出磁盘上的插件清单（读不出来就当没有）。 */
function readPluginManifest() {
  if (!existsSync(PLUGIN_MANIFEST_PATH)) return null;
  try {
    return JSON.parse(readFileSync(PLUGIN_MANIFEST_PATH, "utf8"));
  } catch {
    return null;
  }
}

/**
 * **只能由官方插件安装**的资产类型。
 *
 * 与客户端 `update/model.rs` 的 `KIND_*` 对应：Linux 的 AppImage 与
 * macOS 的 `.app.tar.gz` 只有官方插件能装（自研路径不支持它们）。
 *
 * `installer`（Windows 安装版）**不在其中**：它有自研兜底（安装器脚本），
 * 便携版的 `portable` 同理（自研路径直接替换 exe）。
 */
const PLUGIN_ONLY_KINDS = new Set(["appimage", "macos-app"]);

/**
 * 剔除"插件装不上"的资产。
 *
 * 插件清单没覆盖某个平台时，那个平台上只有插件能装的包对客户端就是**废的**：
 * 界面会照着 latest.json 提示"有新版本"，用户点下去却必然失败
 * （官方插件报"未发现可用更新"，回退自研路径也装不了 AppImage / `.app`）。
 *
 * 剔除之后，客户端在这些平台上会落到"只能手动下载"——这才是诚实的表述。
 */
function dropUninstallableAssets(assets, plugin) {
  if (plugin.available) return assets;
  const dropped = [];
  const kept = assets.filter((a) => {
    if (PLUGIN_ONLY_KINDS.has(a.kind) && plugin.uncoveredPlatforms.includes(a.platform)) {
      dropped.push(a);
      return false;
    }
    return true;
  });
  if (dropped.length > 0) {
    console.warn(
      `[make-update] 已从清单中剔除 ${dropped.length} 个"插件装不上"的资产：` +
        dropped.map((a) => `${a.platform}/${a.kind}`).join(", "),
    );
  }
  return kept;
}

/**
 * 平台键转换：插件清单用的是**插件自己的**平台键。
 *
 * 官方插件按 `{os}-{arch}` 查表，架构名取自 Rust 的 `std::env::consts::ARCH`
 * （见 tauri-plugin-updater 的 `updater_arch()`）：`x86_64` / `aarch64` / `armv7`。
 * 与我们的平台标识有两处差异：
 *
 * - 我们的 `macos` 对应插件的 `darwin`；
 * - 我们的 `arm64`（上游命名）对应插件的 `aarch64`（Rust 架构名）。
 *
 * 写错的后果：插件在自己的清单里查不到这个平台 → 报"未发现可用更新"，
 * 而界面（读 latest.json）已经提示"有新版本" —— 用户先看到一次失败。
 */
function pluginPlatformKey(platform) {
  return platform.replace(/^macos-/, "darwin-").replace(/-arm64$/, "-aarch64");
}

/**
 * 归一化签名相关的环境变量。
 *
 * 实现在 `scripts/signer-env.mjs`——**与发布前检查共用同一份**，
 * 避免"检查时用的是 A 密钥、签名时用的是 B 密钥"这类两边漂移。
 * 它会优先用环境变量（CI secrets），否则回填本地 `.tauri/` 下的密钥与密码。
 */
function signingEnv() {
  return loadSigningEnv();
}

/** 是否配置了可用于签名的私钥。 */
function signingConfigured() {
  const env = signingEnv();
  return Boolean(
    (env.TAURI_SIGNING_PRIVATE_KEY_PATH || "").trim() ||
      (env.TAURI_SIGNING_PRIVATE_KEY || "").trim(),
  );
}

/**
 * 调用 Tauri CLI 给产物签名，生成同目录的 `<file>.sig`。
 *
 * 直接执行 CLI 的 JS 入口，而不是 `pnpm exec tauri`：在 Windows 上
 * `execFileSync` 调用 `.cmd` 会因为 Node 的安全限制直接报 `EINVAL`
 * （批处理必须由 shell 执行），而走 shell 又要处理路径里的空格与引号。
 */
function signFile(file, env) {
  const cli = join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js");
  if (!existsSync(cli)) {
    throw new Error(`未找到 Tauri CLI（${cli}），请先执行 pnpm install`);
  }
  execFileSync(process.execPath, [cli, "signer", "sign", file], {
    cwd: ROOT,
    stdio: "inherit",
    env,
  });
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
    if (name === "latest.json" || name === "tauri.json" || name.endsWith(".sig")) {
      // 清单与签名文件随本次发布生成，不参与版本清理
      if (name.endsWith(".sig") && !name.includes(`-${version}-`)) {
        rmSync(join(OUT_DIR, name), { force: true });
        console.log(`[make-update] 已清理旧签名: ${name}`);
      }
      continue;
    }
    if (!name.startsWith("MC-Link-")) continue;
    // 形如 MC-Link-<版本>-<平台>-<类型>.<扩展名>
    if (name.includes(`-${version}-`)) continue;
    rmSync(join(OUT_DIR, name), { force: true });
    console.log(`[make-update] 已清理本地旧版本包: ${name}`);
  }
}
