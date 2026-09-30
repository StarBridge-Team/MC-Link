/**
 * 简体中文文案。
 *
 * 约定：
 * - 按「域」分组（`app` / `common` / `oobe` / `settings` …），不要堆成一层平铺；
 * - 新增语言必须三处同改：本目录加文件、`src/i18n/index.ts` 注册、
 *   `src-tauri/src/setup.rs` 的 `SUPPORTED_LANGUAGES` 登记；
 * - 组件里用 `const { t } = useI18n()` 后 `t("common.next")`。
 */
export default {
  app: {
    name: "MC Link",
    tagline: "我的世界联机工具",
  },

  common: {
    next: "下一步",
    back: "上一步",
    finish: "完成",
    cancel: "取消",
    confirm: "确定",
    loading: "加载中…",
    retry: "重试",
    save: "保存",
    close: "关闭",
  },

  oobe: {
    title: "欢迎使用 MC Link",
    subtitle: "先选择界面语言与地区，之后可随时在设置里修改。",
    languageLabel: "界面语言",
    regionLabel: "地区",
    regionHint: "地区用于选择更合适的服务节点。",
    detectedHint: "已根据系统设置预选，可自行更改。",
    start: "开始使用",
  },

  settings: {
    language: "界面语言",
    region: "地区",
  },

  /** 地区代码 → 显示名。取不到的名称由界面回退成原始代码。 */
  region: {
    CN: "中国大陆",
    HK: "香港",
    TW: "台湾",
    JP: "日本",
    KR: "韩国",
    SG: "新加坡",
    US: "美国",
    GB: "英国",
    DE: "德国",
    FR: "法国",
    AU: "澳大利亚",
    CA: "加拿大",
    BR: "巴西",
    OTHER: "其他 / 未列出",
  },
};
