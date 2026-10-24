/**
 * 离线账号（每窗口一份投影，权威在 Rust 的 `Setting/account.yml`）。
 *
 * # 迁移
 *
 * 0.5.x 把玩家名只存在 `localStorage`（`KEYS.playerName`），后端 `account.yml`
 * 从未被写过。首次 `load()` 读到"没有账号"时，把老值注册进去——否则升级后
 * 玩家名会凭空消失。`KEYS.playerName` 从此**只读不写**。
 */
import { ref } from "vue";
import { defineStore } from "pinia";

import { errorMessage } from "../ipc/client";
import { accountGet, accountRegister, type OfflineAccount } from "../lib/api/account";
import { KEYS, local } from "../lib/persist";

export const useAccountStore = defineStore("account", () => {
  const account = ref<OfflineAccount | null>(null);
  const loaded = ref(false);

  async function load(): Promise<OfflineAccount | null> {
    if (loaded.value) return account.value;
    try {
      account.value = await accountGet();
      if (!account.value) {
        const legacy = local.getString(KEYS.playerName).trim();
        if (legacy) {
          account.value = await accountRegister(legacy);
        }
      }
    } catch (e) {
      // 读不到就当没注册：界面会要求填名字，比让联机带着空身份发出去好。
      console.warn("[account] 读取账号失败", errorMessage(e));
      account.value = null;
    } finally {
      loaded.value = true;
    }
    return account.value;
  }

  async function register(name: string): Promise<OfflineAccount> {
    const next = await accountRegister(name);
    account.value = next;
    loaded.value = true;
    return next;
  }

  /** 联机时使用的展示名。没注册过时的兜底值。 */
  function displayName(): string {
    return account.value?.name || "Player";
  }

  return { account, loaded, load, register, displayName };
});
