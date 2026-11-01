/**
 * 后端错误码 → 前端固定中文文案。
 *
 * # 为什么存在
 *
 * 后端 `AppError` 是 `{ code, message }` 两段式（见 `src-tauri/src/core/error.rs`）：
 * `code` 是稳定的机器可读标识（kebab-case），`message` 是给人看的兜底文案。
 * 此前前端**只展示 `message`**，于是后端字符串一改、文案就漂移，且无法集中翻译。
 *
 * 这里给出 `code → 中文` 的固定映射：命中 `code` 就显示固定翻译，`message` 只作调试。
 * 一致性优先——宁可文案宽泛，也不要让后端字符串决定界面。
 *
 * # 怎么加一条
 *
 * 1. 后端用 `AppError::new("your-code", "...")`（或 `invalid` / `not_found` / `internal`）；
 * 2. 在这里加一行 `"your-code": "对应的中文"`。
 *
 * 仍返回 `Result<_, String>`（无 `code`）的命令走 `unknown` 兜底，显示原字符串——
 * 那些命令应逐步迁移到 `AppError`。本表只覆盖已带 `code` 的错误。
 */

/** code → 用户可见中文文案。命中即显示固定翻译，不展示后端 `message`。 */
export const ERROR_CODE_MESSAGES: Record<string, string> = {
  "adapter-failed": "联机适配器启动失败，请确认适配器已正确安装并可运行。",
  "invalid-argument": "请求参数不合法，请检查输入后重试。",
  "not-found": "找不到对应的内容（可能已被清理或尚未创建）。",
  internal: "程序内部出错，请重试；若反复出现请查看日志。",
};

/** 按 code 取固定中文；未登记时回退到 `fallback`（一般是后端 `message`）。 */
export function localizeErrorCode(code: string, fallback: string): string {
  return ERROR_CODE_MESSAGES[code] ?? fallback;
}
