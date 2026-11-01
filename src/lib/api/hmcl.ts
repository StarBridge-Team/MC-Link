import { invoke } from "@tauri-apps/api/core";

/**
 * HMCL 主题导入（后端 `src-tauri/src/hmcl.rs`）。
 *
 * 只做一件事：把 HMCL 主题目录里的颜色**翻译成本程序的种子色**。
 * 主题本身不在本程序内实现（用户已定：主题走单独系统）。
 */
export interface HmclTheme {
  name: string;
  path: string;
  /** 解析出的种子色（`#rrggbb`）；解析不到时为 null。 */
  seed: string | null;
}

/** 列出 HMCL 数据目录下已安装的主题。找不到目录时返回空数组（不算失败）。 */
export function listHmclThemes() {
  return invoke<HmclTheme[]>("hmcl_list_themes");
}

/** 重新读取指定主题目录的种子色。 */
export function getHmclThemeSeed(path: string) {
  return invoke<string | null>("hmcl_theme_seed", { path });
}
