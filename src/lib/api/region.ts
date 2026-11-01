import { invoke } from "@tauri-apps/api/core";
import type { RegionInfo } from "./types";

/**
 * 读取当前用户的地区判定。
 *
 * `mainland_china` 由后端算（`commands/region.rs`），前端**不要**拿 `region`
 * 再做一次判断——两套规则迟早不一致，而这类不一致的表现是"某些地区被误锁/漏锁"。
 *
 * 失败时抛出。调用方应把它当成"未知地区"而不是"受限地区"：
 * 网络抖一下就让用户失去功能，比漏拦一次糟得多。
 */
export async function getRegionInfo(): Promise<RegionInfo> {
  return invoke<RegionInfo>("get_region_info");
}
