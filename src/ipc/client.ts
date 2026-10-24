/**
 * IPC 的唯一出口。
 *
 * 页面、组件、store 都不得直接 `import { invoke }`：命令改名时散落的调用
 * 不会有编译错误，只在运行时炸（`scripts/check-arch.mjs` 会拦）。
 *
 * 这里做两件事：
 * 1. 把后端 `{ code, message }` 规范化成 `IpcError`，调用方可以按 `code` 分支；
 * 2. 保留 `Error.message`，既有界面的错误提示不用改。
 */
import { invoke } from "@tauri-apps/api/core";

export class IpcError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "IpcError";
    this.code = code;
  }
}

function toIpcError(raw: unknown): IpcError {
  if (raw && typeof raw === "object") {
    const { code, message } = raw as { code?: unknown; message?: unknown };
    if (typeof code === "string" && typeof message === "string") {
      return new IpcError(code, message);
    }
  }
  if (typeof raw === "string") return new IpcError("unknown", raw);
  return new IpcError("unknown", String(raw));
}

/** 调用一个后端命令。失败一律抛 `IpcError`。 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    throw toIpcError(raw);
  }
}

/** 取错误的展示文案（既有代码里的 `errMessage` 统一走这里）。 */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
