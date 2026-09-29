import { convertFileSrc } from "@tauri-apps/api/core";
import { getAssetUrl, getAssetsServerUrl } from "./api/app";

/**
 * 启动时加载 Bootstrap Icons 与 Poppins 字体 CSS。
 *
 * - dev 模式：直接从 assets-server 加载（http URL），CSS 内相对路径正确解析到服务器
 * - prod 模式：从本地缓存加载（convertFileSrc 转 asset URL），CSS 内字体需绝对路径
 *
 * CSS 内部通过相对路径 `url("./xxx.woff2")` 引用字体文件，
 * dev 下浏览器基于 http URL 同源解析到 assets-server。
 */
export async function loadAssets(): Promise<void> {
  if (import.meta.env.DEV) {
    // dev：直接从 assets-server 加载，相对路径自动解析
    const base = await getAssetsServerUrl();
    if (base) {
      injectStylesheet("mc-link-bi", `${base}/bootstrap-icons/bootstrap-icons.css`);
      injectStylesheet("mc-link-fonts", `${base}/fonts/poppins.css`);
    }
    return;
  }

  // prod：从本地缓存加载
  try {
    const biCssPath = await getAssetUrl("bootstrap-icons/bootstrap-icons.css");
    if (biCssPath) {
      injectStylesheet("mc-link-bi", convertFileSrc(biCssPath));
    }
  } catch {
    /* Bootstrap Icons 未就绪，忽略 */
  }

  try {
    const fontsCssPath = await getAssetUrl("fonts/poppins.css");
    if (fontsCssPath) {
      injectStylesheet("mc-link-fonts", convertFileSrc(fontsCssPath));
    }
  } catch {
    /* Poppins 字体未就绪，忽略 */
  }
}

function injectStylesheet(id: string, href: string): void {
  if (!href || document.getElementById(id)) return;
  const link = document.createElement("link");
  link.id = id;
  link.rel = "stylesheet";
  link.href = href;
  document.head.appendChild(link);
}
