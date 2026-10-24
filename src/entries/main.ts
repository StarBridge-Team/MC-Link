/**
 * 主窗口入口。
 *
 * 与房间窗口共用组件库注册、Pinia 与事件桥；区别只在装了 router、渲染导航壳。
 */
import { createApp } from "vue";

import "../ui/registerM3e";
import "../styles/tokens.css";
import "../styles/base.css";

import App from "../App.vue";
import { i18n, initI18n } from "../i18n";
import { installIpcBridge } from "../ipc/bridge";
import { bootstrapAssets } from "../lib/resourceCache";
import { router } from "../router";
import { pinia } from "../stores";

const app = createApp(App);
app.use(pinia);
app.use(i18n);
app.use(router);

// 事件桥必须在挂载前装好：房间可能已经在推事件，晚一步就丢。
installIpcBridge();

// 挂载前统一等两件事，两者都自带超时与降级，因此不会把"后端慢"变成白屏：
//
// 1. initI18n —— 语言必须在挂载前读到，否则会先按浏览器语言渲染一帧、
//    拿到后端设置后再切换，用户能看到文案闪动。
// 2. bootstrapAssets —— 从资源服务器同步字体/图标到本地缓存并注入（已存在则跳过）。
Promise.allSettled([initI18n(), bootstrapAssets()]).finally(() => app.mount("#app"));
