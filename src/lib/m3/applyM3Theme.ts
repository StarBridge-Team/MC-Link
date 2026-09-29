// 将 M3 配色方案应用到 DOM（写入 CSS 自定义属性）
//
// 策略：把每个角色在「明 / 暗」两套主题下的色值分别写入
//   `--m3-<role>-l` 与 `--m3-<role>-d`
// 再由 m3-theme.css 根据 `.dark` class 选择当前主题，并桥接到 Element Plus 变量与组件样式。
// 这样只需写入一次，明暗切换由 class 切换即可瞬时完成。

import type { M3Scheme } from "./types";

export function applyM3Scheme(scheme: M3Scheme): void {
  const root = document.documentElement;
  if (!root) return;

  const setBoth = (name: string, light: string, dark: string) => {
    root.style.setProperty(`--m3-${name}-l`, light);
    root.style.setProperty(`--m3-${name}-d`, dark);
  };

  // 1) 全部 M3 角色（明 / 暗）
  for (const key of Object.keys(scheme.light)) {
    setBoth(key, scheme.light[key], scheme.dark[key]);
  }

  // 2) 语义色（success / warning / error / info）
  const sem = scheme.semantic;
  (["success", "warning", "error", "info"] as const).forEach((k) => {
    setBoth(`sem-${k}`, sem[k].light.primary, sem[k].dark.primary);
    setBoth(`sem-${k}-container`, sem[k].light.primary_container, sem[k].dark.primary_container);
    setBoth(`sem-${k}-on`, sem[k].light.on_primary, sem[k].dark.on_primary);
  });
}

/** 移除所有 M3 相关变量（恢复默认主题时使用） */
export function clearM3Scheme(): void {
  const root = document.documentElement;
  if (!root) return;
  const toRemove: string[] = [];
  for (let i = 0; i < root.style.length; i++) {
    const prop = root.style.item(i);
    if (prop.startsWith("--m3-")) toRemove.push(prop);
  }
  toRemove.forEach((p) => root.style.removeProperty(p));
}
