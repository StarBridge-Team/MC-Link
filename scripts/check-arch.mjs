#!/usr/bin/env node
/**
 * 架构守卫：把"口头约定"变成 CI 里跑得起来的检查。
 *
 * 为什么需要它：本项目长期由 AI 辅助开发，约定写在文档里没人执行，
 * 于是反复出现"新代码绕过了封装层"这类退化（例如页面直接调 `invoke`、
 * 组件里直接写 `localStorage`）。文档靠自觉，脚本不靠。
 *
 * 规则分两级：
 * - **error**：明确违反分层约定，直接让 CI 失败；
 * - **warn**：需要人工判断的（文件过大、TODO 堆积），打印出来但不拦 CI。
 *
 * 用法：node scripts/check-arch.mjs
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");

/** 单文件行数上限（超限只警告：拆分是重构工作，不该一次提交里顺手做）。 */
const MAX_LINES_FRONTEND = 400;
const MAX_LINES_BACKEND = 500;

/** 跳过这些目录（构建产物、依赖、版本控制）。 */
const SKIP_DIRS = new Set([
  "node_modules",
  "target",
  "dist",
  ".git",
  ".tauri",
  "Assets",
  "Pages",
  "SettingMeta",
]);

const SOURCE_EXT = new Set([".ts", ".vue", ".rs", ".mjs", ".js", ".tsx"]);

const errors = [];
const warnings = [];

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    if (SKIP_DIRS.has(entry)) continue;
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      walk(full, out);
    } else if (SOURCE_EXT.has(extname(entry))) {
      out.push(full);
    }
  }
  return out;
}

/** 相对仓库根、用正斜杠表示（跨平台一致的输出）。 */
function rel(full) {
  return relative(ROOT, full).split("\\").join("/");
}

const files = walk(ROOT);

// ------------------------------------------------------------------
// error：分层约定
// ------------------------------------------------------------------

for (const file of files) {
  const path = rel(file);
  const text = readFileSync(file, "utf8");
  const lines = text.split("\n");

  // 1) IPC 只允许出现在 api 层：其它地方直接 invoke 会在命令改名时被漏掉
  //
  // 覆盖 `invoke(` / `invoke<` 之外的两种常见绕过写法：
  //   - `window.__TAURI__.invoke(...)`（`withGlobalTauri` 打开时）
  //   - `import { invoke as ipc }` 这类别名解构
  // 规则宁可宽一点：这道守卫的价值在于拦住"顺手直连"，而不是拦住刻意规避。
  if (path.startsWith("src/") && !path.startsWith("src/lib/api/")) {
    const invokeCall = /\binvoke\s*[(<]/;
    const tauriGlobal = /__TAURI__/;
    lines.forEach((line, index) => {
      const trimmed = line.trimStart();
      if (trimmed.startsWith("*") || trimmed.startsWith("//")) return;
      if (invokeCall.test(line) || tauriGlobal.test(line)) {
        errors.push(`${path}:${index + 1} 出现 invoke(...)：IPC 只能写在 src/lib/api/**`);
      }
    });
  }

  // 2) 本地存储只允许经 src/lib/persist.ts：直接写会让 key 散落且失败被静默吞掉
  if (path.startsWith("src/") && path !== "src/lib/persist.ts") {
    lines.forEach((line, index) => {
      if (/\b(localStorage|sessionStorage)\s*\./.test(line) && !line.trimStart().startsWith("*")) {
        errors.push(
          `${path}:${index + 1} 直接访问 ${line.includes("sessionStorage") ? "sessionStorage" : "localStorage"}：请改用 src/lib/persist.ts`,
        );
      }
    });
  }
}

// ------------------------------------------------------------------
// warn：文件体积与待办
// ------------------------------------------------------------------

let todoCount = 0;
const oversized = [];

for (const file of files) {
  const path = rel(file);
  const text = readFileSync(file, "utf8");
  const lineCount = text.split("\n").length;

  todoCount += (text.match(/\b(TODO|FIXME|HACK)\b/g) ?? []).length;

  // 只对"实现代码"做体积检查：测试文件天然长
  const isTest = /(^|\/)(tests?|__tests__)\//.test(path) || /tests?\.(rs|ts)$/.test(path);
  if (isTest) continue;

  const limit = path.startsWith("src/") ? MAX_LINES_FRONTEND : MAX_LINES_BACKEND;
  if (lineCount > limit) {
    oversized.push({ path, lineCount, limit });
  }
}

if (oversized.length > 0) {
  oversized.sort((a, b) => b.lineCount - a.lineCount);
  warnings.push(
    `有 ${oversized.length} 个文件超过行数上限（前端 ${MAX_LINES_FRONTEND} / 后端 ${MAX_LINES_BACKEND}）：`,
  );
  for (const item of oversized) {
    warnings.push(`    ${item.lineCount} 行  ${item.path}（上限 ${item.limit}）`);
  }
}

if (todoCount > 0) {
  warnings.push(`代码里存在 ${todoCount} 处 TODO/FIXME/HACK`);
}

// ------------------------------------------------------------------
// 输出
// ------------------------------------------------------------------

console.log("架构守卫检查");
console.log(`  扫描 ${files.length} 个源文件`);

for (const warning of warnings) {
  console.log(`[warn] ${warning}`);
}

if (errors.length === 0) {
  console.log("[ok] 分层约定未被违反");
  process.exit(0);
}

console.error("");
console.error(`[error] 发现 ${errors.length} 处违反分层约定的写法：`);
for (const error of errors) {
  console.error(`    ${error}`);
}
console.error("");
console.error("说明：这些规则是为了防止新代码绕过封装层，详见 project_memory.md 的约定章节。");
process.exit(1);
