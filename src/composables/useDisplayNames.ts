import { useI18n } from "vue-i18n";

/**
 * 语言 / 地区代码 → 显示名。
 *
 * # 为什么地区名走 i18n、语言名走常量表
 *
 * 地区清单由后端维护（`setup.rs` 的 `SUPPORTED_REGIONS`），而且**会随版本增加**；
 * 因此地区名放在 locale 文件里，取不到时回退成原始代码——后端加地区不会让界面崩。
 * 语言清单同理由后端维护，但语言显示名是"语言自己的名字"（简体中文 / English），
 * 跟着界面语言翻译反而会让人选错，所以用常量表。
 */
const LANGUAGE_NAMES: Record<string, string> = {
  "zh-CN": "简体中文",
  "en-US": "English (US)",
};

export function useDisplayNames() {
  const { t, te } = useI18n();

  /** 地区代码 → 显示名；没有翻译时返回代码本身。 */
  function regionLabel(code: string): string {
    const key = `region.${code}`;
    // `te` 保证"没有翻译"与"翻译成了空串"两种情况的处理一致。
    return te(key) ? t(key) : code;
  }

  /** 语言代码 → 显示名；未知语言回退成代码本身。 */
  function languageLabel(code: string): string {
    return LANGUAGE_NAMES[code] ?? code;
  }

  return { regionLabel, languageLabel };
}
