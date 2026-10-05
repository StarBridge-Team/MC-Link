/**
 * 后端契约类型的统一出口。
 *
 * 按域拆成三个文件是为了遵守本项目的单文件行数上限（前端 400 行）；
 * 调用方仍然只 import 这个路径，拆分对使用方不可见。
 *
 * - `typesCore`   —— 通用、个性化设置、初始化、更新、窗口/托盘
 * - `typesSetup`  —— 设置项元配置、OOBE 与法务、插件路由推荐
 * - `typesPlugin` —— 插件系统、社区数据
 *
 * 所有字段名必须与 `src-tauri/src/**` 的 serde 输出逐字一致，详见各文件顶部注释。
 */
export * from "./typesCore";
export * from "./typesSetup";
export * from "./typesPlugin";
export * from "./typesConnect";
