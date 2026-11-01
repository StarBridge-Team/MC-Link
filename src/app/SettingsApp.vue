<script setup lang="ts">
import { onMounted } from "vue";
import { useI18n } from "vue-i18n";

import AppTitleBar from "../components/layout/AppTitleBar.vue";
import SettingView from "../views/SettingView.vue";
import { useWindowControls } from "../composables/useWindowControls";
import { useThemeBinding } from "../ui/themeBinding";

/**
 * 设置独立窗口的外壳：标题栏 + 左树/内容（`SettingView`）。
 *
 * 不挂导航栏、不挂背景层——设置窗口只需要一块干净的可操作区域。
 * 配色与主窗口共用 `useThemeBinding`，否则两个窗口的主题会各走一套。
 */
const { t } = useI18n();
const windowCtl = useWindowControls();
const { settings, colorScheme, themeEl, scheme, color, variant, contrast } = useThemeBinding();

onMounted(async () => {
  settings.bindSystemTheme(() => void settings.applyAll());
  await settings.load();
  // 配色元素要挂载后才存在；绑定后由 useColorScheme 自己决定何时重算。
  colorScheme.attach(themeEl.value);
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
    <div class="shell">
      <AppTitleBar
        :title="t('setting.title')"
        :can-go-back="false"
        @drag="windowCtl.startDrag"
        @minimize="windowCtl.handleMinimize"
        @maximize="windowCtl.handleMaximize"
        @close="windowCtl.handleClose"
      />
      <main class="shell__main">
        <SettingView />
      </main>
    </div>
  </m3e-theme>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  /* 不接背景图/视频层：设置窗口只需要一块可读的表面。
   * `--surface` 在 `m3e-theme` 作用域内已别名到 `--md-sys-color-surface`，
   * 因此跟随配色与明暗，不用再引一份颜色来源。 */
  background: var(--surface);
}

.shell__main {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: var(--sp-4);
}
</style>
