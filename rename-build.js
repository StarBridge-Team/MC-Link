/**
 * 构建产物重命名：给 EXE / 安装包加上版本号后缀。
 * ------------------------------------------------------------------
 * 由 `scripts/tauri-build.mjs` 在 `tauri build` 之后调用。
 *
 * # 幂等是硬要求
 *
 * 历史实现只判断"名字里是否含**当前**版本号"，于是每发一个新版本，上一版已经带过
 * 后缀的文件又被追加一次，最终堆出这种产物：
 *
 *   mc-link_0.2.0_x64-setup-v0.2.1-0.2.1-0.2.2-...-0.4.0.exe
 *
 * 现在改成：**先把末尾已有的版本后缀剥掉，再拼当前版本**。
 * 这样同一份产物无论重跑多少次、无论经过多少个版本，名字都只有一段版本号。
 *
 * 顺带修掉一个隐患：安装包名字可能是 `MC Link_0.4.0_x64-setup.exe`（含空格与大写），
 * 旧实现只匹配小写 `mc-link`，这类文件永远不被重命名。这里统一按"文件名里出现
 * mc-link（忽略大小写与空格差异）"判断。
 */
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const pkg = JSON.parse(
  fs.readFileSync(path.join(__dirname, "package.json"), "utf8"),
);
const version = pkg.version;

const bundleDir = path.join(__dirname, "src-tauri", "target", "release", "bundle");

if (!fs.existsSync(bundleDir)) {
  console.log(`错误: 未找到构建目录 ${bundleDir}`);
  process.exit(1);
}

/** 末尾的 `-1.2.3` 版本后缀。 */
const VERSION_SUFFIX = /-\d+\.\d+\.\d+$/;

/** 文件名是否属于本应用的产物（`mc-link` / `MC Link` / `mc_link` 都算）。 */
function isAppArtifact(nameWithoutExt) {
  return /mc[-_ ]?link/i.test(nameWithoutExt);
}

/**
 * 计算目标文件名；返回 null 表示不改动。
 *
 * 两条规则缺一不可：
 *
 * 1. **只处理本次构建的产物**（文件名里已含当前版本号）。各打包器都会把版本写进文件名，
 *    因此这是可靠判据；若不判断，上一版的安装包会被加上**当前**版本后缀，
 *    变成一个"文件名骗人"的产物。
 * 2. **先剥掉末尾已有版本后缀再拼当前版本**：这是幂等的来源，
 *    同一份产物重跑多少次、跨多少个版本都只有一段版本号。
 */
function targetName(name) {
  const ext = path.extname(name);
  const base = name.slice(0, name.length - ext.length);
  if (!isAppArtifact(base)) return null;
  if (!base.includes(version)) return null;

  const stripped = base.replace(VERSION_SUFFIX, "");
  const next = `${stripped}-${version}${ext}`;
  return next === name ? null : next;
}

function renameFiles(dir) {
  for (const entry of fs.readdirSync(dir)) {
    const full = path.join(dir, entry);
    if (fs.statSync(full).isDirectory()) {
      renameFiles(full);
      continue;
    }

    const next = targetName(entry);
    if (!next) continue;

    const nextPath = path.join(dir, next);
    if (fs.existsSync(nextPath)) fs.unlinkSync(nextPath);
    fs.renameSync(full, nextPath);
    console.log(`已重命名: ${entry} -> ${next}`);
  }
}

console.log(`开始重命名构建文件，版本号: ${version}`);
renameFiles(bundleDir);

// 未打包的可执行文件：复制一份带版本号的副本（原文件保持不变，便于直接运行调试）
const exePath = path.join(__dirname, "src-tauri", "target", "release", "mc-link.exe");
if (fs.existsSync(exePath)) {
  const versionedExe = path.join(
    __dirname,
    "src-tauri",
    "target",
    "release",
    `mc-link-${version}.exe`,
  );
  if (fs.existsSync(versionedExe)) fs.unlinkSync(versionedExe);
  fs.copyFileSync(exePath, versionedExe);
  console.log(`已复制: mc-link.exe -> mc-link-${version}.exe`);
}

console.log("");
console.log("========================================");
console.log("  MC-Link 构建完成!");
console.log("  版本: " + version);
console.log("  路径: src-tauri/target/release/");
console.log("========================================");
console.log("");
