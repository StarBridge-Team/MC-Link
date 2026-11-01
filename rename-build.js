/**
 * 构建收尾：**只**给未打包的调试用 EXE 复制一份带版本号的副本。
 * ------------------------------------------------------------------
 * 由 `scripts/tauri-build.mjs` 在 `tauri build` 之后调用。
 *
 * # 为什么这里几乎什么也不做了
 *
 * 过去这个脚本会给 `bundle/` 里的安装包改名字。现在命名由
 * `scripts/make-update.mjs` **统一裁定**（`MC-Link-<版本>-<平台>-<架构>-<类型>.<扩展名>`），
 * 并复制到 `Assets/update/` 下供发布与 `latest.json` 引用；
 * `bundle/` 里的原始文件名不再被任何人消费，因此不该再改。
 *
 * 原地改名会制造两类问题（都已在发行版里实际出现）：
 * 1. 双版本号：`MC Link_0.5.4_x64-setup-0.5.4.exe`；
 * 2. 同一产物两个名字同时挂到发行版，用户无从选择。
 *
 * 版本号仍然保留在这里，因为下面的 EXE 副本要用它。
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

// 【已停用】不再对 `bundle/` 里的产物重命名。
//
// # 为什么停用
//
// 这些产物**不是最终分发物**：`scripts/make-update.mjs` 会把它们按统一命名
// （`MC-Link-<版本>-<平台>-<架构>-<类型>.<扩展名>`）复制到 `Assets/update/` 下，
// 那才是挂到发行版、以及被 `latest.json` 引用的名字。
//
// 原先这里额外改名会带来两个真实问题：
//
// 1. **产生双版本号的名字**：Tauri 已经写过版本（`MC Link_0.5.4_x64-setup.exe`），
//    本文件的 `VERSION_SUFFIX` 只认「末尾是 `-x.y.z`」，匹配不到 `_x64-setup` 结尾，
//    于是再拼一次版本，得到 `MC Link_0.5.4_x64-setup-0.5.4.exe`。
// 2. **让发行版出现同一产物的两个名字**：CI 一度同时收集 `bundle/` 与
//    `Assets/update/`，用户面对 `MC-Link-0.5.4-windows-x86_64-setup.exe` 与
//    `MC Link_0.5.4_x64-setup-0.5.4.exe` 无从选择。
//
// 而重命名原本想解决的"区分本次构建的产物"并不需要它：Tauri 自己就会把版本号
// 写进文件名，`make-update.mjs` 的版本匹配（`_<版本>_` / `-<版本>-`）直接可用。
//
// 版本匹配的兜底逻辑仍保留在 `make-update.mjs::findInstaller`（找不到精确匹配时
// 退化为取目录中最新修改的那个，并打警告），因此这里不再改名是安全的。
console.log(`[rename-build] 跳过 bundle 重命名（命名由 make-update.mjs 统一裁定），版本号: ${version}`);

// 未打包的可执行文件：复制一份带版本号的副本，**仅供本地调试时区分版本**。
// 不要把它当成分发物——发布走的产物全部来自 `Assets/update/`，
// 那里的名字由 `make-update.mjs` 决定（例如
// `MC-Link-<版本>-windows-x86_64-portable.exe`）。
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
  console.log(`[rename-build] 调试副本: mc-link.exe -> mc-link-${version}.exe`);
}

console.log("");
console.log("========================================");
console.log("  MC-Link 构建完成!");
console.log("  版本: " + version);
console.log("  路径: src-tauri/target/release/");
console.log("========================================");
console.log("");
