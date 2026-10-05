<script setup lang="ts">
import { useI18n } from "vue-i18n";

/**
 * 无边框窗口的自定义标题栏。
 *
 * 左侧是「返回上一步」箭头（`canGoBack` 为假时置灰），中间是当前页面标题，
 * 右侧是窗口按钮。拖动由父组件传入的 `drag` 触发：走 Tauri 的
 * `start_dragging`，比纯 `data-tauri-drag-region` 在多层子元素下更稳。
 *
 * 保留自定义实现（而非 `<m3e-app-bar>`）：无边框窗口按钮需要 Windows 惯例的
 * 紧凑尺寸与悬停红，M3 app bar 的尺寸/内边距会破坏标题栏的窗口控制区。
 * 图标改用 `<m3e-icon>`，与全站图标字体统一。
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
        <m3e-icon name="arrow_back" />
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
        <m3e-icon name="remove" />
      </button>
      <button
        class="titlebar__btn"
        type="button"
        :title="t('titlebar.maximize')"
        @mousedown.stop
        @click="emit('maximize')"
      >
        <m3e-icon name="crop_square" />
      </button>
      <button
        class="titlebar__btn titlebar__btn--danger"
        type="button"
        :title="t('titlebar.close')"
        @mousedown.stop
        @click="emit('close')"
      >
        <m3e-icon name="close" />
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

.titlebar__btn m3e-icon {
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

/* 与通用悬停 `.titlebar__btn:hover:not(:disabled)` 特异性相同（0,3,0），
   且本规则写在后面 → 关闭按钮悬停时优先取红色，不会被灰色通用态覆盖。 */
.titlebar__btn--danger:hover:not(:disabled) {
  background: var(--error);
  color: var(--on-error);
}
</style>
