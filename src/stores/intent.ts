/**
 * 跨窗口的用户意图。
 *
 * 托盘面板要"打开主窗口并落到联机页"，但它跑在另一个 Webview 里，没有主窗口的
 * router 与 store。做法是后端发一条 `mclink://app/intent`，主窗口收下后自己决定
 * 怎么消费——比让小组件直接调主窗口的命令要稳（后者等于跨窗口遥控）。
 */
import { ref } from "vue";
import { defineStore } from "pinia";

export interface AppIntent {
  action: string;
  process?: string | null;
}

export const useIntentStore = defineStore("intent", () => {
  const pending = ref<AppIntent | null>(null);

  function push(intent: AppIntent) {
    pending.value = intent;
  }

  /** 取出并清空。消费方只该有一个，重复消费会让用户被传送到两次。 */
  function take(): AppIntent | null {
    const value = pending.value;
    pending.value = null;
    return value;
  }

  return { pending, push, take };
});
