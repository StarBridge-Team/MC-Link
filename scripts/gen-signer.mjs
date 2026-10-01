#!/usr/bin/env node
/**
 * 生成 / 同步更新签名密钥对。
 * ------------------------------------------------------------------
 * 用法：
 *   pnpm signer:generate             生成新密钥对（已存在则拒绝）
 *   pnpm signer:generate --force     覆盖生成（**会作废已有客户端的更新能力**）
 *   pnpm signer:sync                 只把本地 .pub 同步进 tauri.conf.json
 *
 * 产物（全部在 gitignore 覆盖的 `.tauri/` 下，绝不入库）：
 *   .tauri/mc-link-signer.key        私钥（放进 GitHub secret）
 *   .tauri/mc-link-signer.key.pub    公钥（写进 tauri.conf.json 的 plugins.updater.pubkey）
 *   .tauri/signer-password.txt       私钥密码（放进 GitHub secret）
 *
 * # 为什么不做成"每次发版自动生成"
 *
 * 公钥是**编译进已发布客户端**的。若私钥丢失后由发布脚本悄悄重新生成一对，
 * 客户端里那份旧公钥就再也配不上新私钥 —— 所有已安装用户会永久收不到更新，
 * 而发布日志看起来一切正常。因此生成必须是**显式的人为动作**，
 * 发布链路只做"检查 + 报告"（见 scripts/signer-env.mjs 的 preflight）。
 */
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  CONF_PATH,
  KEY_PATH,
  KEY_PUB_PATH,
  PASSWORD_PATH,
  ROOT,
  signingEnv,
} from "./signer-env.mjs";

const args = process.argv.slice(2);
const force = args.includes("--force");
const syncOnly = args.includes("--sync");
const passwordArg = valueOf("--password");

/** 生成一个 32 位随机密码（仅字母数字，避免命令行转义问题）。 */
function randomPassword() {
  const pool = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let out = "";
  for (let i = 0; i < 32; i += 1) {
    out += pool[Math.floor(Math.random() * pool.length)];
  }
  return out;
}

function valueOf(name) {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}

function main() {
  if (syncOnly) {
    syncPubkey();
    return;
  }

  if (existsSync(KEY_PATH) && !force) {
    console.error(
      `[signer] 已存在密钥：${KEY_PATH}\n` +
        "  想重新生成会作废所有已发布客户端的更新能力（它们的公钥是编译进去的）。\n" +
        "  确实要换请加 --force，并同步更新 GitHub secret 与 tauri.conf.json。\n" +
        "  只是想让配置里的公钥与本地密钥对齐：pnpm signer:sync",
    );
    process.exit(1);
  }

  mkdirSync(dirname(KEY_PATH), { recursive: true });

  const password = passwordArg || randomPassword();
  const cli = join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js");
  if (!existsSync(cli)) {
    console.error(`[signer] 未找到 Tauri CLI（${cli}），请先执行 pnpm install`);
    process.exit(1);
  }

  // 直接执行 CLI 的 JS 入口：Windows 上 execFileSync 调用 .cmd 会 EINVAL
  execFileSync(
    process.execPath,
    [cli, "signer", "generate", "-w", KEY_PATH, "-p", password, "--ci"],
    { cwd: ROOT, stdio: "inherit" },
  );

  writeFileSync(PASSWORD_PATH, password, "utf8");
  console.log(`[signer] 私钥：${KEY_PATH}`);
  console.log(`[signer] 密码：${PASSWORD_PATH}（未在终端打印，请存入密码管理器）`);

  syncPubkey();
  printNextSteps();
}

/** 把本地 `.pub` 内容写入 tauri.conf.json 的 plugins.updater.pubkey。 */
function syncPubkey() {
  if (!existsSync(KEY_PUB_PATH)) {
    console.error(`[signer] 未找到公钥文件：${KEY_PUB_PATH}，请先生成密钥对`);
    process.exit(1);
  }
  const pub = readFileSync(KEY_PUB_PATH, "utf8").trim();
  const conf = JSON.parse(readFileSync(CONF_PATH, "utf8"));
  conf.plugins = conf.plugins || {};
  conf.plugins.updater = conf.plugins.updater || {};
  const before = (conf.plugins.updater.pubkey || "").trim();

  conf.plugins.updater.pubkey = pub;
  writeFileSync(CONF_PATH, JSON.stringify(conf, null, 2) + "\n");

  console.log(
    before === pub
      ? "[signer] tauri.conf.json 的公钥已是最新，无需改动"
      : "[signer] 已把公钥写入 tauri.conf.json 的 plugins.updater.pubkey",
  );
}

function printNextSteps() {
  const env = signingEnv();
  console.log(
    "\n下一步（把密钥交给 CI，两个都要）：\n" +
      "  gh secret set TAURI_SIGNING_PRIVATE_KEY < .tauri/mc-link-signer.key\n" +
      "  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < .tauri/signer-password.txt\n" +
      "\n本地发版无需任何设置：scripts/signer-env.mjs 会自动读取 .tauri/ 下的密钥与密码" +
      `\n（当前私钥来源：${env.TAURI_SIGNING_PRIVATE_KEY_PATH ? "本地 .tauri/" : "环境变量"}）。`,
  );
}

main();
