/**
 * 窗口外壳的原生状态。
 *
 * 这些状态**只能**由后端事件驱动：最大化按钮上盖着一个原生子窗口（Windows 贴靠布局
 * 的需要），它替浏览器接管了鼠标，`CSS :hover` 与 `@click` 都不会再触发。
 * 所以原生侧把"进入/离开/点击"回传过来，这里做唯一的一份记录。
 */
import { ref } from "vue";
import { defineStore } from "pinia";

import type { WindowIntent } from "../ipc/contracts/window";

export const useWindowChromeStore = defineStore("windowChrome", () => {
  /** 原生覆盖层报告的"鼠标在最大化按钮上"。 */
  const snapHovered = ref(false);
  /** 原生覆盖层的点击计数：消费方（`AppTitleBar`）用 watch 递增触发最大化切换。 */
  const snapClicks = ref(0);

  /** 事件桥只负责收，落到哪个字段由这里决定。 */
  function apply(intent: WindowIntent) {
    switch (intent.action) {
      case "snap-hover":
        snapHovered.value = intent.hovering;
        break;
      case "snap-click":
        snapClicks.value += 1;
        break;
    }
  }

  return { snapHovered, snapClicks, apply };
});
