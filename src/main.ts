import { createApp } from "vue";
// @m3e/web：M3E 的 Web Components。按需注册用到的组件包（每个包注册一批自定义元素）。
import "@m3e/web/theme"; // <m3e-theme>（动态配色 + 动效方案）
import "@m3e/web/icon"; // <m3e-icon>
import "@m3e/web/button"; // <m3e-button>
import "@m3e/web/card"; // <m3e-card>
import "@m3e/web/switch"; // <m3e-switch>
import "@m3e/web/checkbox"; // <m3e-checkbox>
import "@m3e/web/slider"; // <m3e-slider>/<m3e-slider-thumb>
import "@m3e/web/select"; // <m3e-select>/<m3e-option>
import "@m3e/web/option"; // <m3e-option>
import "@m3e/web/form-field"; // <m3e-form-field>
import "@m3e/web/search"; // <m3e-search-bar>
import "@m3e/web/avatar"; // <m3e-avatar>
import "@m3e/web/progress-indicator"; // <m3e-linear-progress-indicator>
import "@m3e/web/nav-rail"; // 左侧主导航
import "@m3e/web/nav-bar"; // <m3e-nav-bar>/<m3e-nav-item>（游戏页 Tab 条）
import "@m3e/web/nav-menu"; // 设置二级导航
import "@m3e/web/app-bar"; // 标题栏
import "./styles/tokens.css";
import "./styles/base.css";
import App from "./App.vue";
import { router } from "./router";
import { i18n, initI18n } from "./i18n";
import { bootstrapAssets } from "./lib/resourceCache";

const app = createApp(App);
app.use(i18n);
app.use(router);

// 挂载前统一等两件事，两者都自带超时与降级，因此不会把"后端慢"变成白屏：
//
// 1. initI18n —— 语言必须在挂载前读到，否则会先按浏览器语言渲染一帧、
//    拿到后端设置后再切换，用户能看到文案闪动。
// 2. bootstrapAssets —— 从资源服务器同步字体/图标到本地缓存并注入（已存在则跳过）。
//    这是全局唯一的"统一加载时机"：标题栏与导航在 <router-view> 之外，
//    路由守卫覆盖不到它们，所以门闩只能放在挂载前。
Promise.allSettled([initI18n(), bootstrapAssets()]).finally(() => app.mount("#app"));
