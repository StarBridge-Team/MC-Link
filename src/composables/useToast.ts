import "@m3e/web/snackbar";
import "@m3e/web/dialog";

/**
 * 轻量提示封装（基于 @m3e/web 的 Snackbar 与 Dialog）。
 *
 * @m3e/web 的 Snackbar 是一个挂在全局的类（`M3eSnackbar`），由 `import "@m3e/web/snackbar"`
 * 的副作用注册；这里统一节奏（时长），界面只需要表达"提示什么"。
 * M3E 的 snackbar 是规范色的表面，不再有旧版的黑底。
 */

export type ToastKind = "info" | "success" | "warning" | "error";

const DURATION = 2600;

/** 显示一条提示。 */
export function showToast(content: string, _type?: ToastKind): void {
  M3eSnackbar.open(content, { duration: DURATION });
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

/** 关闭当前提示。 */
export function clearToasts(): void {
  M3eSnackbar.dismiss();
}

/**
 * 确认对话框，`confirm` 解析为 true。
 *
 * 只用于"会丢掉用户操作"的场景；一般提示用 `showToast` 就够了。
 * 用命令式创建一个 `<m3e-dialog>`，其 `show()` 在关闭后 resolve，
 * 结果从 `returnValue` 读取（取消或点关闭按钮都视为 cancel）。
 */
export async function confirmAction(options: {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
}): Promise<boolean> {
  const dialog = document.createElement("m3e-dialog") as HTMLElement & {
    returnValue: string;
    show: () => Promise<void>;
  };
  dialog.setAttribute("dismissible", "");

  const header = document.createElement("div");
  header.slot = "header";
  header.textContent = options.title;

  const body = document.createElement("div");
  body.textContent = options.message;

  const actions = document.createElement("div");
  actions.slot = "actions";
  const confirmBtn = document.createElement("m3e-button");
  confirmBtn.setAttribute("variant", "filled");
  confirmBtn.textContent = options.confirmText ?? "确定";
  confirmBtn.addEventListener("click", () => {
    dialog.returnValue = "confirm";
    dialog.removeAttribute("open");
  });
  const cancelBtn = document.createElement("m3e-button");
  cancelBtn.textContent = options.cancelText ?? "取消";
  cancelBtn.addEventListener("click", () => {
    dialog.returnValue = "cancel";
    dialog.removeAttribute("open");
  });
  actions.append(cancelBtn, confirmBtn);

  dialog.append(header, body, actions);
  document.body.appendChild(dialog);

  await dialog.show();
  const result = dialog.returnValue;
  dialog.remove();
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
