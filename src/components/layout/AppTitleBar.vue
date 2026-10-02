<script setup lang="ts">
import { useI18n } from "vue-i18n";

/**
 * 无边框窗口的自定义标题栏。
 *
 * 左侧是「返回上一步」箭头（`canGoBack` 为假时置灰），中间是当前页面标题，
 * 右侧是窗口按钮。拖动由父组件传入的 `drag` 触发：走 Tauri 的
 * `start_dragging`，比纯 `data-tauri-drag-region` 在多层子元素下更稳。
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
</script>

<template>
  <header class="titlebar" @mousedown="emit('drag')">
    <div class="titlebar__side">
      <button
        class="titlebar__btn"
        type="button"
        :disabled="!canGoBack"
        :title="t('titlebar.back')"
        @mousedown.stop
        @click="emit('back')"
      >
        <i class="material-symbols-rounded">arrow_back</i>
      </button>
    </div>

    <div class="titlebar__title">{{ title }}</div>

    <div class="titlebar__side titlebar__side--end">
      <button
        class="titlebar__btn"
        type="button"
        :title="t('titlebar.minimize')"
        @mousedown.stop
        @click="emit('minimize')"
      >
        <i class="material-symbols-rounded">remove</i>
      </button>
      <button
        class="titlebar__btn"
        type="button"
        :title="t('titlebar.maximize')"
        @mousedown.stop
        @click="emit('maximize')"
      >
        <i class="material-symbols-rounded">crop_square</i>
      </button>
      <button
        class="titlebar__btn titlebar__btn--danger"
        type="button"
        :title="t('titlebar.close')"
        @mousedown.stop
        @click="emit('close')"
      >
        <i class="material-symbols-rounded">close</i>
      </button>
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

.titlebar__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 30px;
  border-radius: var(--r-sm);
  color: inherit;
  font-size: var(--fs-title);
  transition: background-color var(--motion-short) var(--ease-standard),
    color var(--motion-short) var(--ease-standard);
}

.titlebar__btn .material-symbols-rounded {
  font-size: inherit;
}

.titlebar__btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--on-surface) 10%, transparent);
  color: var(--text-primary);
}

.titlebar__btn:disabled {
  opacity: 0.3;
  cursor: default;
}

.titlebar__btn--danger:hover {
  background: var(--error);
  color: var(--on-error);
}
</style>
