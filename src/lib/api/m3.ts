import { invoke } from "@tauri-apps/api/core";
import type { M3GenerateOptions, M3Scheme } from "../m3/types";

/**
 * M3 配色方案的 IPC 封装。
 *
 * 此前 `src/lib/m3/m3Client.ts` 直接 `invoke("generate_m3_scheme")`，
 * 绕过了 api 层；命令改名时会漏掉它。这里把调用收口到 api 层，
 * `m3Client` 只负责"后端优先、前端镜像回退"的策略。
 */
export async function generateM3Scheme(
  seed: string,
  options: M3GenerateOptions = {},
) {
  return invoke<M3Scheme>("generate_m3_scheme", {
    seed,
    variant: options.variant,
    contrast: options.contrast,
  });
}
