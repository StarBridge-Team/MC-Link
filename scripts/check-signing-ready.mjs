#!/usr/bin/env node
/**
 * 更新签名前置检查（可在不构建的情况下单独运行）。
 * ------------------------------------------------------------------
 * 复用 `scripts/signer-env.mjs` 的 `preflight()`：
 *   - 公私钥不配对          → error（中断）
 *   - 发布环境缺签名私钥    → error（中断）
 *   - 本地开发构建缺私钥    → warn（放行）
 *
 * 有了这个入口，CI 就能在"检查阶段"（每次 push/PR，快）而不是"发布阶段"（慢、且已经
 * 打包完成）发现签名配置问题。发布流程内部的 `tauri-build.mjs` 仍会再查一次。
 *
 * 用法：node scripts/check-signing-ready.mjs
 *   MC_LINK_BUILD_CHANNEL=official 或 CI=true 时，"缺私钥"视为 error。
 */
import { preflight } from "./signer-env.mjs";

const status = preflight();

console.log("更新签名前置检查");
for (const line of status.lines) console.log(`  ${line}`);

if (status.level === "error") {
  console.error("\n[error] 签名配置不可用于发布（修复方式见上）\n");
  process.exit(1);
}
if (status.level === "warn") {
  console.log("\n[warn] 本次环境缺少签名私钥；若这是发布构建，请在 CI 里配置 secrets\n");
}
