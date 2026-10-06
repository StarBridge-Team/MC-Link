#!/usr/bin/env node
/**
 * 把 `.flatpak` bundle 导入为 **OSTree 仓库**，供 `flatpak update` 使用。
 * ------------------------------------------------------------------
 * # 为什么需要它
 *
 * 单文件 `.flatpak` 只能 `flatpak install`，装完之后**不再可更新**——
 * Flatpak 只认仓库（remote）。要让用户能用 `flatpak update` 拿到新版本，
 * 必须把每个版本的构建成果导入一个持续的 OSTree 仓库并部署到远端。
 *
 * 本脚本负责"导入 + 导出"这一段（纯本地、可离线验证）；上传交给
 * `sync-assets.mjs`（沿用既有的资源服务器上传通道）。
 *
 * # 产物
 *
 *   assets-server/Assets/flatpak/repo/        增量仓库（每次发布会变大）
 *   assets-server/Assets/flatpak/com.cugo.mc-link.flatpakrepo   给用户导入的 remote 描述
 *
 * 用户可以：
 *   flatpak remote-add --user --if-not-exists mclink <flatpakrepo 的 URL>
 *   flatpak install --user mclink com.cugo.mc-link
 *   flatpak update --user com.cugo.mc-link      # 之后就是常规更新
 *
 * # 签名
 *
 * 生产环境**必须**用 GPG 给仓库签名（`--gpg-sign`），否则 Flatpak 会拒绝
 * 从非 Flathub 的 remote 安装（除非用户显式 `--no-gpg-verify`，那不该被引导）。
 * 没有配置签名密钥时本脚本**只产出未签名的仓库并明确告警**，
 * 不会静默产出一个用户装不上的东西。
 *
 * 用法：
 *   node scripts/flatpak-repo.mjs [--bundle <path>] [--no-bundle]
 * 环境变量：
 *   FLATPAK_GPG_KEY   GPG 密钥 ID 或指纹（不设则产出未签名仓库并告警）
 */
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync, statSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const APP_ID = "com.cugo.mc-link";
const FLATPAK_DIR = join(ROOT, "assets-server", "Assets", "flatpak");
const REPO_DIR = join(FLATPAK_DIR, "repo");

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const valueOf = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};

const version = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")).version;

/** 在 PATH 里找一个可执行文件（跨平台）。 */
function hasCommand(cmd) {
  try {
    execFileSync(cmd, ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function run(cmd, cmdArgs) {
  console.log(`[flatpak-repo] $ ${cmd} ${cmdArgs.join(" ")}`);
  execFileSync(cmd, cmdArgs, { cwd: ROOT, stdio: "inherit" });
}

/**
 * 确保 `REPO_DIR` 是一个**已初始化的 OSTree 仓库**。
 *
 * # 为什么必须显式做这一步
 *
 * `flatpak build-import-bundle` **不会**在目标不存在时自动建仓库
 * （`build-export` 会，所以这里很容易误以为它也会）。只 `mkdir` 出目录的话，
 * 导入时会直接失败：
 *
 *     error: opening repo: opendir(objects): No such file or directory
 *
 * 判断依据是 OSTree 仓库的标志：`objects/` 子目录 + `config` 文件都存在。
 * 已存在就跳过（幂等），这样重复发布只是往同一个仓库追加版本 ——
 * 这也正是"用户可以 flatpak update"的前提。
 */
function ensureOstreeRepo() {
  const looksInitialized =
    existsSync(join(REPO_DIR, "objects")) && existsSync(join(REPO_DIR, "config"));
  if (looksInitialized) {
    console.log("[flatpak-repo] 仓库已存在，将追加新版本");
    return;
  }

  // `--mode=archive`：Flatpak 本地/HTTP 分发仓库用的就是 archive 模式
  // （bundle 里的 delta 也是针对它生成的）。用 bare 模式会导致
  // 客户端取不到对象。
  console.log(`[flatpak-repo] 初始化 OSTree 仓库: ${REPO_DIR}`);
  run("ostree", ["init", `--repo=${REPO_DIR}`, "--mode=archive"]);
}

/**
 * 把 ASCII-armored 的 PGP 公钥转成 `.flatpakrepo` 要求的**单行 base64**。
 *
 * `flatpakrepo(5)` 对 `GPGKey` 的说明是 "The base64-encoded gpg key for the remote"，
 * 官方示例就是一长串单行 base64（即 armor 的正文去掉头尾与换行）。
 * `.flatpakrepo` 是 ini 格式、值不能跨行，所以必须压成一行。
 *
 * 输入不是 armor 形态（没找到 BEGIN 标记）时返回空串，由调用方决定怎么告警 ——
 * 静默写入一个格式不对的值会让用户在 `remote-add` 时才失败。
 */
function armoredToSingleLineBase64(armored) {
  if (!armored) return "";
  const m = armored.match(
    /-----BEGIN PGP PUBLIC KEY BLOCK-----([\s\S]*?)-----END PGP PUBLIC KEY BLOCK-----/,
  );
  const body = m ? m[1] : armored;
  // 去掉所有空白：armor 正文本来就是 base64，换行只是排版
  const oneLine = body.replace(/\s+/g, "");
  if (!oneLine) return "";
  // 粗校验：base64 字符集 + 长度是 4 的倍数（不满足说明输入不是公钥）
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(oneLine) || oneLine.length % 4 !== 0) {
    return "";
  }
  return oneLine;
}

/**
 * 找到要导入的 bundle。
 *
 * 优先用显式 `--bundle`；否则在 `build-aux/flatpak/` 与 `assets-server/Assets/flatpak/`
 * 下找最新的 `.flatpak`（CI 会由 flatpak-builder action 产出到前者）。
 */
function findBundle() {
  const explicit = valueOf("--bundle");
  if (explicit) {
    const p = resolve(ROOT, explicit);
    if (!existsSync(p)) throw new Error(`指定的 bundle 不存在: ${p}`);
    return p;
  }

  const candidates = [];
  for (const dir of [
    join(ROOT, "build-aux", "flatpak"),
    FLATPAK_DIR,
    join(ROOT, "src-tauri", "target", "release", "bundle", "flatpak"),
  ]) {
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir)) {
      if (name.endsWith(".flatpak")) candidates.push(join(dir, name));
    }
  }
  if (candidates.length === 0) return null;

  // 取**最新写入**的一个：构建目录里可能残留历次产物，体积大小并不可靠
  // （不同版本的压缩率差别可能盖过"谁更新"）。
  candidates.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return candidates[0];
}

function main() {
  // 两个都要：flatpak 负责导入与刷新摘要，ostree 负责首次初始化仓库
  // （build-import-bundle 不会自动建仓库，见 `ensureOstreeRepo`）。
  // Debian/Ubuntu 上 flatpak 包会带 ostree 依赖，但仍显式检查 ——
  // 缺了它报错信息会指向内部命令而不是"环境缺东西"。
  const missing = ["flatpak", "ostree"].filter((c) => !hasCommand(c));
  if (missing.length > 0) {
    console.error(
      `[flatpak-repo] 未找到命令: ${missing.join(", ")}。\n` +
        "  安装：Debian/Ubuntu `sudo apt install flatpak ostree`；\n" +
        "        Arch `sudo pacman -S flatpak ostree`；\n" +
        "        Fedora `sudo dnf install flatpak ostree`。",
    );
    process.exit(1);
  }

  const skipBundle = flag("--no-bundle");
  const bundle = skipBundle ? null : findBundle();

  mkdirSync(REPO_DIR, { recursive: true });
  ensureOstreeRepo();
  const gpgKey = (process.env.FLATPAK_GPG_KEY || "").trim();

  if (!gpgKey) {
    console.warn(
      "[flatpak-repo] 未设置 FLATPAK_GPG_KEY：将产出**未签名**仓库。\n" +
        "  未签名仓库无法作为正常 remote 被用户导入（Flatpak 默认要求 GPG 校验），\n" +
        "  这里只是让流程能走通并留痕，正式发布必须配置签名密钥。",
    );
  }

  if (bundle) {
    // 直接把 bundle 导入仓库。原始 bundle 由调用方保管
    // （CI 里它已在 artifact 与 Assets/update/ 下各留了一份，供用户
    //  `flatpak install <file>`），这里不搬家。
    console.log(`[flatpak-repo] 导入 bundle: ${bundle}`);
    const importArgs = ["build-import-bundle", REPO_DIR, bundle];
    if (gpgKey) importArgs.push(`--gpg-sign=${gpgKey}`);
    run("flatpak", importArgs);
  } else {
    if (!existsSync(join(REPO_DIR, "config"))) {
      console.error(
        "[flatpak-repo] 没有找到 .flatpak bundle，且仓库尚不存在，无法继续。\n" +
          "  先构建：`pnpm build:release`（Linux 上会产出 deb），再跑 flatpak-builder。",
      );
      process.exit(1);
    }
    console.log("[flatpak-repo] --no-bundle：仅刷新仓库元数据");
  }

  // 刷新仓库摘要（appstream / summary / 索引）
  const updateArgs = ["build-update-repo", REPO_DIR];
  if (gpgKey) {
    updateArgs.push(`--gpg-sign=${gpgKey}`);
    // `--gpg-import` 收的是**公钥文件路径**，不是 key id
    // （见 flatpak 命令参考：build-update-repo 支持 --gpg-import=FILE。
    //  同一个"提供公钥"的功能在各子命令里选项名还不一样，
    //  build-bundle 是 --gpg-keys、install 是 --gpg-file，不能混用）。
    // 没有公钥文件就跳过这一步：只签名、不导入公钥时客户端仍可通过
    // `.flatpakrepo` 里的 GPGKey 字段拿到公钥（见下方生成逻辑）。
    const pubPath = (process.env.FLATPAK_GPG_PUBLIC_KEY_FILE || "").trim();
    if (pubPath) {
      if (!existsSync(pubPath)) {
        throw new Error(`FLATPAK_GPG_PUBLIC_KEY_FILE 指向的文件不存在: ${pubPath}`);
      }
      updateArgs.push(`--gpg-import=${pubPath}`);
    } else {
      console.warn(
        "[flatpak-repo] 未提供 FLATPAK_GPG_PUBLIC_KEY_FILE：跳过向仓库导入公钥" +
          "（仍会签名；客户端靠 .flatpakrepo 的 GPGKey 取公钥）。",
      );
    }
  }
  run("flatpak", updateArgs);

  // 生成 .flatpakrepo：用户一行命令就能加上 remote。
  // `DeployCollectionID` 与 GPGKey 让客户端知道该信谁；未签名时不写 GPGKey（否则必失败）。
  const repoUrl = (
    process.env.FLATPAK_REPO_URL ||
    "https://mclinkassets.xigo.top:54789/flatpak/repo"
  ).replace(/\/+$/, "");

  const lines = [
    "[Flatpak Repo]",
    `Title=MC Link`,
    `Url=${repoUrl}`,
    `Homepage=https://www.xigo.top/`,
    `Comment=MC Link 联机工具`,
    `Description=MC Link 的 Flatpak 更新源`,
  ];
  if (gpgKey) {
    // `GPGKey` 要的是**去掉 armor 头尾、合并成单行的 base64**（见 flatpakrepo(5)：
    // "The base64-encoded gpg key for the remote"）。
    // 直接内联 `-----BEGIN PGP PUBLIC KEY BLOCK-----` 那一段是错的 —— 它是多行，
    // 而 .flatpakrepo 是 ini 格式，值不能跨行。
    const armored = (process.env.FLATPAK_GPG_PUBLIC_KEY || "").trim();
    const b64 = armoredToSingleLineBase64(armored);
    if (b64) {
      lines.push(`GPGKey=${b64}`);
    } else {
      console.warn(
        "[flatpak-repo] 未提供可用的 FLATPAK_GPG_PUBLIC_KEY（需 ASCII armor 公钥）：\n" +
          "  生成的 .flatpakrepo 将没有 GPGKey，用户导入 remote 时会因无法校验而失败。\n" +
          "  导出：gpg --armor --export <keyid> > pub.asc",
      );
    }
    // `DeployCollectionID` 已弃用，用 `DeploySideloadCollectionID`
    // （Flatpak 1.12.8+ 才认后者；旧客户端忽略 collection id 仍可工作）。
    lines.push(`DeploySideloadCollectionID=${APP_ID}`);
  } else {
    lines.push(`# 未签名仓库：客户端需要 --no-gpg-verify 才能导入（不推荐）。`);
  }
  lines.push("");

  const repoFile = join(FLATPAK_DIR, `${APP_ID}.flatpakrepo`);
  writeFileSync(repoFile, lines.join("\n"), "utf8");
  console.log(`[flatpak-repo] 已生成 remote 描述: ${repoFile}`);

  // 记录本次导入的版本，供上传脚本与排查使用
  writeFileSync(
    join(FLATPAK_DIR, "VERSION"),
    `${version}\n`,
    "utf8",
  );

  console.log(
    `[flatpak-repo] 完成（版本 ${version}${gpgKey ? "，已签名" : "，**未签名**"}）。\n` +
      `  用户安装：flatpak install --user <bundle 路径>\n` +
      `  接入更新源：flatpak remote-add --user --if-not-exists mclink <flatpakrepo URL>\n` +
      `  之后更新：flatpak update --user ${APP_ID}`,
  );
}

try {
  main();
} catch (e) {
  console.error(`[flatpak-repo] 失败: ${e.message}`);
  process.exit(1);
}
