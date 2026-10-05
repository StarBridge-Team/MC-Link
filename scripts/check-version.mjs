#!/usr/bin/env node
/**
 * 版本号一致性门禁。
 * ------------------------------------------------------------------
 * 版本号在仓库里有**三个权威位置**，发布链路依赖它们一致：
 *
 *   package.json              —— CI 读它作 tag / 标题 / 产物名 / 清单版本
 *   src-tauri/tauri.conf.json —— 安装包内嵌的版本元数据
 *   src-tauri/Cargo.toml      —— crate 自身版本（`env!("CARGO_PKG_VERSION")`）
 *
 * 三者不一致的后果**不会报错**，只会让产物名、安装记录与客户端版本比较对不上，
 * 属于最难排查的一类问题。而 `scripts/tauri-build.mjs` 的 bumpVersion 只同步前两个，
 * Cargo.toml 必须靠人记得改 —— 所以这里把它变成一个机械检查。
 *
 * 用法：
 *   node scripts/check-version.mjs           # 校验，不一致即退出码 1
 *   node scripts/check-version.mjs --print   # 只打印版本号（CI 取值用）
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");

/** 只接受 `x.y.z`（允许预发布后缀）：Tauri 与清单格式都以此为准。 */
const VERSION_RE = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

export function readVersions() {
  const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));

  const conf = JSON.parse(readFileSync(join(ROOT, "src-tauri", "tauri.conf.json"), "utf8"));

  // Cargo.toml 的 `version = "x.y.z"` 必须取 **[package] 段**里那一个：
  // 依赖行里也有 `version = "..."`，用正则全表扫描会抓错。
  const cargo = readFileSync(join(ROOT, "src-tauri", "Cargo.toml"), "utf8");
  const pkgSection = cargo.split(/^\[/m).find((s) => /^package\]/.test(s.trim())) ?? "";
  const cargoVersion = (pkgSection.match(/^\s*version\s*=\s*"([^"]+)"/m) ?? [])[1] ?? "";

  return {
    packageJson: (pkg.version || "").trim(),
    tauriConf: (conf.version || "").trim(),
    cargoToml: cargoVersion.trim(),
  };
}

/** 返回不一致项的描述数组（空数组 = 一致）。 */
export function findMismatches(versions = readVersions()) {
  const problems = [];
  for (const [label, value] of Object.entries(versions)) {
    if (!value) {
      problems.push(`${label} 缺少 version`);
    } else if (!VERSION_RE.test(value)) {
      problems.push(`${label} 的版本号格式非法: ${value}`);
    }
  }
  const unique = new Set(Object.values(versions));
  if (unique.size > 1) {
    problems.push(
      `三处版本号不一致：` +
        Object.entries(versions)
          .map(([k, v]) => `${k}=${v || "(空)"}`)
          .join("  "),
    );
  }
  return problems;
}

const args = process.argv.slice(2);

if (args.includes("--print")) {
  // 供 CI 取版本：以 package.json 为准（与既有工作流一致）
  console.log(readVersions().packageJson);
  process.exit(0);
}

const versions = readVersions();
const problems = findMismatches(versions);

console.log("版本号一致性检查");
for (const [label, value] of Object.entries(versions)) {
  console.log(`  ${label.padEnd(14)} ${value || "(空)"}`);
}

if (problems.length === 0) {
  console.log(`[ok] 三处版本号一致：${versions.packageJson}`);
  process.exit(0);
}

console.error("");
console.error("[error] 版本号不一致：");
for (const problem of problems) {
  console.error(`    ${problem}`);
}
console.error("");
console.error(
  "  修复：把 package.json / src-tauri/tauri.conf.json / src-tauri/Cargo.toml\n" +
    "        三处改成同一个版本号（发布流程用 --no-bump，版本号以仓库为准）。",
);
process.exit(1);
