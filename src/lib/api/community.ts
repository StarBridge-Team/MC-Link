import { invoke } from "@tauri-apps/api/core";
import type { Community } from "./types";

/**
 * 社区数据（贡献者与 Issues）。
 *
 * 后端从 GitHub 取数并落磁盘缓存（TTL 1 小时），离线时回退到过期缓存并把
 * `stale` 置真；因此**这个命令实际上永不报错**，失败信息在 `error` 字段里。
 */

/** 拉取社区数据（带 1 小时磁盘缓存）。 */
export async function fetchCommunity() {
  return invoke<Community>("community_fetch_command");
}

/** 清空社区数据缓存。 */
export async function clearCommunityCache() {
  return invoke<void>("community_clear_cache_command");
}
