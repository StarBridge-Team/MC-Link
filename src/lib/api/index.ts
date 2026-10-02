/**
 * IPC 层的唯一出口。
 *
 * 架构约束（`scripts/check-arch.mjs` 会拦）：`invoke(...)` 只允许出现在
 * `src/lib/api/**`。页面与组件一律从本目录取能力，命令改名时只需改这一层。
 */
export * from "./types";
export * from "./app";
export * from "./config";
export * from "./effect";
export * from "./datadir";
export * from "./m3";
export * from "./update";
export * from "./setup";
export * from "./legal";
export * from "./plugin";
export * from "./community";
export * from "./adapter";
export * from "./settingMeta";
export * from "./window";
// 联机（P2P）后端命令当前不存在，此模块仅作将来重接的素材保留。
export * from "./connect";
