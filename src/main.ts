import { createApp } from "vue";
import Varlet from "@varlet/ui";
// Varlet 基线样式必须先于本项目的 token / base 载入，后写的才能覆盖它。
import "@varlet/ui/es/varlet.css";
import "@m3e/web/theme"; // 注册 <m3e-theme> 自定义元素（动态配色 + 动效方案）
import "@m3e/web/icon"; // 图标（Material Symbols）
import "@m3e/web/nav-rail"; // 左侧主导航
import "@m3e/web/nav-menu"; // 设置二级导航
import "@m3e/web/app-bar"; // 标题栏
import "./styles/tokens.css";
import "./styles/base.css";
import App from "./App.vue";
import { router } from "./router";
import { i18n, initI18n } from "./i18n";
import { bootstrapAssets } from "./lib/resourceCache";
import { applyTheme } from "./lib/theme";

// 在挂载前先把 Varlet 的 MD3 亮色主题装上，避免首帧先按 Varlet 默认主题渲染、
// 等 `useSettings.load()` 跑完再切成 MD3 造成闪动。暗色会在 load 后按设置重设。
applyTheme(false);

const app = createApp(App);
app.use(i18n);
app.use(router);
app.use(Varlet);

// 挂载前统一等两件事，两者都自带超时与降级，因此不会把"后端慢"变成白屏：
//
// 1. initI18n —— 语言必须在挂载前读到，否则会先按浏览器语言渲染一帧、
//    拿到后端设置后再切换，用户能看到文案闪动。
// 2. bootstrapAssets —— 从资源服务器同步字体/图标到本地缓存并注入（已存在则跳过）。
//    这是全局唯一的"统一加载时机"：标题栏与导航在 <router-view> 之外，
//    路由守卫覆盖不到它们，所以门闩只能放在挂载前。
Promise.allSettled([initI18n(), bootstrapAssets()]).finally(() => app.mount("#app"));
