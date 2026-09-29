import { createRouter, createWebHashHistory } from "vue-router";

const HomePage = () => import("@/components/HomePage.vue");
const ConnectPage = () =>
  import(/* webpackPrefetch: true */ "@/components/connect/ConnectPage.vue");
const SettingPage = () =>
  import(/* webpackPrefetch: true */ "@/components/setting/SettingPage.vue");
const M3Showcase = () => import("@/components/ui/M3Showcase.vue");

const routes = [
  { path: "/", redirect: "/home" },
  { path: "/home", name: "home", component: HomePage },
  { path: "/connect", name: "connect", component: ConnectPage },
  {
    path: "/setting/:tab?",
    name: "setting",
    component: SettingPage,
    props: true,
  },
  { path: "/m3", name: "m3", component: M3Showcase },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
