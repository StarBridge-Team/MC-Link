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

/**
 * 类型的视觉前缀。
 *
 * `M3eSnackbar.open(message, ...)` 只认 `duration` / `closeLabel` / `actionCallback`
 * （见 `node_modules/@m3e/web/dist/snackbar.js`），**没有类型参数**，
 * 因此成功 / 失败 / 警告只能靠前缀区分，否则三种提示在界面上完全一样。
 */
const TYPE_PREFIX: Record<ToastKind, string> = {
  info: "",
  success: "✓ ",
  error: "✕ ",
  warning: "! ",
};

/**
 * 显示一条提示。
 *
 * `type` 省略时不加前缀（普通信息提示保持纯文本）；单参调用仍然可用。
 */
export function showToast(content: string, type?: ToastKind): void {
  const prefix = type ? TYPE_PREFIX[type] ?? "" : "";
  M3eSnackbar.open(prefix ? `${prefix}${content}` : content, { duration: DURATION });
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

/** `<m3e-dialog>` 里确认框用到的成员（只声明用得到的，避免 `any`）。 */
interface M3eDialogElement extends HTMLElement {
  returnValue: string;
  show(): Promise<void>;
  hide(returnValue?: string): Promise<void>;
}

/**
 * 确认对话框，`confirm` 解析为 true。
 *
 * 只用于"会丢掉用户操作"的场景；一般提示用 `showToast` 就够了。
 * 用命令式创建一个 `<m3e-dialog>`。
 *
 * # 结果必须等 `closed` 事件，不能读 `show()` 的 resolve
 *
 * `show()` 在弹窗**打开后**就 resolve（JSDoc: "resolves when the dialog is open"），
 * 那一刻 `returnValue` 还是空串 —— 直接读它必然得到 `false`，确认框整体不可用。
 * 真正写入 `returnValue` 并派发 `closed` 的是 `hide()`，所以这里等 `closed`。
 * 取消按钮 / ESC / 点遮罩都走同一个 `hide()`，一律按 cancel 处理。
 */
export async function confirmAction(options: {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
}): Promise<boolean> {
  const dialog = document.createElement("m3e-dialog") as unknown as M3eDialogElement;
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
    // 必须走 `hide()`：它才会写 `returnValue` 并派发 `closed`。
    // `removeAttribute("open")` 只是摘掉属性，事件永远不来（结果恒为 false）。
    void dialog.hide().catch(() => undefined);
  });
  const cancelBtn = document.createElement("m3e-button");
  cancelBtn.textContent = options.cancelText ?? "取消";
  cancelBtn.addEventListener("click", () => {
    dialog.returnValue = "cancel";
    void dialog.hide().catch(() => undefined);
  });
  actions.append(cancelBtn, confirmBtn);

  dialog.append(header, body, actions);
  document.body.appendChild(dialog);

  // 先挂监听再 `show()`：`closed` 由 `hide()` 派发，漏挂就永远等不到结果。
  const closed = new Promise<string>((resolve) => {
    dialog.addEventListener("closed", () => resolve(dialog.returnValue), { once: true });
  });

  try {
    await dialog.show();
    return (await closed) === "confirm";
  } finally {
    dialog.remove();
  }
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
