import { Dialog, Snackbar } from "@varlet/ui";

/**
 * 轻量提示封装。
 *
 * 直接暴露 Varlet 的 `Snackbar` 会让调用点散落各种选项；这里统一节奏
 * （位置、时长、样式），界面只需要表达"提示什么"。
 * 主题变量写在 `:root`，而 Snackbar/Dialog 会 teleport 到 body，因此取得到变量。
 */

export type ToastKind = "info" | "success" | "warning" | "error";

const DURATION = 2600;

/** 显示一条提示。 */
export function showToast(content: string, type?: ToastKind): void {
  Snackbar({
    content,
    type,
    position: "bottom",
    duration: DURATION,
  });
}

export function showSuccess(content: string): void {
  showToast(content, "success");
}

export function showError(content: string): void {
  showToast(content, "error");
}

export function showWarning(content: string): void {
  showToast(content, "warning");
}

/** 关闭所有正在显示的提示。 */
export function clearToasts(): void {
  Snackbar.clear();
}

/**
 * 确认对话框，`confirm` 解析为 true。
 *
 * 只用于"会丢掉用户操作"的场景；一般提示用 `showToast` 就够了。
 */
export async function confirmAction(options: {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
}): Promise<boolean> {
  const result = await Dialog({
    title: options.title,
    message: options.message,
    confirmButtonText: options.confirmText,
    cancelButtonText: options.cancelText,
    confirmButtonProps: { type: "danger" },
  });
  return result === "confirm";
}

/** 统一的"失败了就提示原因"包装：调用方不必重复写 try/catch 与文案拼接。 */
export async function withErrorToast(
  action: () => Promise<unknown>,
  fallback: string,
): Promise<boolean> {
  try {
    await action();
    return true;
  } catch (e) {
    showError(`${fallback}：${messageOf(e)}`);
    return false;
  }
}

export function messageOf(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return String(e);
}
