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
import { bootstrapAssets } from "../lib/resourceCache";
import { pinia } from "../stores";

const app = createApp(WidgetApp);
app.use(pinia);
app.use(i18n);

installIpcBridge();

// 字体与图标字体由 `bootstrapAssets` 注入（Poppins / Material Symbols 都在资源服务器上，
// 本地只缓存副本）。漏了这一步的窗口会表现为：图标变成 `open_in_new` 这样的文字、
// 正文字体回落到系统默认——正是"托盘面板字体图标都没工作"的样子。
Promise.allSettled([initI18n(), bootstrapAssets()]).finally(() => app.mount("#app"));
