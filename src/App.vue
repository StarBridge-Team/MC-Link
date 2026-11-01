<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import AppTitleBar from "./components/layout/AppTitleBar.vue";
import NavList, { type NavListItem } from "./components/layout/NavList.vue";
import NavRail, { type NavItem } from "./components/layout/NavRail.vue";
import OobeOverlay from "./components/oobe/OobeOverlay.vue";
import { useSetup } from "./composables/useSetup";
import { useWindowControls } from "./composables/useWindowControls";
import { adapterStartupInit } from "./lib/api/adapter";
import { openSettingsWindow } from "./lib/api/window";
import { DEFAULT_SETTING_TAB, SETTING_TABS, isSettingTab } from "./router";
import { useIntentStore } from "./stores/intent";
import { useRegionStore } from "./stores/region";
import { useThemeBinding } from "./ui/themeBinding";
import WindowResizeEdges from "./ui/WindowResizeEdges.vue";
import WidgetLayer from "./ui/WidgetLayer.vue";

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

// 主题绑定与房间窗口共用一份实现（见 `ui/themeBinding.ts`）：
// 两个窗口各写一遍，配色迟早会不一致。
const { settings, colorScheme, themeEl, scheme, color, variant, contrast } = useThemeBinding();
const setup = useSetup();
const windowCtl = useWindowControls();
/** 地区判定：导航入口与联机页都读它，启动时查一次（见 `stores/region.ts`）。 */
const region = useRegionStore();

/** 把引导状态收敛成一个 ref，模板里不必写 `setup.needsOobe.value`。 */
const needsOobe = computed(() => setup.needsOobe.value);

// ---- 标题 ----
const pageTitle = computed(() => {
  switch (route.name) {
    case "connect":
      return t("nav.connect");
    case "rooms":
      return t("nav.rooms");
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
const allNavItems = computed<NavItem[]>(() => [
  { id: "home", icon: "home", label: t("nav.home") },
  { id: "connect", icon: "broadcast_on_home", label: t("nav.connect") },
  { id: "rooms", icon: "meeting_room", label: t("nav.rooms") },
  { id: "game", icon: "sports_esports", label: t("nav.game") },
  { id: "setting", icon: "tune", label: t("nav.setting") },
]);

/**
 * 受地区限制的功能**入口**：**只有中国大陆**命中时隐藏（判定见 `stores/region.ts`）。
 *
 * 注意范围：这里只隐藏入口，**路由一律保留**。用户可能从后退、深链或托盘进来，
 * 把人强行弹走比让他看到说明更懵；联机页本身由 `ConnectView` 整页替换成提示页。
 */
const REGION_GATED_NAV = new Set(["connect", "rooms"]);

const navItems = computed<NavItem[]>(() =>
  allNavItems.value.filter((item) => !(region.blocked && REGION_GATED_NAV.has(item.id))),
);

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
  // 设置已抽成**独立窗口**（B7）：主窗口只负责把它叫出来，不再切自己的路由。
  // `/setting` 路由仍保留，作为深链/兜底入口。
  if (id === "setting") {
    void openSettingsWindow().catch((e) => console.warn("[app] 打开设置窗口失败:", e));
    return;
  }
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

// 托盘面板的意图只负责"把人送到联机页"，具体落到哪个 Tab 由 `ConnectView` 消费。
const intent = useIntentStore();
watch(
  () => intent.pending,
  (value) => {
    if (value?.action === "connect" && route.name !== "connect") {
      // 受限地区不把人送进联机页：入口都藏了，托盘再把人送过去只会撞上提示页。
      if (region.blocked) return;
      void router.push({ name: "connect" });
    }
  },
);

function goBack() {
  if (!canGoBack.value) return;
  router.back();
}

// ---- 启动 ----
onMounted(async () => {
  // 房间事件由 `ipc/bridge.ts` 在挂载前统一订阅，这里不再有页面级订阅：
  // 会话在自己的窗口里，主窗口只维护房间列表。
  settings.bindSystemTheme(() => void settings.applyAll());

  await settings.load();

  // 配色元素要挂载后才存在；绑定后由 useColorScheme 自己决定何时重算
  colorScheme.attach(themeEl.value);

  // 引导状态与窗口位置并行准备，两者互不依赖。
  // 窗口这一段整体兜底：非 Tauri 环境（如 vite 预览）里窗口 API 不存在，
  // 但界面仍应能正常渲染，不能因为"窗口不存在"而中断启动流程。
  await Promise.all([
    setup.load(),
    // 地区判定与引导并行：导航栏要等它出结果才知道该不该显示联机入口。
    // 它内部失败即放行，因此这里不需要 catch（不会让启动流程中断）。
    region.ensure(),
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
    :color="color"
    :variant="variant"
    :contrast="contrast"
    :scheme="scheme"
    motion="standard"
  >
    <Transition name="win" appear>
    <div class="shell">
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

    <!-- 窗口内挂件层：内置背景音视频控制面板 + 未来的 widget 类插件挂件 -->
    <WidgetLayer v-if="!hideNav" />

    <OobeOverlay v-if="needsOobe" />
    <!-- 边缘缩放热区：Windows 由命中测试接管，组件内部只在 Linux 渲染 -->
    <WindowResizeEdges />
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

/* 窗口过渡：打开时由 <Transition appear> 播放淡入 + 轻微放大（.win-enter-*）。
   仅用 transition，不写 @keyframes。
 *
 * **只有进场，没有退场**：最小化/关闭由系统自己处理（DWM 对窗口抓帧缩放），
 * 之前那套"先播 200ms 淡出再调 minimize()"已经删掉——它只是把动作拆成两段、
 * 多出 200ms 迟滞，并不能让窗口真的缩进任务栏。理由见 `useWindowControls.ts` 文件头。 */
.win-enter-from {
  opacity: 0;
  transform: scale(0.96);
}

.win-enter-active {
  transition: opacity var(--motion-medium) var(--ease-standard),
    transform var(--motion-medium) var(--ease-standard);
}
</style>
