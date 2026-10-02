// M3 配色客户端：负责前后端配色数据的同步
//
// 同步策略（优先顺序）：
//   1. 接口同步 —— 在 Tauri 运行时调用 Rust 后端 `generate_m3_scheme` 命令
//      （使用成熟的 `material-colors` crate 计算，结果权威）。
//   2. 本地回退 —— 非 Tauri 环境（如 vite 浏览器预览）或后端调用失败时，
//      使用前端 TS 镜像引擎生成等价方案，保证 UI 始终可用。
//
// 注意：这里只"算"配色，不"应用"——应用主题由 `<m3e-theme>` 的动态配色负责，
// 见 `lib/theme.ts`。`/m3` 配色实验室只用它来预览。

import { generateM3Scheme as invokeGenerateM3Scheme } from "../api/m3";
import { generateM3Scheme } from "./engine";
import type { M3GenerateOptions, M3Scheme } from "./types";

/** 当前是否处于 Tauri 运行环境 */
function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

/**
 * 生成 M3 配色方案。
 * 优先调用 Rust 后端命令（接口同步），失败则回退到前端引擎。
 * 返回方案与实际数据来源（backend / frontend），便于 UI 展示同步状态。
 */
export async function generateM3SchemeSynced(
  seed: string,
  options: M3GenerateOptions = {}
): Promise<{ scheme: M3Scheme; source: "backend" | "frontend" }> {
  if (isTauri()) {
    try {
      const result = await invokeGenerateM3Scheme(seed, options);
      return { scheme: result, source: "backend" };
    } catch (e) {
      console.warn("[M3] 后端命令调用失败，回退到前端引擎：", e);
    }
  }
  return { scheme: generateM3Scheme(seed, options), source: "frontend" };
}

/** 将方案导出为可持久化的 JSON 字符串（配色实验室的"复制方案"用）。 */
export function exportM3Scheme(scheme: M3Scheme): string {
  return JSON.stringify(scheme, null, 2);
}
