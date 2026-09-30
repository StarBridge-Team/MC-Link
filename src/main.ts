import { createApp } from "vue";
import App from "./App.vue";
import { router } from "./router";
import ElementPlus from "element-plus";
import "element-plus/dist/index.css";
import "./assets/animations.css";
import "./styles/m3-theme.css";
import { i18n, initI18n } from "./i18n";

const app = createApp(App);
app.use(i18n);
app.use(router);
app.use(ElementPlus);

// 语言设置要在挂载前读到：否则会先按浏览器语言渲染一帧、拿到后端设置后再切换，
// 用户能看到文案闪动。initI18n 自带超时与兜底（读不到就用浏览器语言），不会拖住启动。
initI18n().finally(() => app.mount("#app"));
