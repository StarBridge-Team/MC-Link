#!/usr/bin/env node
/**
 * pnpm tauri 包装脚本
 * ------------------------------------------------------------------
 * - `pnpm tauri build`            : 自动改版本号 + tauri build --ci + 重命名产物 + 生成更新包与清单 + 上传资源
 * - `pnpm tauri build --minor`    : 次版本号 +1（--major / --patch 同理，默认 patch）
 * - `pnpm tauri build --no-bump`  : 不改版本号
 * - `pnpm tauri build --no-upload`: 跳过资源上传
 * - `pnpm tauri build --mandatory`: 生成的更新清单标记为强制更新
 * - `pnpm tauri build --no-release`: 仅执行原生 tauri build（不做版本/上传）
 * - `pnpm tauri dev|icon|...`     : 透传给真实 tauri CLI
 *
 * 其他架构（如 arm64）的更新包，请单独执行：
 *   node scripts/make-update.mjs --platform windows-aarch64
 *
 * 发布构建会注入 MC_LINK_BUILD_CHANNEL=official：客户端据此才允许自动更新，
 * 详见 src-tauri/src/build_channel.rs。**CI 若要产出可自动更新的包，也必须走本脚本**
 * （直接调用 `pnpm exec tauri build` 编出来的会被视为"自行构建"）。
 *
 * 原生 tauri 通过 `pnpm exec tauri` 调用，避免递归调用本脚本。
 */
import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import { preflight, signingEnv } from "./signer-env.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");

const args = process.argv.slice(2);
const cmd = args[0];
const rest = args.slice(1);

const KNOWN_BUILD_FLAGS = new Set([
  "--major",
  "--minor",
  "--patch",
  "--no-bump",
  "--no-upload",
  "--no-release",
  "--mandatory",
]);

if (cmd === "build" && !rest.includes("--no-release")) {
  const noBump = rest.includes("--no-bump");
  const noUpload = rest.includes("--no-upload");
  const bump = rest.includes("--major")
    ? "major"
    : rest.includes("--minor")
      ? "minor"
      : rest.includes("--patch")
        ? "patch"
        : "patch";
  // 其余以 -- 开头的参数（如 --ci、--debug、--target）透传给原生 tauri
  const extra = rest.filter((a) => a.startsWith("--") && !KNOWN_BUILD_FLAGS.has(a));

  if (!noBump) bumpVersion(bump);

  // 发布前检查：更新签名私钥是否就绪、与配置里的公钥是否配对
  checkUpdateSigning();

  // 标记为官方发布构建：只有带这个标记的二进制才允许自动更新
  // （见 src-tauri/src/build_channel.rs）。--no-release 的构建不带标记，
  // 会被客户端视为"自行构建"而拒绝自更新，这是刻意的。
  //
  // 同时带上签名环境（本地 .tauri/ 或 CI secrets），让 make-update 能签出
  // 官方插件要读的 tauri.json。
  const releaseEnv = { ...signingEnv(), MC_LINK_BUILD_CHANNEL: "official" };

  // 真实 tauri build（--ci 保证非交互）
  execSync(`pnpm exec tauri build --ci ${extra.join(" ")}`.trim(), {
    cwd: ROOT,
    stdio: "inherit",
    env: releaseEnv,
  });

  // 产物重命名（EXE/安装包加版本号）
  execSync("node rename-build.js", { cwd: ROOT, stdio: "inherit" });

  // 生成应用更新包与更新清单（必须早于 sync-assets，否则清单与更新包不会被上传）
  const makeUpdateArgs = ["scripts/make-update.mjs"];
  if (rest.includes("--mandatory")) makeUpdateArgs.push("--mandatory");
  execSync(`node ${makeUpdateArgs.join(" ")}`, {
    cwd: ROOT,
    stdio: "inherit",
    env: releaseEnv,
  });

  // 资源上传到资源服务器
  if (!noUpload) {
    execSync("node scripts/sync-assets.mjs", { cwd: ROOT, stdio: "inherit" });
  }

  process.exit(0);
}

// 其余子命令（dev / icon / ...）原样透传
execSync(`pnpm exec tauri ${args.join(" ")}`.trim(), {
  cwd: ROOT,
  stdio: "inherit",
});

// ------------------------------------------------------------------
// 发布前检查：更新签名（见 scripts/signer-env.mjs）
// ------------------------------------------------------------------
function checkUpdateSigning() {
  const status = preflight();
  console.log("\n[tauri-build] 发布前检查");
  for (const line of status.lines) console.log(`  ${line}`);

  // 公私钥不配对必须中断：编译进客户端的公钥与签名私钥不匹配时，
  // 客户端会拒绝所有更新，而这在运行时完全看不出来。
  if (status.level === "error") {
    console.error("\n[tauri-build] 已中断发布：更新签名配置不一致（修复方式见上）\n");
    process.exit(1);
  }
  if (status.level === "warn") {
    console.warn("\n[tauri-build] 继续发布，但官方更新插件路径本次不可用（见上）");
  }
  console.log("");
}

// ------------------------------------------------------------------
// 版本号自增：同步 package.json 与 src-tauri/tauri.conf.json
// ------------------------------------------------------------------
function bumpVersion(kind) {
  const pkgPath = join(ROOT, "package.json");
  const confPath = join(ROOT, "src-tauri", "tauri.conf.json");
  const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
  const conf = JSON.parse(readFileSync(confPath, "utf8"));

  const old = pkg.version;
  const next = inc(old, kind);
  pkg.version = next;
  conf.version = next;

  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
  writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n");
  console.log(`[tauri-build] 版本 ${old} -> ${next} (${kind})`);
}

function inc(v, kind) {
  const parts = (v.split(".").map((n) => parseInt(n, 10) || 0));
  let [maj, min, pat] = [parts[0] ?? 0, parts[1] ?? 0, parts[2] ?? 0];
  if (kind === "major") {
    maj += 1;
    min = 0;
    pat = 0;
  } else if (kind === "minor") {
    min += 1;
    pat = 0;
  } else {
    pat += 1;
  }
  return `${maj}.${min}.${pat}`;
}
