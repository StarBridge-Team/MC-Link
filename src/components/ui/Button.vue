<script setup lang="ts">
interface Props {
  variant?: "default" | "primary" | "danger" | "ghost" | "link" | "outline" | "tool" | "success";
  size?: "default" | "sm" | "lg" | "icon" | "sm-icon" | "lg-icon";
  class?: string;
}

withDefaults(defineProps<Props>(), {
  variant: "default",
  size: "default",
});
</script>

<template>
  <button
    :class="[
      'ubtn',
      `ubtn--${variant}`,
      `ubtn--${size}`,
      $props.class,
    ]"
    v-bind="$attrs"
  >
    <slot />
  </button>
</template>

<style scoped>
.ubtn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  white-space: nowrap;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  border: none;
  cursor: pointer;
  transition: all calc(0.2s * var(--anim-speed, 1)) ease;
  box-sizing: border-box;
}
.ubtn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* sizes */
.ubtn--default {
  height: 40px;
  padding: 0 24px;
}
.ubtn--sm {
  height: 32px;
  padding: 0 14px;
  font-size: 13px;
  border-radius: 6px;
}
.ubtn--lg {
  height: 48px;
  padding: 0 32px;
  font-size: 15px;
  border-radius: 10px;
}
.ubtn--icon {
  width: 40px;
  height: 40px;
  padding: 0;
  font-size: 18px;
}
.ubtn--sm-icon {
  width: 32px;
  height: 32px;
  padding: 0;
  font-size: 15px;
  border-radius: 8px;
}
.ubtn--lg-icon {
  width: 48px;
  height: 48px;
  padding: 0;
  font-size: 22px;
  border-radius: 10px;
}

/* variants */
.ubtn--default {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}
.ubtn--default:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.15);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

.ubtn--primary {
  background: var(--accent-primary);
  color: #fff;
  box-shadow: 0 2px 8px rgba(0, 102, 204, 0.3);
}
.ubtn--primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.ubtn--danger {
  background: #ef4444;
  color: #fff;
  box-shadow: 0 2px 8px rgba(239, 68, 68, 0.3);
}
.ubtn--danger:hover:not(:disabled) {
  background: #f87171;
}

.ubtn--ghost {
  background: transparent;
  color: var(--text-secondary);
  box-shadow: none;
}
.ubtn--ghost:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.ubtn--link {
  background: transparent;
  color: var(--accent-primary);
  box-shadow: none;
  padding: 0;
  height: auto;
  border-radius: 0;
}
.ubtn--link:hover:not(:disabled) {
  color: var(--accent-hover);
  text-decoration: underline;
}

.ubtn--outline {
  background: transparent;
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  box-shadow: none;
}
.ubtn--outline:hover:not(:disabled) {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
}

.ubtn--tool {
  background: transparent;
  color: var(--text-muted);
  box-shadow: none;
}
.ubtn--tool:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.ubtn--success {
  background: rgba(34, 197, 94, 0.15);
  color: #22c55e;
}
.ubtn--success:hover:not(:disabled) {
  background: #22c55e;
  color: #fff;
}

/* 按压缩放（全局 animations.css 也有，这里 scoped 优先） */
.ubtn:active:not(:disabled) {
  transform: scale(0.96);
}
</style>