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
  if (!hasCommand("flatpak")) {
    console.error(
      "[flatpak-repo] 未找到 flatpak 命令。\n" +
        "  安装：Debian/Ubuntu `sudo apt install flatpak`；Arch `sudo pacman -S flatpak`；\n" +
        "        Fedora `sudo dnf install flatpak`。",
    );
    process.exit(1);
  }

  const skipBundle = flag("--no-bundle");
  const bundle = skipBundle ? null : findBundle();

  mkdirSync(REPO_DIR, { recursive: true });
  const gpgKey = (process.env.FLATPAK_GPG_KEY || "").trim();

  if (!gpgKey) {
    console.warn(
      "[flatpak-repo] 未设置 FLATPAK_GPG_KEY：将产出**未签名**仓库。\n" +
        "  未签名仓库无法作为正常 remote 被用户导入（Flatpak 默认要求 GPG 校验），\n" +
        "  这里只是让流程能走通并留痕，正式发布必须配置签名密钥。",
    );
  }

  if (bundle) {
    // 先把 bundle 拷进 incoming：`build-update-repo`/`build-import-bundle` 之后
    // 我们仍需要保留原始 bundle 供用户直接 `flatpak install`。
    mkdirSync(INCOMING_DIR, { recursive: true });
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
    // 让 Flatpak 客户端能取到用于校验的 GPG 公钥
    updateArgs.push(`--gpg-import=${gpgKey}`);
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
    // 公钥正文由发布者导出后填在这里；这里只标注 key id，CI 里若提供
    // FLATPAK_GPG_PUBLIC_KEY（ASCII armor 多行）则直接内联。
    const pub = (process.env.FLATPAK_GPG_PUBLIC_KEY || "").trim();
    if (pub) {
      lines.push(`GPGKey=${pub.split("\n").join("\\n")}`);
    } else {
      lines.push(`# 注意：未提供 FLATPAK_GPG_PUBLIC_KEY，客户端无法自动导入公钥。`);
      lines.push(`#      请把 GPG 公钥（ASCII armor）填到下面的 GPGKey=`);
      lines.push(`GPGKey=`);
    }
    lines.push(`DeployCollectionID=${APP_ID}`);
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
