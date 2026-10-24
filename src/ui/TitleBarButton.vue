<script setup lang="ts">
/**
 * 无边框窗口标题栏上的按钮。
 *
 * 主窗口与房间窗口共用这一份：两个窗口各写一套，悬停色、尺寸、圆角迟早会分叉，
 * 用户看到的就是"同一个应用里两种标题栏"。
 *
 * 保留自定义 `<button>` 而不是 `m3e-button`：窗口控制区需要 Windows 惯例的紧凑尺寸
 * （34×30）与关闭键的红色悬停，M3E 按钮的尺寸/内边距会撑坏标题栏。
 */
defineProps<{
  icon: string;
  title: string;
  danger?: boolean;
  disabled?: boolean;
}>();

const emit = defineEmits<{ (e: "click"): void }>();
</script>

<template>
  <!-- `@mousedown.stop`：标题栏整体是拖拽区，按钮上按下鼠标不能触发拖动 -->
  <button
    class="tb-btn"
    :class="{ 'tb-btn--danger': danger }"
    type="button"
    :title="title"
    :disabled="disabled"
    @mousedown.stop
    @click="emit('click')"
  >
    <m3e-icon :name="icon" />
  </button>
</template>

<style scoped>
.tb-btn {
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

.tb-btn m3e-icon {
  font-size: inherit;
}

.tb-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--on-surface) 10%, transparent);
  color: var(--text-primary);
}

.tb-btn:disabled {
  opacity: 0.3;
  cursor: default;
}

/* 与通用悬停规则特异性相同（0,3,0），写在后面 → 关闭键悬停取红色。 */
.tb-btn--danger:hover:not(:disabled) {
  background: var(--error);
  color: var(--on-error);
}
</style>
