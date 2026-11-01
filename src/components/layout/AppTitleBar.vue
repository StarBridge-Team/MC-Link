<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useSnapTracking } from "../../composables/useSnapTracking";
import { useWindowChromeStore } from "../../stores/windowChrome";
import TitleBarButton from "../../ui/TitleBarButton.vue";

/**
 * 无边框窗口的自定义标题栏。
 *
 * 左侧是「返回上一步」箭头（`canGoBack` 为假时置灰），中间是当前页面标题，
 * 右侧是窗口按钮。拖动由父组件传入的 `drag` 触发：走 Tauri 的
 * `start_dragging`，比纯 `data-tauri-drag-region` 在多层子元素下更稳。
 *
 * 按钮本身抽到 `ui/TitleBarButton.vue`，房间窗口用的是同一份。
 */
defineProps<{
  title: string;
  canGoBack: boolean;
}>();

const emit = defineEmits<{
  back: [];
  drag: [];
  minimize: [];
  maximize: [];
  close: [];
}>();

const { t } = useI18n();

/**
 * 最大化按钮的 id 是**跨层契约**：后端的贴靠覆盖层要靠它找到按钮并盖准位置
 * （见 `src/composables/useSnapTracking.ts` 与 `src-tauri/src/window/snap`）。
 */
const MAXIMIZE_BUTTON_ID = "snap-maximize";

const chrome = useWindowChromeStore();
/** 原生覆盖层回传的悬停态：鼠标被它接管，`CSS :hover` 不会自己触发。 */
const maximizeHovered = computed(() => chrome.snapHovered);

// 位置上报 + 点击回传都在这里接上：点击复用父组件已有的最大化路径，
// 不另写一份 `maximize()`，避免原生/JS 两套状态机。
useSnapTracking(MAXIMIZE_BUTTON_ID, () => emit("maximize"));
</script>

<template>
  <header class="titlebar" @mousedown="emit('drag')">
    <div class="titlebar__side">
      <TitleBarButton
        icon="arrow_back"
        :title="t('titlebar.back')"
        :disabled="!canGoBack"
        @click="emit('back')"
      />
    </div>

    <div class="titlebar__title">{{ title }}</div>

    <div class="titlebar__side titlebar__side--end">
      <TitleBarButton
        icon="remove"
        :title="t('titlebar.minimize')"
        @click="emit('minimize')"
      />
      <TitleBarButton
        :id="MAXIMIZE_BUTTON_ID"
        icon="crop_square"
        :title="t('titlebar.maximize')"
        :active="maximizeHovered"
        @click="emit('maximize')"
      />
      <TitleBarButton
        icon="close"
        danger
        :title="t('titlebar.close')"
        @click="emit('close')"
      />
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  height: var(--titlebar-height);
  padding: 0 var(--sp-2);
  flex-shrink: 0;
  color: var(--text-secondary);
  /* 标题栏本身透明：窗口材质与自定义背景要能透上来 */
  background: transparent;
  -webkit-app-region: drag;
}

.titlebar__side {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 96px;
  -webkit-app-region: no-drag;
}

.titlebar__side--end {
  justify-content: flex-end;
}

.titlebar__title {
  flex: 1;
  min-width: 0;
  text-align: center;
  font-size: var(--fs-base);
  font-weight: var(--fw-medium);
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
