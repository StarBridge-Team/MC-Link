/**
 * 房间窗口入口。
 *
 * 与主窗口共用一个 Vite 构建、一份组件库注册、一份 Pinia 与事件桥；
 * 但不装 router、不渲染导航壳——房间窗口只承载一个会话。
 */
import { createApp } from "vue";

import "../ui/registerM3e";
import "../styles/tokens.css";
import "../styles/base.css";

import RoomApp from "../app/RoomApp.vue";
import { i18n, initI18n } from "../i18n";
import { installIpcBridge } from "../ipc/bridge";
import { bootstrapAssets } from "../lib/resourceCache";
import { pinia } from "../stores";

const app = createApp(RoomApp);
app.use(pinia);
app.use(i18n);

installIpcBridge();

Promise.allSettled([initI18n(), bootstrapAssets()]).finally(() => app.mount("#app"));
