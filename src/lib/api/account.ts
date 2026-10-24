/**
 * 离线账号（后端 `src-tauri/src/commands/account.rs`）。
 *
 * UUID 由后端按 Minecraft 离线模式算法生成，前端只读——身份标识不能由前端决定。
 */
import { call } from "../../ipc/client";

export interface OfflineAccount {
  name: string;
  uuid: string;
  createdAt: number;
}

/** 当前账号；从未注册过时为 `null`。 */
export function accountGet() {
  return call<OfflineAccount | null>("account_get");
}

/** 注册或改名。同名重复调用幂等（UUID 不变）。 */
export function accountRegister(name: string) {
  return call<OfflineAccount>("account_register", { name });
}
