<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import AppTitleBar from "./components/layout/AppTitleBar.vue";
import NavList, { type NavListItem } from "./components/layout/NavList.vue";
import NavRail, { type NavItem } from "./components/layout/NavRail.vue";
import OobeOverlay from "./components/oobe/OobeOverlay.vue";
import { useColorScheme } from "./composables/useColorScheme";
import { useConnect } from "./composables/useConnect";
import { useSettings } from "./composables/useSettings";
import { useSetup } from "./composables/useSetup";
import { useWindowControls } from "./composables/useWindowControls";
import { adapterStartupInit } from "./lib/api/adapter";
import { DEFAULT_SETTING_TAB, SETTING_TABS, isSettingTab } from "./router";

/**
 * 应用外壳：标题栏 + 导航 + 路由出口 + 背景层 + 引导浮层。
 *
 * 这里只做"编排"，不持有业务状态：
 * - 个性化状态在 `useSettings()`（模块级单例，设置页读的是同一份）；
 * - 引导状态在 `useSetup()`；
 * - 窗口与进程控制在 `useWindowControls()`（在壳里创建一次，处理器下发给标题栏，
 *   避免子组件再造一个实例导致监听器重复注册）。
 */
const route = useRoute();
const router = useRouter();
const { t } = useI18n();

const settings = useSettings();
const setup = useSetup();
const windowCtl = useWindowControls();
const { roomCode, mode, startListening } = useConnect();

// m3e-theme 的明暗方案：system 映射到 auto（跟随系统），light/dark 直传。
const scheme = computed<"light" | "dark" | "auto">(() => {
  const m = settings.state.theme_mode;
  return m === "system" ? "auto" : (m as "light" | "dark");
});

/**
 * 动态配色走后端（`useColorScheme`），`<m3e-theme>` 只保留它独有的能力（动效变量等）。
 *
 * 后端一旦生效，就把它的三个入参**钉死**：只要它们还跟着用户设置走，`<m3e-theme>`
 * 每次变化都会自己重算调色板、重写整张 CSS 变量样式表，并强制一次同步回流 ——
 * 那正是"改配色很卡"的来源。钉死之后颜色由后端给出的变量决定（内联写在它身上，
 * 优先于它自己算的值，所以它算出来什么都不影响）。
 *
 * 后端不可用（浏览器预览、命令失败）时 `active` 为假，这三个值恢复成真实设置，
 * 重新交给 m3e 自己算 —— 慢一点，但颜色是对的。
 */
const colorScheme = useColorScheme();
const themeEl = ref<HTMLElement | null>(null);

const PINNED_THEME = { color: "#0066cc", variant: "tonal-spot", contrast: "standard" } as const;

const themeColor = computed(() =>
  colorScheme.active.value ? PINNED_THEME.color : settings.state.theme_color,
);
const themeVariant = computed(() =>
  colorScheme.active.value ? PINNED_THEME.variant : settings.state.theme_variant,
);
const themeContrast = computed(() =>
  colorScheme.active.value ? PINNED_THEME.contrast : settings.state.theme_contrast,
);

/** 把引导状态收敛成一个 ref，模板里不必写 `setup.needsOobe.value`。 */
const needsOobe = computed(() => setup.needsOobe.value);

// ---- 标题 ----
const pageTitle = computed(() => {
  switch (route.name) {
    case "connect":
      return t("nav.connect");
    case "room":
      // 房间视图不以导航命名：标题直接用房间码更贴合它"在做一件事"的定位。
      return roomCode.value || t("nav.connect");
    case "game":
      return t("nav.game");
    case "setting": {
      const tab = isSettingTab(route.params.tab) ? route.params.tab : DEFAULT_SETTING_TAB;
      return `${t("setting.title")} · ${t(`setting.${tab}`)}`;
    }
    default:
      return t("app.name");
  }
});

// ---- 导航 ----
const navItems = computed<NavItem[]>(() => [
  { id: "home", icon: "home", label: t("nav.home") },
  { id: "connect", icon: "broadcast_on_home", label: t("nav.connect") },
  { id: "game", icon: "sports_esports", label: t("nav.game") },
  { id: "setting", icon: "tune", label: t("nav.setting") },
]);

const SETTING_TAB_ICONS: Record<string, string> = {
  personalization: "brush",
  homepage: "home",
  general: "translate",
  update: "cloud_download",
  about: "info",
};

const secondaryItems = computed<NavListItem[]>(() =>
  SETTING_TABS.map((id) => ({
    id,
    icon: SETTING_TAB_ICONS[id] ?? "circle",
    label: t(`setting.${id}`),
  })),
);

/**
 * 是否收起左侧主导航栏。
 *
 * 由路由 `meta.hideNav` 决定（房间视图用它把整屏让给成员网格）。
 * 收起时 `NavList` 也要一起让位——它挂在导航栏右侧，只藏一个会留下孤立的二级栏。
 */
const hideNav = computed(() => Boolean(route.meta.hideNav));

const showSecondary = computed(() => route.name === "setting" && !hideNav.value);
const activeSecondary = computed(() =>
  isSettingTab(route.params.tab) ? route.params.tab : DEFAULT_SETTING_TAB,
);

function onNavSelect(id: string) {
  if (route.name === id) return;
  void router.push({ name: id });
}

function onSecondarySelect(id: string) {
  void router.push({ name: "setting", params: { tab: id } });
}

// ---- 返回上一步 ----
const canGoBack = ref(false);

function syncCanGoBack() {
  const state = router.options.history.state as { back?: string | null } | undefined;
  canGoBack.value = Boolean(state?.back);
}

watch(() => route.fullPath, syncCanGoBack, { immediate: true });

function goBack() {
  if (!canGoBack.value) return;
  router.back();
}

// ---- 会话 → 房间视图的流转 ----
//
// 放在外壳而不是 `ConnectView`：进入房间视图时 `ConnectView` 已被卸载，它里面的
// `watch` 不会执行。这里与订阅同层，才覆盖得了整条会话（含退出）。
watch(mode, (next, prev) => {
  if (next === "connected" && prev !== "connected") {
    void router.push({ name: "room" });
  } else if (next === "idle" && prev === "connected" && route.name === "room") {
    // 房间结束（用户退出、对端断开、出错回退）→ 回到联机页。
    void router.push({ name: "connect" });
  }
});

// ---- 启动 ----
onMounted(async () => {
  // 联机事件订阅挂在外壳上，与窗口同生命周期。
  // 页面级订阅会在离开联机页时断开，导致房间视图期间的 `stopped` 事件丢失、
  // 退出后状态卡在 `connected`（详见 `useConnect.startListening` 的注释）。
  void startListening();

  settings.bindSystemTheme(() => void settings.applyAll());

  await settings.load();

  // 配色元素要挂载后才存在；绑定后由 useColorScheme 自己决定何时重算
  colorScheme.attach(themeEl.value);

  // 引导状态与窗口位置并行准备，两者互不依赖。
  // 窗口这一段整体兜底：非 Tauri 环境（如 vite 预览）里窗口 API 不存在，
  // 但界面仍应能正常渲染，不能因为"窗口不存在"而中断启动流程。
  await Promise.all([
    setup.load(),
    (async () => {
      try {
        const restored = await windowCtl.restoreWindowState();
        if (!restored) await windowCtl.setDefaultWindowSize();
        await windowCtl.setupWindowStateListeners();
      } catch (e) {
        console.warn("[app] 初始化窗口状态失败:", e);
      }
    })(),
  ]);

  // 适配器启动初始化：失败不影响界面，插件页里可以手动重试。
  void adapterStartupInit().catch(() => undefined);
});
</script>

<template>
  <m3e-theme
    ref="themeEl"
    :color="themeColor"
    :variant="themeVariant"
    :contrast="themeContrast"
    :scheme="scheme"
    motion="standard"
  >
    <Transition name="win" appear>
    <div class="shell" :class="{ 'is-exiting': windowCtl.isExiting.value }">
    <!-- 背景层：图片背景走 CSS 变量，视频走 <video>（CSS 背景不支持视频） -->
    <div class="shell__bg" />
    <video id="bg-video" class="shell__video" autoplay loop playsinline muted />
    <audio id="bg-music" loop />
    <div class="shell__scrim" />

    <AppTitleBar
      :title="pageTitle"
      :can-go-back="canGoBack"
      @back="goBack"
      @drag="windowCtl.startDrag"
      @minimize="windowCtl.handleMinimize"
      @maximize="windowCtl.handleMaximize"
      @close="windowCtl.handleClose"
    />

    <div class="shell__body">
      <NavRail
        v-if="!hideNav"
        :items="navItems"
        :active="String(route.name ?? 'home')"
        @select="onNavSelect"
      />
      <NavList
        v-if="showSecondary"
        :items="secondaryItems"
        :active="activeSecondary"
        @select="onSecondarySelect"
      />
      <main class="shell__main">
        <RouterView v-slot="{ Component, route: current }">
          <!--
            刻意**不用** `<Transition>`，改为只给进入项一条 CSS 动画（base.css 的
            `page-in`）：

            - 默认的交叉模式会让新旧两页同时存在、一起参与布局，在纵向 flex 里互相挤，
              交叉那一瞬间整块内容会抖一下 —— 这就是"淡入看起来很诡异"的根源；
            - `mode="out-in"` 实测更糟：路由组件是 `() => import(...)` 懒加载的，搭配
              out-in 后第一次切换之后 `<RouterView>` 就永久只剩一个注释节点，整块内容区
              空白（标题与侧栏正常，很容易被误判成"页面组件的问题"）。

            `:key="current.path"` 有两个作用：同一组件实例在 `/setting/a` → `/setting/b`
            之间会被复用，不加 key 时组件内的 `onMounted` 不会再跑；同时它保证路由变化时
            元素是重新创建的，进入动画才会重播。
          -->
          <component :is="Component" :key="current.path" />
        </RouterView>
      </main>
    </div>

    <OobeOverlay v-if="needsOobe" />
    </div>
  </Transition>
  </m3e-theme>
</template>

<style scoped>
.shell {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  /* 背景 = 「当前底色」按「背景不透明度」淡出。
   *
   *   --app-bg       `applyBackground` **只在 solid 时**写入（用户选的颜色）；
   *   --surface      其余情况回落到调色板 surface。`.shell` 在 m3e-theme 作用域内，
   *                  `--surface` 已别名到 `--md-sys-color-surface`，因此跟随配色与暗色；
   *   --app-opacity  0–1，「背景不透明度」。这是**控制看得见多少窗口材质**的旋钮。
   *
   * 两点别动：
   * 1. **不能把 `--app-bg` 写成 `transparent`**（早先在选 Mica 时这么写过）：
   *    `transparent` 混任何比例仍是 `transparent`，alpha 会被吃掉 → 滑块没反应。
   *    必须回落到 `--surface` 这个真实颜色，alpha 才有东西可调。
   * 2. **不能给 `--app-bg` 设默认值**：`var()` 的 fallback 只在变量"无效"时才启用，
   *    一个具体默认值会把 `--surface` 那一级直接截断。tokens.css 有同样的告诫。 */
  background: color-mix(
    in srgb,
    var(--app-bg, var(--surface)) calc(var(--app-opacity) * 100%),
    transparent
  );
  /* 种子色（材质）背景的模糊：backdrop-filter 只能作用于元素自身，
   * 且要求它的背后没有不透明父级 —— `.shell` 正是满足这两条的层级。 */
  backdrop-filter: blur(var(--app-material-blur));
  -webkit-backdrop-filter: blur(var(--app-material-blur));
}

/* 两个背景层都是"糊自己内容的载体"：`.shell__bg` 用 background-image 画图，
 * 对它自己加 filter 就能糊到图（backdrop-filter 反而糊不到），
 * 所以这里用 filter 而不是 backdrop-filter。 */
.shell__bg {
  position: absolute;
  inset: 0;
  z-index: 0;
  background-image: var(--app-bg-image);
  background-size: var(--app-bg-size);
  background-position: center;
  background-repeat: no-repeat;
  pointer-events: none;
  filter: blur(var(--app-media-blur));
}

.shell__video {
  position: absolute;
  inset: 0;
  z-index: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  pointer-events: none;
  display: none;
  filter: blur(var(--app-media-blur));
  /* 模糊会把边缘糊出一圈透明，缩放一点点把露出来的底色盖回去。 */
  transform: scale(1.04);
}

.shell__scrim {
  position: absolute;
  inset: 0;
  z-index: 1;
  background: var(--app-scrim);
  pointer-events: none;
}

.shell__body {
  position: relative;
  z-index: 2;
  flex: 1;
  display: flex;
  min-height: 0;
}

.shell__main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* 窗口过渡：打开时由 <Transition appear> 播放淡入 + 轻微放大（.win-enter-*），
   关闭/最小化时由 .shell.is-exiting 播放淡出 + 缩小。仅用 transition，不写 @keyframes。 */
.win-enter-from {
  opacity: 0;
  transform: scale(0.96);
}

.win-enter-active {
  transition: opacity var(--motion-medium) var(--ease-standard),
    transform var(--motion-medium) var(--ease-standard);
}

.shell.is-exiting {
  opacity: 0;
  transform: scale(0.98);
  transition: opacity var(--motion-medium) var(--ease-standard),
    transform var(--motion-medium) var(--ease-standard);
}
</style>
