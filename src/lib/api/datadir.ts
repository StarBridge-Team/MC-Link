import { invoke } from "@tauri-apps/api/core";
import type { BackgroundFile } from "./types";

/** 获取背景文件列表（包含 data_dir/Background 与旧 exe_dir/Background） */
export async function getBackgroundFiles() {
  return invoke<BackgroundFile[]>("get_background_files");
}

/** 获取背景文件本地绝对路径 */
export async function getBackgroundFileUrl(filename: string) {
  return invoke<string>("get_background_file_url", { filename });
}
