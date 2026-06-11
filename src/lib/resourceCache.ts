/**
 * 资源缓存管理模块（纯前端缓存，无网络请求）
 * 资源下载由 Rust 后端 prepare_app 命令处理
 */

// --- Google Fonts ---
const GFX_CACHE_KEY = 'resource_cache_v1';

interface ResourceCache {
  googleFontsCss?: string;
  cachedAt?: number;
}

function getGfxCache(): ResourceCache {
  try {
    const raw = localStorage.getItem(GFX_CACHE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function setGfxCache(data: ResourceCache): void {
  try {
    localStorage.setItem(GFX_CACHE_KEY, JSON.stringify(data));
  } catch { /* ignore */ }
}

export function injectGoogleFontsCss(css: string): void {
  if (!css) return;
  const id = 'mc-link-google-fonts';
  if (document.getElementById(id)) return;
  const style = document.createElement('style');
  style.id = id;
  style.textContent = css;
  document.head.appendChild(style);
}

export function tryInjectCachedFonts(): boolean {
  try {
    const cache = getGfxCache();
    if (cache.googleFontsCss) {
      injectGoogleFontsCss(cache.googleFontsCss);
      return true;
    }
  } catch { /* ignore */ }
  return false;
}

export function cacheFontsCss(css: string): void {
  setGfxCache({ ...getGfxCache(), googleFontsCss: css, cachedAt: Date.now() });
}

// --- Bootstrap Icons ---
const BI_CACHE_KEY = 'bi_css_v1';

export function injectBiCss(css: string): void {
  if (!css) return;
  const id = 'mc-link-bi';
  if (document.getElementById(id)) return;
  const style = document.createElement('style');
  style.id = id;
  style.textContent = css;
  document.head.appendChild(style);
}

export function tryInjectCachedIcons(): boolean {
  try {
    const css = localStorage.getItem(BI_CACHE_KEY);
    if (css) {
      injectBiCss(css);
      return true;
    }
  } catch { /* ignore */ }
  return false;
}

export function cacheBiCss(css: string): void {
  localStorage.setItem(BI_CACHE_KEY, css);
}

/** 检查所有资源是否已缓存 */
export function isFullyCached(): boolean {
  try {
    const gfx = getGfxCache();
    const bi = localStorage.getItem(BI_CACHE_KEY);
    return !!gfx.googleFontsCss && !!bi;
  } catch {
    return false;
  }
}