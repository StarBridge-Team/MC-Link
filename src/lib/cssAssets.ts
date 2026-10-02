/**
 * 远程 CSS 里的资源地址改写。
 *
 * # 为什么必须改写
 *
 * 通过 asset 协议加载的 CSS，其内部相对 `url()` 会被解析失败：`convertFileSrc` 会把
 * **整条绝对路径百分号编码成单个路径段**（连 `/` 都编码成 `%2F`），浏览器从 URL 角度
 * 看"目录"就是 asset 根，于是 `url("./material-symbols-rounded.woff2")` 落到
 * `http://asset.localhost/material-symbols-rounded.woff2` → 404 → 字体不加载 → 图标全变豆腐块。
 *
 * 解决办法是把每个相对引用换算成它在 Assets 下的相对路径，再交由调用方转成绝对地址。
 *
 * 本文件**刻意保持纯函数**（不 import Tauri、不 import Vue）：地址换算逻辑最容易写错，
 * 而只有纯函数才能脱离运行环境直接用真实 CSS 文件验证。
 */

/** 匹配 `url(...)`，兼容带引号与不带引号两种写法。 */
const CSS_URL = /url\(\s*(['"]?)([^'")]+)\1\s*\)/g;

/** 已经是绝对地址或内联数据，保持原样。 */
export function isAbsoluteRef(value: string): boolean {
  return (
    value.startsWith("data:") ||
    value.startsWith("blob:") ||
    value.startsWith("http:") ||
    value.startsWith("https:") ||
    value.startsWith("asset:") ||
    value.startsWith("//") ||
    value.startsWith("/")
  );
}

/**
 * 按 CSS 自身所在目录，把引用换算成 Assets 下的相对路径。
 *
 * 例：CSS 在 `material-symbols/material-symbols.css`，引用 `./x.woff2`
 * → `material-symbols/x.woff2`；引用 `../fonts/y.css` → `fonts/y.css`。
 */
export function resolveAgainst(cssPath: string, ref: string): string {
  // 去掉 ?query 与 #fragment，它们不参与文件定位
  const clean = ref.split("?")[0].split("#")[0];
  const dir = cssPath.includes("/") ? cssPath.slice(0, cssPath.lastIndexOf("/")) : "";
  const parts = (dir ? `${dir}/${clean}` : clean).split("/");

  const out: string[] = [];
  for (const part of parts) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      out.pop();
      continue;
    }
    out.push(part);
  }
  return out.join("/");
}

/** 收集 CSS 里所有需要改写的相对引用。 */
export function collectRelativeRefs(css: string): string[] {
  const refs = new Set<string>();
  for (const match of css.matchAll(CSS_URL)) {
    const value = match[2].trim();
    if (!isAbsoluteRef(value)) refs.add(value);
  }
  return [...refs];
}

/**
 * 把 CSS 中的相对 `url()` 全部替换成绝对地址。
 *
 * `toAbsolute` 由调用方注入：接收 Assets 下的相对路径，返回可直接放进 CSS 的绝对 URL；
 * 返回 `null` 表示换算不出来（例如该文件不在清单里），此时**保留原样**，不抛错——
 * 单个资源缺失不应该让整张样式表失效。
 */
export async function absolutizeCssUrls(
  css: string,
  cssPath: string,
  toAbsolute: (assetRelPath: string) => Promise<string | null>,
): Promise<string> {
  const refs = collectRelativeRefs(css);
  if (refs.length === 0) return css;

  const resolved = new Map<string, string>();
  for (const ref of refs) {
    try {
      const abs = await toAbsolute(resolveAgainst(cssPath, ref));
      if (abs) resolved.set(ref, abs);
    } catch {
      // 换算失败就保持原样
    }
  }

  return css.replace(CSS_URL, (whole, _quote, value) => {
    const abs = resolved.get(value.trim());
    return abs ? `url("${abs}")` : whole;
  });
}
