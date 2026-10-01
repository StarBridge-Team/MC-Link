// M3 配色客户端：负责前后端配色数据的同步
//
// 同步策略（优先顺序）：
//   1. 接口同步 —— 在 Tauri 运行时调用 Rust 后端 `generate_m3_scheme` 命令
//      （使用成熟的 `material-colors` crate 计算，结果权威）。
//   2. 本地回退 —— 非 Tauri 环境（如 vite 浏览器预览）或后端调用失败时，
//      使用前端 TS 镜像引擎生成等价方案，保证 UI 始终可用。
//   3. 配置同步 —— 通过 `loadM3Scheme` 直接载入后端导出的 JSON 配置。

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

/**
 * 配置同步：直接载入后端导出的 M3 方案 JSON。
 * 可用于从配置文件 / 网络拉取已计算好的配色，避免运行时重复计算。
 */
export function loadM3Scheme(json: M3Scheme): M3Scheme {
  // 轻量校验，确保结构完整
  if (!json.seed || !json.light || !json.dark || !json.palettes) {
    throw new Error("无效的 M3 方案配置");
  }
  return json;
}

/** 将方案导出为可持久化的 JSON 字符串（供配置同步使用） */
export function exportM3Scheme(scheme: M3Scheme): string {
  return JSON.stringify(scheme, null, 2);
}
