#!/usr/bin/env node
/**
 * 生成第三方开源许可声明（`legal/THIRD-PARTY.md` 与 `legal/third-party.json`）。
 *
 * # 为什么是"生成"而不是"手写"
 *
 * 依赖会变，而声明必须跟着变——手写的声明一定会过期，而过期的声明比没有更糟
 * （它会让"已履行义务"变成一句无法核实的话）。所以这里把三件事分开：
 *
 * - **自动收集的东西**：Rust 依赖（`cargo metadata`）与前端的**实际安装树**
 *   （遍历 `node_modules/.pnpm`，因此包含全部传递依赖，不只 package.json 里那几个）；
 * - **必须手工登记的东西**：那些"下载来直接分发、不在任何依赖清单里"的第三方件
 *   （见下面的 [`MANUAL`]），它们恰恰是最容易漏、且最需要履职的一类；
 * - **许可证正文**：放在 `legal/licenses/<SPDX>.txt`，由本脚本核对是否齐备并告警。
 *
 * 输出是**确定性**的：同样的依赖树产出逐字节相同的文件（不含生成时间戳），
 * 所以它只随依赖变化而变，不会每跑一次就产生无意义的 diff。
 *
 * 用法：
 *   node scripts/gen-licenses.mjs          # 生成
 *   node scripts/gen-licenses.mjs --check   # 只检查是否与当前依赖一致（CI 用）
 */
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const LEGAL_DIR = join(ROOT, "legal");
const LICENSE_TEXT_DIR = join(LEGAL_DIR, "licenses");
const MD_PATH = join(LEGAL_DIR, "THIRD-PARTY.md");
const JSON_PATH = join(LEGAL_DIR, "third-party.json");

/**
 * 手工登记的第三方件：**不在任何包管理器清单里，但会随我们分发**。
 *
 * 漏掉这一类的后果最严重——AGPL/GPL 的义务正是冲着"分发二进制"来的，
 * 而它们不会出现在 `cargo metadata` 或 `node_modules` 里。
 */
const MANUAL = [
  {
    name: "Terracotta（陶瓦联机）",
    version: "0.4.2",
    license: "AGPL-3.0-or-later",
    homepage: "https://github.com/burningtnt/Terracotta",
    note:
      "以**未经修改**的二进制整包分发，并通过其进程间通信接口调用，未静态或动态链接。" +
      "依其 AGPL 例外条款，本程序不因此被 AGPL 覆盖；但须在界面明显处标注其版权信息，并随附其许可证全文。",
  },
];

/// 需要一并收集的 Rust 清单（路径相对仓库根）。
const RUST_MANIFESTS = ["src-tauri/Cargo.toml", "assets-server/Cargo.toml"];

/** 从 SPDX 表达式里拆出单个许可证标识。 */
export function spdxIds(expression) {
  return String(expression)
    .replace(/[()]/g, " ")
    .split(/\s+(?:OR|AND|WITH)\s+|\//i)
    .map((part) => part.trim())
    .filter((part) => part && part !== "LicenseRef-proprietary");
}

/**
 * 许可证正文文件名。
 *
 * 去掉 `-only` / `-or-later` 后缀：正文本来就是同一份（`AGPL-3.0-or-later` 与
 * `AGPL-3.0` 用的是同一条款文本），拆成两个文件只会让人以为内容不同。
 */
export function licenseFileName(id) {
  return `${id.replace(/-(only|or-later)$/i, "")}.txt`;
}

/// Rust：`cargo metadata` 覆盖整棵解析后的依赖树（含传递依赖）。
function collectRust() {
  const found = new Map();
  for (const manifest of RUST_MANIFESTS) {
    const raw = execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--manifest-path", manifest],
      { cwd: ROOT, encoding: "utf8", maxBuffer: 512 * 1024 * 1024 }
    );
    for (const pkg of JSON.parse(raw).packages) {
      // 本仓库自己的包（path 依赖）没有来源，不该出现在第三方声明里
      if (!pkg.source) continue;
      const key = `${pkg.name}@${pkg.version}`;
      if (!found.has(key)) {
        found.set(key, {
          name: pkg.name,
          version: pkg.version,
          license: pkg.license || "(未声明)",
          homepage: pkg.homepage || pkg.repository || "",
          ecosystem: "rust",
        });
      }
    }
  }
  return [...found.values()];
}

/** 读一个包的版权行（MIT 一类要求保留版权声明，光列名字不够）。 */
function copyrightLine(pkgDir) {
  let entries;
  try {
    entries = readdirSync(pkgDir);
  } catch {
    return "";
  }
  const candidate = entries.find((name) => /^(licen[cs]e|copying|notice)/i.test(name));
  if (!candidate) return "";
  try {
    const text = readFileSync(join(pkgDir, candidate), "utf8");
    const line = text
      .split(/\r?\n/)
      .map((l) => l.trim())
      .find((l) => /copyright/i.test(l));
    return line ? line.slice(0, 200) : "";
  } catch {
    return "";
  }
}

/**
 * 前端：遍历 pnpm 的实际安装树（`.pnpm` 存储），因此**含全部传递依赖**。
 *
 * 只看 `package.json` 的直接依赖是不够的：Vite 会把传递依赖一并打进产物，
 * 那些代码同样是"被分发"的，声明里必须有它们。
 */
function collectFrontend() {
  const store = join(ROOT, "node_modules", ".pnpm");
  if (!existsSync(store)) return [];
  const found = new Map();
  for (const entry of readdirSync(store)) {
    const inner = join(store, entry, "node_modules");
    let names;
    try {
      names = readdirSync(inner);
    } catch {
      continue;
    }
    for (const name of names) {
      const dirs = name.startsWith("@")
        ? readdirSync(join(inner, name)).map((sub) => join(inner, name, sub))
        : [join(inner, name)];
      for (const pkgDir of dirs) {
        const pkgJson = join(pkgDir, "package.json");
        if (!existsSync(pkgJson)) continue;
        let meta;
        try {
          meta = JSON.parse(readFileSync(pkgJson, "utf8"));
        } catch {
          continue;
        }
        const key = `${meta.name}@${meta.version}`;
        if (found.has(key)) continue;
        const license =
          meta.license ||
          (Array.isArray(meta.licenses) ? meta.licenses.map((l) => l.type).join(" OR ") : "") ||
          "(未声明)";
        found.set(key, {
          name: meta.name,
          version: meta.version,
          license,
          homepage: meta.homepage || "",
          copyright: copyrightLine(pkgDir),
          ecosystem: "node",
        });
      }
    }
  }
  return [...found.values()];
}

function byNameThenVersion(a, b) {
  return a.name.localeCompare(b.name) || String(a.version).localeCompare(String(b.version));
}

/** 收集全部第三方件（已去重、已排序）。 */
export function collect() {
  const all = new Map();
  const push = (item) => {
    const key = `${item.name}@${item.version}`;
    if (!all.has(key)) all.set(key, item);
  };
  for (const item of collectRust()) push(item);
  for (const item of collectFrontend()) push(item);
  for (const item of MANUAL) {
    push({ ...item, ecosystem: "binary", copyright: "" });
  }
  return [...all.values()].sort(byNameThenVersion);
}

/** 找出声明里用到、但 `legal/licenses/` 下缺正文的许可证。 */
export function missingLicenseTexts(packages) {
  const ids = new Set();
  for (const pkg of packages) for (const id of spdxIds(pkg.license)) ids.add(id);
  return [...ids].filter((id) => !existsSync(join(LICENSE_TEXT_DIR, licenseFileName(id)))).sort();
}

function renderMarkdown(packages, missing) {
  const lines = [];
  lines.push("<!-- 由 `node scripts/gen-licenses.mjs` 生成，请勿手工编辑。 -->");
  lines.push("# 第三方开源软件许可声明");
  lines.push("");
  lines.push(
    "本程序使用了下列第三方开源软件。文件由脚本自动生成（遍历 Rust 与前端依赖的实际安装树，"
  );
  lines.push(
    "外加手工登记的、随程序分发的第三方二进制），依赖变化时重新运行 `pnpm licenses` 即可。"
  );
  lines.push("");
  lines.push("## 随程序分发的第三方二进制");
  lines.push("");
  lines.push("| 名称 | 版本 | 许可证 | 出处 |");
  lines.push("|---|---|---|---|");
  for (const pkg of packages.filter((p) => p.ecosystem === "binary")) {
    lines.push(
      `| ${pkg.name} | ${pkg.version} | ${pkg.license} | ${pkg.homepage || "-"} |`
    );
  }
  lines.push("");
  for (const pkg of packages.filter((p) => p.ecosystem === "binary")) {
    lines.push(`> **${pkg.name}**：${pkg.note ?? ""}`);
    lines.push("");
  }
  lines.push("## 依赖清单");
  lines.push("");
  lines.push(`共 ${packages.length - packages.filter((p) => p.ecosystem === "binary").length} 个依赖包。`);
  lines.push("");
  lines.push("| 名称 | 版本 | 许可证 |");
  lines.push("|---|---|---|");
  for (const pkg of packages.filter((p) => p.ecosystem !== "binary")) {
    lines.push(`| ${pkg.name} | ${pkg.version} | ${pkg.license} |`);
  }
  lines.push("");
  lines.push("## 版权声明");
  lines.push("");
  lines.push("各依赖包附带的版权声明（MIT 一类要求保留）：");
  lines.push("");
  const withCopyright = packages.filter((p) => p.copyright);
  if (withCopyright.length === 0) {
    lines.push("（依赖包中未找到版权行）");
  } else {
    for (const pkg of withCopyright) {
      lines.push(`- ${pkg.name}@${pkg.version}：${pkg.copyright}`);
    }
  }
  lines.push("");
  lines.push("## 许可证全文");
  lines.push("");
  lines.push(
    "下列文本**内嵌在本文档里**，而不是只给一个链接：分发义务要求的是「随附许可证文本」，" +
      "指向网上的链接不能替代它。源文件同时保留在 `legal/licenses/` 下，便于单独查阅。"
  );
  lines.push("");
  if (missing.length > 0) {
    lines.push(`> 注意：有 ${missing.length} 份许可证正文缺失，需补齐后重新生成：${missing.join("、")}`);
    lines.push("");
  }
  const ids = [...new Set(packages.flatMap((p) => spdxIds(p.license)))].sort();
  for (const id of ids) {
    const file = join(LICENSE_TEXT_DIR, licenseFileName(id));
    lines.push(`### ${id}`);
    lines.push("");
    if (!existsSync(file)) {
      lines.push(`> **缺失**：请把正文放到 \`legal/licenses/${licenseFileName(id)}\` 后重新生成。`);
      lines.push("");
      continue;
    }
    lines.push("```text");
    lines.push(readFileSync(file, "utf8").trimEnd());
    lines.push("```");
    lines.push("");
  }
  lines.push("");
  lines.push("## 重新生成");
  lines.push("");
  lines.push("```bash");
  lines.push("pnpm licenses            # 生成（依赖变化后重跑）");
  lines.push("pnpm licenses:check      # 只检查是否与当前依赖一致（CI 用）");
  lines.push("```");
  lines.push("");
  return lines.join("\n");
}

/** 生成两份产物，返回它们的内容。 */
export function render() {
  const packages = collect();
  const missing = missingLicenseTexts(packages);
  return {
    packages,
    missing,
    markdown: renderMarkdown(packages, missing),
    json:
      JSON.stringify(
        {
          note: "由 scripts/gen-licenses.mjs 生成，请勿手工编辑。",
          binaries: packages.filter((p) => p.ecosystem === "binary"),
          packages: packages.filter((p) => p.ecosystem !== "binary"),
          licenseFiles: [
            ...new Set(packages.flatMap((p) => spdxIds(p.license))),
          ].sort(),
        },
        null,
        2
      ) + "\n",
  };
}

/** 生成并写入文件；`check` 为真时只比对不写入。 */
export function generate({ check = false } = {}) {
  const { markdown, json, packages, missing } = render();
  if (check) {
    const current = existsSync(MD_PATH) ? readFileSync(MD_PATH, "utf8") : "";
    if (current !== markdown) {
      console.error(
        "[licenses] legal/THIRD-PARTY.md 与当前依赖不一致，请运行：pnpm licenses"
      );
      return false;
    }
    console.log(`[licenses] 声明与依赖一致（${packages.length} 个包）`);
  } else {
    mkdirSync(LEGAL_DIR, { recursive: true });
    mkdirSync(LICENSE_TEXT_DIR, { recursive: true });
    writeFileSync(MD_PATH, markdown);
    writeFileSync(JSON_PATH, json);
    console.log(`[licenses] 已生成 legal/THIRD-PARTY.md（${packages.length} 个包）`);
  }

  if (missing.length > 0) {
    console.warn(
      `[licenses] 缺少 ${missing.length} 份许可证正文，请放到 legal/licenses/ 下：`
    );
    for (const id of missing) console.warn(`  - ${licenseFileName(id)}`);
  }
  return true;
}

// 作为脚本直接运行时的入口。
//
// 用 `pathToFileURL` 而不是手拼 `file://` + 替换反斜杠：Windows 上盘符的形式
// （`file://E:/…` 与 `file:///E:/…`）并不一致，手拼会让脚本**静默什么都不做**——
// 命令返回 0、没有任何输出，这类问题排查起来非常费时。
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const ok = generate({ check: process.argv.includes("--check") });
  if (!ok) process.exit(1);
}
