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
    path: "/game/:tab?",
    name: "game",
    component: () => import("@/views/GameView.vue"),
    props: true,
  },
  {
    path: "/setting/:tab?",
    name: "setting",
    component: () => import("@/views/SettingView.vue"),
    props: true,
  },
];

/**
 * 设置页的分区 id，顺序即侧栏顺序。`SettingView` 与侧栏共用这份定义。
 *
 * 插件**不在这里**：它已移到「游戏」页（`views/GameView.vue`）。
 */
export const SETTING_TABS = [
  "personalization",
  "homepage",
  "general",
  "update",
  "about",
] as const;

export type SettingTab = (typeof SETTING_TABS)[number];

export const DEFAULT_SETTING_TAB: SettingTab = "personalization";

export function isSettingTab(value: unknown): value is SettingTab {
  return typeof value === "string" && (SETTING_TABS as readonly string[]).includes(value);
}

/** 「游戏」页的三个模式，顺序即工具栏上的 Tab 顺序。 */
export const GAME_TABS = ["market", "games", "plugins"] as const;

export type GameTab = (typeof GAME_TABS)[number];

/** 默认落在「游戏」而不是最左边的「市场」：市场暂时是空的，落地页不该是个空页面。 */
export const DEFAULT_GAME_TAB: GameTab = "games";

export function isGameTab(value: unknown): value is GameTab {
  return typeof value === "string" && (GAME_TABS as readonly string[]).includes(value);
}

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
