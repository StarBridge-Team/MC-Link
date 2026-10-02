import { createRouter, createWebHashHistory } from "vue-router";

/**
 * 路由表。
 *
 * 路径与路由名沿用重构前的结构（`/` → `/home`、`/connect`、`/setting/:tab?`）。
 * 视图放在 `src/views/`，与可复用组件（`src/components/`）分开。
 */
const routes = [
  { path: "/", redirect: "/home" },
  {
    path: "/home",
    name: "home",
    component: () => import("@/views/HomeView.vue"),
  },
  {
    path: "/connect",
    name: "connect",
    component: () => import("@/views/ConnectView.vue"),
  },
  {
    path: "/setting/:tab?",
    name: "setting",
    component: () => import("@/views/SettingView.vue"),
    props: true,
  },
];

/** 设置页的分区 id，顺序即侧栏顺序。`SettingView` 与侧栏共用这份定义。 */
export const SETTING_TABS = [
  "personalization",
  "homepage",
  "plugins",
  "update",
  "about",
] as const;

export type SettingTab = (typeof SETTING_TABS)[number];

export const DEFAULT_SETTING_TAB: SettingTab = "personalization";

export function isSettingTab(value: unknown): value is SettingTab {
  return typeof value === "string" && (SETTING_TABS as readonly string[]).includes(value);
}

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
