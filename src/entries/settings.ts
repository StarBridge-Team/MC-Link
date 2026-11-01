/**
 * 设置窗口入口（B7：设置独立窗口）。
 *
 * 与主窗口共用组件库注册、Pinia 与事件桥；但**只挂设置路由**——这里不需要
 * 首页/联机/房间那套导航壳，`SettingView` 自己带左树。
 *
 * 用独立 router 而不是复用 `router/index.ts`：后者会把整套导航路由（以及
 * `createWebHashHistory` 的初始重定向到 `/home`）带进来，设置窗口一打开就会
 * 落到首页，还得再兜一次重定向。
 */
import { createApp } from "vue";
import { createRouter, createWebHashHistory } from "vue-router";

import "../ui/registerM3e";
import "../styles/tokens.css";
import "../styles/base.css";

import SettingsApp from "../app/SettingsApp.vue";
import SettingView from "../views/SettingView.vue";
import { i18n, initI18n } from "../i18n";
import { installIpcBridge } from "../ipc/bridge";
import { bootstrapAssets } from "../lib/resourceCache";
import { pinia } from "../stores";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [{ path: "/:tab?", name: "setting", component: SettingView }],
});

const app = createApp(SettingsApp);
app.use(pinia);
app.use(i18n);
app.use(router);

installIpcBridge();

// 与 widget/room 入口一致：字体与图标字体由 `bootstrapAssets` 注入，
// 漏了这一步会表现为"图标变成文字、正文字体回落系统默认"。
Promise.allSettled([initI18n(), bootstrapAssets()]).finally(() => app.mount("#app"));
