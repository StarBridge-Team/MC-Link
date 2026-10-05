import { ref, watch } from "vue";
import { extractBackgroundSeed, generateM3Scheme, type M3Roles } from "../lib/api/m3";
import { isRemoteUrl } from "../lib/appearance/background";
import { remoteBackground, useSettings } from "./useSettings";

/**
 * 把动态配色接到后端（`generate_m3_scheme`）。
 *
 * # 分工
 *
 * - **后端**：由种子色 / 变体 / 对比度算出 48 个颜色角色（明暗两套）。
 * - **这里**：把当前该用的那一套**内联**写成 `--md-sys-color-*` 变量。
 * - **`<m3e-theme>`**：入参被钉死（见 `App.vue`），只负责它独有的部分（动效等），
 *   不再随配色变化重算 —— 那一步才是卡顿来源。
 *
 * # 为什么变量必须写在 `<m3e-theme>` 元素上
 *
 * `tokens.css` 里的角色别名（`--primary: var(--md-sys-color-primary)`）是声明在
 * **`m3e-theme` 选择器**上的。CSS 自定义属性的 `var()` 在**声明它的元素**上求值，
 * 所以把变量写到更深的元素（比如 `.shell`）不会影响这些别名 —— 会得到一半新色、
 * 一半旧色。内联样式优先于样式表，因此写在这个元素上既生效、又盖得住它自己的值。
 */
export function useColorScheme() {
  const { state, isDark } = useSettings();

  /** 后端配色是否生效。false = 已回退（调用方应把 m3e 的入参还原成真实设置）。 */
  const active = ref(false);

  /** 取色器拖动时的合并窗口：太短会每个事件一次 IPC，太长会显得不跟手。 */
  const DEBOUNCE_MS = 90;

  const VAR_PREFIX = "--md-sys-color-";

  let target: HTMLElement | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let lastKey = "";
  let token = 0;

  /**
   * 背景图提取出的种子色缓存，键是文件名。
   *
   * 取色要读文件、解码、量化，比 `generate_m3_scheme` 贵得多；而配色变体、对比度、
   * 明暗切换都会触发重算，这些都不该让图片被重新解一遍。
   */
  const seedCache = new Map<string, string>();

  /** 最近一次取色失败的图片名，避免同一张坏图被反复重试（每次都要走一遍解码）。 */
  let failedFor = "";

  /**
   * 决定本次配色的种子色。
   *
   * 两条来源，取色成本完全不同：
   *
   * - **网络背景**：种子色是后端在下载时顺手提取的（`remoteBackground().seed`），
   *   这里直接取用，**不要再发一次 IPC**（那等于把同一张图再解码一遍）。
   * - **本地背景**：需要调 `extract_background_seed` 现取，代价高，所以按文件名缓存。
   *
   * 其余情况与取色失败一律回退到手选 `theme_color`——**不返回空**：
   * 没有种子色就没有主题色，那比"用了手选色"更糟。
   */
  async function resolveSeed(): Promise<string> {
    const fallback = state.theme_color;
    if (!state.theme_from_background) return fallback;
    if (state.background_type !== "image" || !state.background_value) return fallback;

    const source = state.background_value;

    // 网络背景：种子色随下载结果一起来，直接复用。
    if (isRemoteUrl(source)) {
      const remote = remoteBackground();
      // URL 不匹配说明下载还没完成（或失败），此刻先用手选色；
      // 下载完成后 `useSettings` 会更新它，进而触发下面的 watch 重算。
      if (remote.value?.url !== source) return fallback;
      return remote.value.seed ?? fallback;
    }

    const cached = seedCache.get(source);
    if (cached) return cached;
    if (failedFor === source) return fallback;

    try {
      const seed = await extractBackgroundSeed(source);
      seedCache.set(source, seed);
      failedFor = "";
      return seed;
    } catch (e) {
      // 记下这张图已经失败过：动画/居中/遮罩等无关设置变动时不再重复尝试解码。
      failedFor = source;
      console.warn("[color] 背景图取色失败，回退到手选主题色:", e);
      return fallback;
    }
  }

  function writeRoles(el: HTMLElement, roles: M3Roles) {
    for (const [field, value] of Object.entries(roles)) {
      if (typeof value === "string" && value) {
        el.style.setProperty(VAR_PREFIX + field.replace(/_/g, "-"), value);
      }
    }
  }

  async function run() {
    const el = target;
    if (!el) return;

    const dark = isDark.value;
    // 跟随背景图时，种子实际来自图片，key 必须带上图片来源——否则"换一张背景图"
    // 会因为 `theme_color` 没变而被这行判断挡住，配色纹丝不动。
    const sourceKey = state.theme_from_background
      ? `bg:${state.background_value}`
      : `seed:${state.theme_color}`;
    const key = `${sourceKey}|${state.theme_variant}|${state.theme_contrast}|${dark ? "dark" : "light"}`;
    // 同一个输入不重复要一次（配色之外的其他设置改动也会触发本组合函数）
    if (key === lastKey) return;

    const mine = ++token;
    try {
      const seed = await resolveSeed();
      // 取色期间用户又改了设置：丢弃这次结果，别让旧种子盖住新值。
      if (mine !== token || !target) return;

      const scheme = await generateM3Scheme(seed, state.theme_variant, state.theme_contrast);
      // 期间用户又改了配色：丢弃这次结果，避免旧值盖住新值
      if (mine !== token || !target) return;
      writeRoles(el, dark ? scheme.dark : scheme.light);
      lastKey = key;
      active.value = true;
    } catch (e) {
      // 后端不可用（浏览器预览、命令失败）：把控制权交回 m3e 自己算。
      // 颜色仍然正确，只是会慢 —— 比"整站没有配色"好得多。
      if (mine !== token) return;
      active.value = false;
      console.warn("[color] 后端配色不可用，已回退到 m3e 计算:", e);
    }
  }

  function schedule(immediate = false) {
    if (!target) return;
    if (timer) clearTimeout(timer);
    if (immediate) {
      timer = null;
      void run();
      return;
    }
    timer = setTimeout(() => {
      timer = null;
      void run();
    }, DEBOUNCE_MS);
  }

  watch(
    () => [
      state.theme_color,
      state.theme_variant,
      state.theme_contrast,
      state.theme_from_background,
      // 换背景图 / 换背景类型都要重算：前者换种子，后者可能让"跟随"失效。
      state.background_type,
      state.background_value,
      isDark.value,
      // 网络背景的下载结果。**这一项不能少**：URL 下载完成前 `resolveSeed` 只能拿到
      // 手选色（此时 `remoteBackground().url` 还没对上），下载完成后若不重算，
      // 配色就会一直停在手选色上，看起来像"跟随背景图没生效"。
      remoteBackground().value?.url,
    ],
    () => schedule(),
  );

  return {
    active,
    /** 绑定 `<m3e-theme>` 元素（在 `onMounted` 里调用一次）。 */
    attach(el: HTMLElement | null) {
      target = el;
      // 换了目标元素就要重算一遍（入参没变时上面的 key 判断会挡住，所以清空它）
      lastKey = "";
      schedule(true);
    },
  };
}
