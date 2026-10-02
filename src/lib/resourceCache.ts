import { convertFileSrc } from "@tauri-apps/api/core";
import { reactive } from "vue";
import { getAssetUrl, prepareApp, readAssetText } from "./api/app";
import { absolutizeCssUrls } from "./cssAssets";
import type { AssetState, PrepareAppData } from "./api/types";

/**
 * 远程资源（字体、图标等）的统一加载与就绪状态。
 *
 * # 加载时机
 *
 * 统一在 `main.ts` 挂载前等一次，但**带有限超时**：
 *
 * - 后续启动（缓存命中）：后端只做版本比对 + 逐文件哈希校验，毫秒级，几乎不会超时；
 * - 首次运行（缓存为空）：可能要下载一两秒，超时上限内没完成就先按降级渲染，
 *   后台继续下载，完成后把 CSS 补注入（注入函数自带 id 去重，不会重复插入）。
 *
 * # 为什么不直接阻塞到下载完成
 *
 * 远程资源是**增强**，不是渲染前提。若把"资源服务器慢/离线"变成白屏，
 * 对一个必须能离线使用的桌面应用是明确的倒退。所以：超时 → 降级渲染
 * （字体回落系统字体、图标暂时缺省）→ 后台补齐。
 */

/** 挂载前最多等多久。只在缓存冷启动时才会真正等到这个上限。 */
const ASSET_WAIT_MS = 2500;

export const resourceState = reactive({
  /** 全部资源是否就绪。 */
  ready: false,
  /** 是否处于降级渲染（超时或部分资源失败）。 */
  degraded: false,
  /** 是否离线降级（连不上资源服务器，用的是本地缓存）。 */
  offline: false,
  version: "",
  /** 失败原因，可直接展示给用户。 */
  failures: [] as string[],
  /** 未就绪的资源路径。 */
  missing: [] as string[],
});

let started = false;

/**
 * 等待资源就绪。必须在 `app.mount()` 之前调用，且只调用一次。
 * 无论成功、失败还是超时，本函数都会在 `ASSET_WAIT_MS` 内返回。
 */
export async function bootstrapAssets(): Promise<void> {
  if (started) return;
  started = true;

  const syncing = loadFromBackend();

  let timer: ReturnType<typeof setTimeout> | undefined;
  const timedOut = new Promise<"timeout">((resolve) => {
    timer = setTimeout(() => resolve("timeout"), ASSET_WAIT_MS);
  });

  const winner = await Promise.race([syncing.then(() => "done" as const), timedOut]);
  if (timer) clearTimeout(timer);

  if (winner === "timeout") {
    // 先渲染；后台那次同步完成时会把缺的 CSS 补上。
    resourceState.degraded = true;
    void syncing;
  } else {
    resourceState.degraded = !resourceState.ready;
  }
}

async function loadFromBackend(): Promise<void> {
  let data: PrepareAppData;
  try {
    data = await prepareApp();
  } catch (e) {
    resourceState.failures = [e instanceof Error ? e.message : String(e)];
    resourceState.degraded = true;
    return;
  }
  await applyManifest(data);
}

async function applyManifest(data: PrepareAppData): Promise<void> {
  const assets = data.assets ?? [];
  resourceState.version = data.asset_version ?? "";
  resourceState.offline = !!data.assets_offline;
  resourceState.failures = data.asset_failures ?? [];
  resourceState.ready = !!data.assets_ready;
  resourceState.missing = assets.filter((a) => !a.ready).map((a) => a.path);
  resourceState.degraded = !data.assets_ready;

  // 清单驱动：注入所有就绪的样式表。
  // 不硬编码资源名——服务器加一张新样式表，客户端无需改代码。
  for (const asset of assets) {
    if (asset.ready && asset.path.toLowerCase().endsWith(".css")) {
      await injectStylesheet(asset);
    }
  }
}

/**
 * 注入一张样式表。
 *
 * # 为什么不能直接用 `<link href={convertFileSrc(css)}>`
 *
 * `convertFileSrc` 会把**整条绝对路径百分号编码成单个路径段**（连 `/` 都编码成
 * `%2F`），浏览器从 URL 角度看，"目录"就是 asset 根。于是 CSS 内部的相对引用
 * `url("./material-symbols-rounded.woff2")` 会被解析成 `http://asset.localhost/material-symbols-rounded.woff2`
 * → 404 → 字体不加载 → 图标全变成豆腐块。
 *
 * 所以这里改为：取 CSS 原文 → 把相对 `url()` 重写成**绝对**的 convertFileSrc 地址
 * → 以 `<style>` 注入。字体地址仍然由「相对路径 + 清单」推导，不硬编码任何资源名。
 */
async function injectStylesheet(asset: AssetState): Promise<void> {
  const id = `mc-link-asset-${asset.path}`;
  if (document.getElementById(id)) return;
  try {
    const css = await readAssetText(asset.path);
    const rewritten = await absolutizeCssUrls(css, asset.path, async (assetRelPath) => {
      const local = await getAssetUrl(assetRelPath);
      return local ? convertFileSrc(local) : null;
    });
    const style = document.createElement("style");
    style.id = id;
    style.textContent = rewritten;
    document.head.appendChild(style);
  } catch {
    // 单个资源注入失败不影响其它资源；状态里已经记了 missing。
  }
}

// 地址换算逻辑在 `./cssAssets`（纯函数，可脱离运行环境直接验证）。
