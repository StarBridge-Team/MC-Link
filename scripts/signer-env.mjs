#!/usr/bin/env node
/**
 * 更新签名私钥的统一读取与一致性校验（发布链路共用）。
 * ------------------------------------------------------------------
 * 背景：官方更新插件（安装版 / Linux / macOS 走的那条路）**不可关闭签名校验**，
 * 它把 `tauri.conf.json` 里的 `plugins.updater.pubkey` 编译进客户端，
 * 发布时用配对的私钥给产物签名。于是有一类错误在运行时**无法发现**：
 * 客户端的公钥与发布方签名用的私钥不是一对 → 校验永远失败 → "更新不了"。
 * 本模块存在的意义就是把这个错误提前到构建时中断。
 *
 * 私钥来源（优先级从高到低）：
 *   1. 环境变量 `TAURI_SIGNING_PRIVATE_KEY[_PATH]` —— CI 用 secrets 注入
 *   2. 本地 `.tauri/mc-link-signer.key`（gitignore 覆盖）+ `.tauri/signer-password.txt`
 *      —— 让本地发版不必每次手敲环境变量
 *
 * 密钥不会、也不应进入 git：`.gitignore` 已覆盖 `.tauri/` 与 `*.key`。
 */
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

export const ROOT = join(__dirname, "..");
export const KEY_PATH = join(ROOT, ".tauri", "mc-link-signer.key");
export const KEY_PUB_PATH = join(ROOT, ".tauri", "mc-link-signer.key.pub");
export const PASSWORD_PATH = join(ROOT, ".tauri", "signer-password.txt");
export const CONF_PATH = join(ROOT, "src-tauri", "tauri.conf.json");

/** 本地是否已生成过密钥对。 */
export function localKeyExists() {
  return existsSync(KEY_PATH) && existsSync(KEY_PUB_PATH);
}

/**
 * 归一化后的环境变量（含本地密钥回填），可直接透传给 Tauri CLI。
 *
 * Tauri CLI 区分两个变量：`_PATH` 收文件路径、`TAURI_SIGNING_PRIVATE_KEY` 收密钥内容。
 * 为少踩坑，这里允许把路径写进 `_KEY`（只要它确实指向一个存在的文件）。
 */
export function signingEnv() {
  const env = { ...process.env };
  const keyPath = (env.TAURI_SIGNING_PRIVATE_KEY_PATH || "").trim();
  const keyValue = (env.TAURI_SIGNING_PRIVATE_KEY || "").trim();

  // 单行、且确实是个文件 → 当成路径（密钥内容是多行文本，不会命中）
  if (!keyPath && keyValue && !keyValue.includes("\n") && existsSync(keyValue)) {
    env.TAURI_SIGNING_PRIVATE_KEY_PATH = keyValue;
    delete env.TAURI_SIGNING_PRIVATE_KEY;
  }

  // 环境变量完全没给私钥时，回退到本地密钥（连密码一起回填）
  if (!env.TAURI_SIGNING_PRIVATE_KEY_PATH && !env.TAURI_SIGNING_PRIVATE_KEY) {
    if (localKeyExists()) {
      env.TAURI_SIGNING_PRIVATE_KEY_PATH = KEY_PATH;
      if (!env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD && existsSync(PASSWORD_PATH)) {
        env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = readFileSync(PASSWORD_PATH, "utf8").trim();
      }
    }
  }

  return env;
}

/** 私钥来源描述（用于发布日志里说清"这次是用哪把钥匙签的"）。 */
export function keySource() {
  const env = signingEnv();
  if (env.TAURI_SIGNING_PRIVATE_KEY_PATH === KEY_PATH) return "本地 .tauri/";
  if (env.TAURI_SIGNING_PRIVATE_KEY_PATH) return `环境变量路径（${env.TAURI_SIGNING_PRIVATE_KEY_PATH}）`;
  if (env.TAURI_SIGNING_PRIVATE_KEY) return "环境变量内容（CI secrets）";
  return null;
}

/** `tauri.conf.json` 里配置的公钥（编译进客户端的那份）。 */
export function configuredPubkey() {
  const conf = JSON.parse(readFileSync(CONF_PATH, "utf8"));
  return (conf.plugins?.updater?.pubkey || "").trim();
}

/** 本地 `.pub` 文件里的公钥（= 本地私钥配对的公钥）。 */
export function localPubkey() {
  if (!existsSync(KEY_PUB_PATH)) return "";
  return readFileSync(KEY_PUB_PATH, "utf8").trim();
}

/**
 * 校验"编译进客户端的公钥"与"本地私钥"是否配对。
 *
 * 仅当两者都能拿到时才比较；返回 `{ ok, reason }`。
 */
export function checkKeyPubkeyPair() {
  const configured = configuredPubkey();
  const local = localPubkey();

  if (!local) {
    return { ok: true, reason: "本地没有 .pub 文件，无法比对（CI 环境下正常）", compared: false };
  }
  if (!configured) {
    return {
      ok: false,
      compared: true,
      reason:
        "tauri.conf.json 的 plugins.updater.pubkey 为空，但本地已存在密钥对。\n" +
        "    客户端会因此完全无法校验官方插件下发的更新（安装版永远收不到更新）。\n" +
        "    修复：node scripts/gen-signer.mjs --sync",
    };
  }
  if (configured !== local) {
    return {
      ok: false,
      compared: true,
      reason:
        "tauri.conf.json 的公钥与本地私钥**不是一对**。\n" +
        "    这会让已发布的客户端拒绝所有更新（运行时无法发现，只会表现为「更新不了」）。\n" +
        "    修复：确认真要换密钥就执行 node scripts/gen-signer.mjs --sync；\n" +
        "    若只是想换回旧密钥，请把旧的 .pub 内容写回 tauri.conf.json。",
    };
  }
  return { ok: true, compared: true, reason: "公钥与私钥配对正常" };
}

/**
 * 本次构建是否**必须**具备可用的签名配置。
 *
 * 判据是"这次构建会不会被当作官方发布产物分发出去"：
 *   - `MC_LINK_BUILD_CHANNEL=official`（发布流程注入）→ 必须签名；
 *   - `CI=true` 且非 PR 环境（GitHub Actions 的发布 job）→ 必须签名。
 *
 * 本地 `pnpm tauri dev` / 随手 `pnpm build:release` 不属于此列：那时没有可用密钥是
 * 正常的，不该把人拦在门外。
 */
function signingRequired() {
  if ((process.env.MC_LINK_BUILD_CHANNEL || "").trim() === "official") return true;
  const ci = (process.env.CI || "").trim().toLowerCase();
  return ci === "true" || ci === "1";
}

/**
 * 发布前检查：返回 `{ level: "ok" | "warn" | "error", lines: string[] }`。
 *
 * 调用方自行决定是中断还是继续：`error` 一律应当中断发布。
 *
 * # 为什么"没配密钥"在发布环境是 error 而不是 warn
 *
 * 以前它只 warn，于是可能出现"CI 全绿但产物不可更新"：`update/tauri.json` 根本不生成，
 * 安装版客户端永远收不到更新，且没有任何红叉提示。这类缺陷最难排查——
 * 流水线看着正常，用户侧却一直失败。发布链路上，缺签名必须让流水线**红掉**。
 */
export function preflight() {
  const source = keySource();
  const pair = checkKeyPubkeyPair();
  const lines = [];

  lines.push(`更新签名私钥：${source ?? "**未配置**"}`);
  lines.push(`签名公钥配对：${pair.reason}`);

  if (!source) {
    const required = signingRequired();
    const consequence = [
      "  后果：不会生成 update/tauri.json，官方插件路径整体不可用",
      "        （Windows 便携版仍可自研更新；安装版会退回自研拉起安装器）。",
      "  修复：pnpm signer:generate（生成后把公钥同步进 tauri.conf.json）",
    ];
    if (required) {
      return {
        level: "error",
        lines: [
          ...lines,
          ...consequence,
          "  本次是发布构建（official / CI），缺签名会让产物不可更新，已中断。",
        ],
      };
    }
    return { level: "warn", lines: [...lines, ...consequence] };
  }

  // 私钥存在但无法比对本地公钥（CI 环境没有 .pub 文件）：至少提示一句，
  // 让"公钥可能配错"这件事在日志里留痕，而不是完全静默。
  if (!pair.compared) {
    lines.push(
      "  提示：无法在本次环境比对公钥（缺少本地 .pub）。",
      "        发布前建议在本地跑一次 `node scripts/gen-signer.mjs --sync` 核对。",
    );
  }

  return { level: pair.ok ? "ok" : "error", lines };
}
