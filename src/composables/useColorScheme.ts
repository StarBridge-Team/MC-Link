import { ref, watch } from "vue";
import { generateM3Scheme, type M3Roles } from "../lib/api/m3";
import { useSettings } from "./useSettings";

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
    const key = `${state.theme_color}|${state.theme_variant}|${state.theme_contrast}|${dark ? "dark" : "light"}`;
    // 同一个输入不重复要一次（配色之外的其他设置改动也会触发本组合函数）
    if (key === lastKey) return;

    const mine = ++token;
    try {
      const scheme = await generateM3Scheme(
        state.theme_color,
        state.theme_variant,
        state.theme_contrast,
      );
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
      isDark.value,
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
