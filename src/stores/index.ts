/**
 * Pinia 实例。
 *
 * 注意：**每个窗口一份**。Pinia 不是跨窗口的全局单例——跨窗口同步只能靠
 * Rust 的权威状态 + 事件（见 `src/ipc/bridge.ts`）。
 */
import { createPinia } from "pinia";

export const pinia = createPinia();
