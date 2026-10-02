#!/usr/bin/env node
/**
 * 从 GitHub 抓取贡献者与开放 Issues，产出资源服务器发布的两个静态 JSON。
 *
 *   <assets_server>/community/contributors.json
 *   <assets_server>/community/issues.json
 *
 * 客户端读取端见 `src-tauri/src/community.rs`（它**不直连 GitHub**）。
 *
 * # 为什么在 CI 抓，而不是在服务器上实时抓
 *
 * - GitHub Actions 的 runner 访问 GitHub 天然可达；而未认证的 GitHub API 只有
 *   60 次/小时**每 IP**，服务器实时抓还要自己管配额与缓存；
 * - 资源服务器保持"纯静态托管"：不用装 GitHub 令牌、不用为它改代码、不用重新部署。
 *
 * # 宁可失败也不要发布坏数据
 *
 * 抓取失败时**不覆盖**已发布的文件并让 CI 变红 —— 线上仍是上一次的可用数据。
 * 贡献者列表为空同样拒绝发布（仓库有提交却拿到 0 人，说明接口或鉴权出了问题，
 * 而不是"真的没人"）。
 *
 * 用法：
 *   node scripts/sync-community.mjs                 # 只写文件，不上传
 *   node scripts/sync-community.mjs --prod          # 写文件并上传到生产资源服务器
 *   GITHUB_TOKEN=xxx node scripts/sync-community.mjs --prod
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import http from "node:http";
import https from "node:https";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const OUT_DIR = join(ROOT, "assets-server", "Assets", "community");

const REPO = "StarBridge-Team/MC-Link";
const REPO_URL = `https://github.com/${REPO}`;
const API = "https://api.github.com";

const PROD_URL = "https://mclinkassets.xigo.top:54789";
const WITH_PROD = process.argv.includes("--prod");
const SERVER_URL =
  process.env.ASSET_SERVER_URL || (WITH_PROD ? PROD_URL : "http://localhost:54789");

const SKIP_UPLOAD = process.argv.includes("--no-upload");
const ALLOW_EMPTY = process.argv.includes("--allow-empty");

/** CI 里用自带的 GITHUB_TOKEN（5000 次/小时）；本地通常留空。 */
const GH_TOKEN = process.env.GITHUB_TOKEN || process.env.GH_TOKEN || "";

/** 上传令牌：与 sync-assets.mjs 同一套约定（环境变量优先，否则读本地文件）。 */
function localUploadToken() {
  try {
    return readFileSync(join(ROOT, ".tauri", "asset-upload-token.txt"), "utf8").trim();
  } catch {
    return "";
  }
}
const UPLOAD_TOKEN = process.env.ASSET_UPLOAD_TOKEN || localUploadToken();

function request(url, { method = "GET", headers = {}, body = null } = {}) {
  return new Promise((resolve, reject) => {
    const u = new URL(url);
    const lib = u.protocol === "https:" ? https : http;
    const req = lib.request(
      { method, hostname: u.hostname, port: u.port, path: u.pathname + u.search, headers },
      (res) => {
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () => resolve({ status: res.statusCode, body: Buffer.concat(chunks) }));
      },
    );
    req.on("error", reject);
    if (body) req.write(body);
    req.end();
  });
}

async function ghGet(path) {
  const headers = {
    // GitHub 强制要求 User-Agent，缺了直接 403
    "User-Agent": "MC-Link-community-sync",
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (GH_TOKEN) headers.Authorization = `Bearer ${GH_TOKEN}`;

  const res = await request(`${API}${path}`, { headers });
  if (res.status < 200 || res.status >= 300) {
    const remaining = res.body.toString("utf8").slice(0, 200);
    throw new Error(`GitHub ${path} 返回 ${res.status}：${remaining}`);
  }
  return JSON.parse(res.body.toString("utf8"));
}

function isBot(login) {
  return (login ?? "").endsWith("[bot]");
}

/** 只保留界面要用的字段：GitHub 的原始对象里带了 issue 正文等大量内容。 */
function shapeContributor(c) {
  return {
    login: c.login ?? "",
    avatar_url: c.avatar_url ?? "",
    html_url: c.html_url ?? "",
    contributions: c.contributions ?? 0,
  };
}

function shapeIssue(i) {
  return {
    number: i.number,
    title: i.title ?? "",
    state: i.state ?? "open",
    html_url: i.html_url ?? "",
    created_at: i.created_at ?? "",
    updated_at: i.updated_at ?? "",
    comments: i.comments ?? 0,
    labels: (i.labels ?? []).map((l) => ({ name: l.name ?? "", color: l.color ?? "" })),
    user: i.user
      ? {
          login: i.user.login ?? "",
          avatar_url: i.user.avatar_url ?? "",
          html_url: i.user.html_url ?? "",
        }
      : null,
  };
}

async function upload(rel, buf) {
  const url = `${SERVER_URL}/upload?path=${encodeURIComponent(rel)}`;
  const res = await request(url, {
    method: "POST",
    headers: {
      // 令牌走 Authorization 头，不进 URL：URL 会进访问日志与 Referer
      Authorization: `Bearer ${UPLOAD_TOKEN}`,
      "Content-Type": "application/octet-stream",
      "Content-Length": buf.length,
    },
    body: buf,
  });
  const ok = res.status >= 200 && res.status < 300;
  console.log(
    `${ok ? "  ok  " : "  !!  "}${rel} -> HTTP ${res.status}${ok ? "" : ` ${res.body.toString("utf8").slice(0, 120)}`}`,
  );
  return ok;
}

async function main() {
  const [rawContributors, rawIssues] = await Promise.all([
    ghGet(`/repos/${REPO}/contributors?per_page=100`),
    ghGet(`/repos/${REPO}/issues?state=open&per_page=50&sort=updated`),
  ]);

  const contributors = rawContributors
    .filter((c) => !isBot(c.login))
    .map(shapeContributor)
    .sort((a, b) => b.contributions - a.contributions || a.login.localeCompare(b.login));

  if (contributors.length === 0 && !ALLOW_EMPTY) {
    console.error(
      "[sync-community] 贡献者列表为空 —— 拒绝发布（仓库有提交却拿到 0 人，多半是接口或鉴权出了问题，" +
        "发布出去会把关于页清空）。确需发布空列表请加 --allow-empty。",
    );
    process.exit(1);
  }

  // PR 混在 /issues 里，必须滤掉，否则"社区反馈"里会出现代码 PR
  const issues = rawIssues
    .filter((i) => !i.pull_request && !isBot(i.user?.login))
    .map(shapeIssue)
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));

  const stamp = new Date().toISOString();
  const doc = (key, list) =>
    JSON.stringify({ repo: REPO, repo_url: REPO_URL, updated_at: stamp, [key]: list }, null, 2) +
    "\n";

  mkdirSync(OUT_DIR, { recursive: true });
  writeFileSync(join(OUT_DIR, "contributors.json"), doc("contributors", contributors), "utf8");
  writeFileSync(join(OUT_DIR, "issues.json"), doc("issues", issues), "utf8");

  console.log(
    `[sync-community] 贡献者 ${contributors.length} 人 / 开放 Issues ${issues.length} 条` +
      `（PR 与机器人已滤掉）→ ${OUT_DIR}`,
  );
  console.log(`[sync-community] ${GH_TOKEN ? "已用 GITHUB_TOKEN" : "未认证请求（配额 60 次/小时）"}`);

  if (SKIP_UPLOAD) {
    console.log("[sync-community] 已跳过上传（--no-upload）");
    return;
  }
  if (!UPLOAD_TOKEN) {
    console.error(
      "[sync-community] 缺少上传令牌：请设 ASSET_UPLOAD_TOKEN，或把令牌放在 .tauri/asset-upload-token.txt",
    );
    process.exit(1);
  }

  console.log(`[sync-community] 上传到 ${SERVER_URL}`);
  let failed = 0;
  for (const name of ["contributors.json", "issues.json"]) {
    const ok = await upload(`community/${name}`, readFileSync(join(OUT_DIR, name)));
    if (!ok) failed++;
  }
  if (failed > 0) {
    console.error(`[sync-community] 上传完成，但有 ${failed} 个失败`);
    process.exit(1);
  }
  console.log("[sync-community] 上传完成");
}

main().catch((err) => {
  // 抓取失败：不覆盖已发布的文件，直接让 CI 变红（线上仍是上一次的可用数据）
  console.error(`[sync-community] 失败：${err.message}`);
  process.exit(1);
});
