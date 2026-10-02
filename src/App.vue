<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import AppTitleBar from "./components/layout/AppTitleBar.vue";
import NavList, { type NavListItem } from "./components/layout/NavList.vue";
import NavRail, { type NavItem } from "./components/layout/NavRail.vue";
import OobeOverlay from "./components/oobe/OobeOverlay.vue";
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

// m3e-theme 的明暗方案：system 映射到 auto（跟随系统），light/dark 直传。
const scheme = computed<"light" | "dark" | "auto">(() => {
  const m = settings.state.theme_mode;
  return m === "system" ? "auto" : (m as "light" | "dark");
});

/** 把引导状态收敛成一个 ref，模板里不必写 `setup.needsOobe.value`。 */
const needsOobe = computed(() => setup.needsOobe.value);

// ---- 标题 ----
const pageTitle = computed(() => {
  switch (route.name) {
    case "connect":
      return t("nav.connect");
    case "m3":
      return t("m3.title");
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
  { id: "m3", icon: "palette", label: t("nav.m3") },
  { id: "setting", icon: "tune", label: t("nav.setting") },
]);

const SETTING_TAB_ICONS: Record<string, string> = {
  personalization: "brush",
  background: "image",
  homepage: "home",
  general: "translate",
  plugins: "extension",
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

const showSecondary = computed(() => route.name === "setting");
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

// ---- 启动 ----
onMounted(async () => {
  settings.bindSystemTheme(() => void settings.applyAll());

  await settings.load();

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
    :color="settings.state.theme_color"
    :scheme="scheme"
    motion="standard"
    variant="tonal-spot"
  >
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
      <NavRail :items="navItems" :active="String(route.name ?? 'home')" @select="onNavSelect" />
      <NavList
        v-if="showSecondary"
        :items="secondaryItems"
        :active="activeSecondary"
        @select="onSecondarySelect"
      />
      <main class="shell__main">
        <RouterView v-slot="{ Component, route: current }">
          <!--
            刻意**不用** `mode="out-in"`：路由组件是 `() => import(...)` 懒加载的，
            搭配 out-in 时实测第一次切换之后 `<RouterView>` 就永久只剩一个注释节点，
            整块内容区空白（标题与侧栏正常，所以很容易被误判成"页面组件的问题"）。
            默认的交叉淡入淡出没有这个现象，观感上也够用。

            `:key="current.path"` 是必要的：同一组件实例在 `/setting/a` → `/setting/b`
            之间会被复用，不加 key 时组件内的 `onMounted` 不会再跑。
          -->
          <Transition name="page">
            <component :is="Component" :key="current.path" />
          </Transition>
        </RouterView>
      </main>
    </div>

    <OobeOverlay v-if="needsOobe" />
  </div>
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
  background: var(--app-bg);
}

.shell__bg {
  position: absolute;
  inset: 0;
  z-index: 0;
  background-image: var(--app-bg-image);
  background-size: var(--app-bg-size);
  background-position: center;
  background-repeat: no-repeat;
  pointer-events: none;
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
</style>
