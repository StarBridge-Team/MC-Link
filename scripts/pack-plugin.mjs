#!/usr/bin/env node
/**
 * 打包插件：生成完整性清单 → 签名（可选）→ 产出 `.mclink-plugin`（tar.gz）。
 * ------------------------------------------------------------------
 * 用法：
 *   node scripts/pack-plugin.mjs <插件目录> [--out dist] [--no-sign]
 *
 * 产物：
 *   <out>/<id>-<version>.mclink-plugin
 *
 * # 为什么清单要放进包里
 *
 * 核心每次扫描插件目录都会用 `integrity.txt` + `integrity.sig` 复核**整个目录**：
 * 签名只覆盖清单，清单覆盖每个文件的哈希，并且拒绝"清单之外的文件"。所以清单必须
 * 随包分发，而不是留在发布者机器上。
 *
 * # 为什么先复制到临时目录
 *
 * `integrity.txt` 不能出现在源码目录里（否则开发时核心会看到一份指向旧哈希的清单，
 * 表现为"改完代码插件就消失了"）。因此先把插件复制到临时目录，在那里生成清单与签名，
 * 再打包，最后删掉临时目录。
 *
 * # 签名
 *
 * 复用发布链路的同一套密钥与环境变量（`scripts/signer-env.mjs`）：
 * 与更新包、插件包共用一把 minisign 私钥。没配置私钥时**默认拒绝打包**——
 * 打出一个装不上的包（未签名会被安装器拒绝）比报错更浪费时间。
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, relative, resolve, sep } from "node:path";
import { gzipSync } from "node:zlib";

import { ROOT, signingEnv } from "./signer-env.mjs";

const INTEGRITY = "integrity.txt";
const INTEGRITY_SIG = "integrity.sig";
/** 打包时忽略的目录/文件（不该进包）。 */
const SKIP = new Set([".git", "node_modules", "target", "dist", INTEGRITY, INTEGRITY_SIG]);

const args = process.argv.slice(2);
const source = args.find((a) => !a.startsWith("--"));
const outDir = valueOf("--out") ?? join(ROOT, "dist-plugins");
const wantSign = !args.includes("--no-sign");

function valueOf(name) {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}

function fail(message) {
  console.error(`[pack-plugin] ${message}`);
  process.exit(1);
}

/** 递归列出文件，返回相对于插件目录的 POSIX 风格路径。 */
function listFiles(dir, base = dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (SKIP.has(entry.name)) continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listFiles(full, base));
    } else if (entry.isFile()) {
      out.push(relative(base, full).split(sep).join("/"));
    }
  }
  return out;
}

/**
 * 写 `integrity.txt`：每行 `<sha256><两个空格><相对路径>`，按路径升序，LF。
 *
 * 排序不是美观问题：核心按清单逐条核对，格式（两个空格、LF）与顺序必须与
 * `plugin/trust.rs` 的解析完全一致，否则会判成"清单非法"而不是"文件被改"。
 */
function writeIntegrity(dir, files) {
  const lines = files
    .slice()
    .sort()
    .map((rel) => {
      const hash = createHash("sha256").update(readFileSync(join(dir, rel))).digest("hex");
      return `${hash}  ${rel}`;
    });
  writeFileSync(join(dir, INTEGRITY), `${lines.join("\n")}\n`, "utf8");
}

/** 调 Tauri CLI 签名（与发布链路同一份实现，见 `scripts/make-update.mjs`）。 */
function signIntegrity(dir, env) {
  const cli = join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js");
  if (!existsSync(cli)) {
    fail(`未找到 Tauri CLI（${cli}），请先执行 pnpm install`);
  }
  execFileSync(process.execPath, [cli, "signer", "sign", join(dir, INTEGRITY)], {
    cwd: ROOT,
    stdio: "inherit",
    env,
  });
  const produced = join(dir, `${INTEGRITY}.sig`);
  if (!existsSync(produced)) {
    fail("签名没有产出 .sig 文件");
  }
  renameSync(produced, join(dir, INTEGRITY_SIG));
}

/** 写一个 ustar 头（512 字节）。 */
function tarHeader({ name, size, mode = 0o644, mtime = 0 }) {
  if (Buffer.byteLength(name) > 100) {
    fail(`路径过长（tar 单段上限 100 字节）: ${name}`);
  }
  const header = Buffer.alloc(512, 0);
  header.write(name, 0, 100, "utf8");
  header.write(mode.toString(8).padStart(7, "0"), 100, 8, "utf8");
  header.write("0000000", 108, 8, "utf8"); // uid
  header.write("0000000", 116, 8, "utf8"); // gid
  header.write(size.toString(8).padStart(11, "0"), 124, 12, "utf8");
  header.write(mtime.toString(8).padStart(11, "0"), 136, 12, "utf8");
  header.write("        ", 148, 8, "utf8"); // 校验和先填空格
  header.write("0", 156, 1, "utf8"); // 普通文件
  header.write("ustar", 257, 6, "utf8");
  header.write("00", 263, 2, "utf8");
  const checksum = header.reduce((sum, byte) => sum + byte, 0);
  header.write(`${checksum.toString(8).padStart(6, "0")}\0 `, 148, 8, "utf8");
  return header;
}

/** 用最小 tar 写出器打包（不引入 tar 依赖）。 */
function buildArchive(dir, files) {
  const chunks = [];
  for (const rel of files.slice().sort()) {
    const body = readFileSync(join(dir, rel));
    chunks.push(tarHeader({ name: rel, size: body.length, mtime: 0 }));
    chunks.push(body);
    const pad = (512 - (body.length % 512)) % 512;
    if (pad > 0) chunks.push(Buffer.alloc(pad, 0));
  }
  // 两个全零块表示归档结束
  chunks.push(Buffer.alloc(1024, 0));
  return gzipSync(Buffer.concat(chunks), { level: 9 });
}

function main() {
  if (!source) {
    fail("用法: node scripts/pack-plugin.mjs <插件目录> [--out dist] [--no-sign]");
  }
  const pluginDir = resolve(source);
  if (!existsSync(join(pluginDir, "plugin.json"))) {
    fail(`${pluginDir} 里没有 plugin.json`);
  }
  const manifest = JSON.parse(readFileSync(join(pluginDir, "plugin.json"), "utf8"));
  for (const key of ["id", "version"]) {
    if (!manifest[key]) fail(`plugin.json 缺少 ${key}`);
  }

  const env = signingEnv();
  const keyConfigured = Boolean(
    (env.TAURI_SIGNING_PRIVATE_KEY_PATH || "").trim() ||
      (env.TAURI_SIGNING_PRIVATE_KEY || "").trim(),
  );
  if (wantSign && !keyConfigured) {
    fail(
      "未配置签名私钥（TAURI_SIGNING_PRIVATE_KEY[_PATH]）。\n" +
        "未签名的包会被安装器拒绝；确实要打调试包请加 --no-sign。",
    );
  }

  const stage = mkdtempSync(join(tmpdir(), "mclink-pack-"));
  try {
    cpSync(pluginDir, stage, { recursive: true });
    // 复制时可能带进旧清单，先清掉再重建
    rmSync(join(stage, INTEGRITY), { force: true });
    rmSync(join(stage, INTEGRITY_SIG), { force: true });

    const files = listFiles(stage);
    if (files.length === 0) fail("插件目录是空的");
    writeIntegrity(stage, files);
    if (wantSign) {
      signIntegrity(stage, env);
      files.push(INTEGRITY, INTEGRITY_SIG);
    } else {
      files.push(INTEGRITY);
    }

    const archive = buildArchive(stage, files);
    mkdirSync(outDir, { recursive: true });
    const target = join(outDir, `${manifest.id}-${manifest.version}.mclink-plugin`);
    writeFileSync(target, archive);

    const size = statSync(target).size;
    console.log(
      `[pack-plugin] ${manifest.id}@${manifest.version} → ${target}\n` +
        `  文件 ${files.length} 个，包体积 ${(size / 1024).toFixed(1)} KB，` +
        `签名 ${wantSign ? "已附带" : "未附带（--no-sign）"}`,
    );
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}

main();
