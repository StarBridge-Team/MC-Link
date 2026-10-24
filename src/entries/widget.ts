/**
 * 小组件窗口入口（当前只有托盘面板）。
 *
 * 与主窗口共用组件库注册、Pinia 与事件桥：房间列表要跟着事件实时更新，
 * 不能靠"每次右键重新拉一次"。
 *
 * 不装 router：一个窗口只承载一个小组件，由 `kind` 决定渲染什么。
 */
import { createApp } from "vue";

import "../ui/registerM3e";
import "../styles/tokens.css";
import "../styles/base.css";

import WidgetApp from "../app/WidgetApp.vue";
import { i18n, initI18n } from "../i18n";
import { installIpcBridge } from "../ipc/bridge";
import { pinia } from "../stores";

const app = createApp(WidgetApp);
app.use(pinia);
app.use(i18n);

installIpcBridge();

Promise.allSettled([initI18n()]).finally(() => app.mount("#app"));
