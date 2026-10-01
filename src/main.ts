import { createApp } from "vue";
import App from "./App.vue";
import { router } from "./router";
import ElementPlus from "element-plus";
import "element-plus/dist/index.css";
import "./assets/animations.css";
import "./styles/m3-theme.css";
import { i18n, initI18n } from "./i18n";
import { bootstrapAssets } from "./lib/resourceCache";

const app = createApp(App);
app.use(i18n);
app.use(router);
app.use(ElementPlus);

// 挂载前统一准备两件事，两者都自带超时与降级，因此不会把"后端慢"变成白屏：
//
// 1. initI18n —— 语言设置必须在挂载前读到，否则会先按浏览器语言渲染一帧、
//    拿到后端设置后再切换，用户能看到文案闪动。
// 2. bootstrapAssets —— 从资源服务器同步字体/图标到本地缓存并注入（已存在则跳过）。
//    此处是全局唯一的"统一加载时机"：标题栏与侧边栏在 <router-view> 之外，
//    路由守卫覆盖不到它们，所以门闩只能放在挂载前。
Promise.allSettled([initI18n(), bootstrapAssets()]).finally(() => app.mount("#app"));
