<script setup lang="ts">
/**
 * 轻量信息条（成功 / 警告 / 危险 / 信息）。
 *
 * 与 Toast 的分工：Toast 是"刚发生的事，看一眼就过去"；
 * InfoBar 是"当前状态，会一直留在页面上直到状态变化"。
 */
withDefaults(
  defineProps<{
    kind?: "info" | "success" | "warning" | "danger";
    icon?: string;
    text: string;
  }>(),
  { kind: "info", icon: "" },
);

const DEFAULT_ICON: Record<string, string> = {
  info: "bi bi-info-circle",
  success: "bi bi-check-circle",
  warning: "bi bi-exclamation-triangle",
  danger: "bi bi-x-octagon",
};
</script>

<template>
  <div class="info-bar" :class="`info-bar--${kind}`">
    <i :class="icon || DEFAULT_ICON[kind]" class="info-bar__icon" />
    <span class="info-bar__text">{{ text }}</span>
    <div v-if="$slots.default" class="info-bar__actions">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.info-bar {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  border-radius: var(--r-md);
  font-size: var(--fs-base);
  line-height: var(--lh-normal);
  background: var(--info-container);
  color: var(--text-primary);
}

.info-bar__icon {
  font-size: var(--fs-title);
  flex-shrink: 0;
  color: var(--info);
}

.info-bar__text {
  flex: 1;
  min-width: 0;
}

.info-bar__actions {
  display: flex;
  gap: var(--sp-2);
  flex-shrink: 0;
}

.info-bar--success {
  background: var(--success-container);
}

.info-bar--success .info-bar__icon {
  color: var(--success);
}

.info-bar--warning {
  background: var(--warning-container);
}

.info-bar--warning .info-bar__icon {
  color: var(--warning);
}

.info-bar--danger {
  background: var(--error-container);
  color: var(--on-error-container);
}

.info-bar--danger .info-bar__icon {
  color: var(--error);
}
</style>
