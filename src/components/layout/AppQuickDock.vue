<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useRoute } from "vue-router";

import { openSettingsWindow } from "../../lib/api/window";
import { useAccountStore } from "../../stores/account";

/**
 * 主窗口右下角的快捷入口（与左侧导航同级）。
 *
 * 三项：任务中心 / 账号（头像）/ 设置。
 *
 * - **任务中心**：打开浮层，不占主窗口路由；
 * - **账号**：头像首字母来自后端账号（`Setting/account.yml`），点击进入设置「通用」分区；
 * - **设置**：普通图标按钮，**不用导航项组件**——设置是独立窗口，不是主窗口路由，
 *   用 `m3e-nav-item` 会让它看起来像"当前选中"的一页。
 *
 * 与左侧 Nav 的"选中态互通"：两边都读同一个 `route.name`。左侧有对应路由的项会高亮；
 * 这里三项都没有主窗口路由，因此不显示选中——这是刻意的，避免"两个入口同时高亮"。
 */
const emit = defineEmits<{ tasks: [] }>();

const route = useRoute();
const account = useAccountStore();

const active = computed(() => String(route.name ?? ""));

/** 头像文字：取玩家名首字符（离线账号没有真实头像图源）。 */
const initial = computed(() => {
  const name = account.displayName() || "?";
  return name.slice(0, 1).toUpperCase();
});

onMounted(() => {
  // 玩家名来自后端；失败不阻塞界面（显示占位 "?"）。
  void account.load().catch(() => undefined);
});
</script>

<template>
  <div class="dock">
    <button
      class="dock__item"
      type="button"
      :class="{ 'is-active': active === 'tasks' }"
      title="任务中心"
      @click="emit('tasks')"
    >
      <i class="material-symbols-rounded">task_alt</i>
    </button>

    <button
      class="dock__item dock__item--avatar"
      type="button"
      title="账号"
      @click="openSettingsWindow('general')"
    >
      <m3e-avatar class="dock__avatar">{{ initial }}</m3e-avatar>
    </button>

    <button
      class="dock__item"
      type="button"
      title="设置"
      @click="openSettingsWindow()"
    >
      <i class="material-symbols-rounded">settings</i>
    </button>
  </div>
</template>

<style scoped>
.dock {
  position: absolute;
  right: var(--sp-4);
  bottom: var(--sp-4);
  z-index: 5;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-1) var(--sp-2);
  border-radius: 999px;
  background: color-mix(in srgb, var(--surface-container-high) 88%, transparent);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  box-shadow: 0 2px 10px color-mix(in srgb, #000 18%, transparent);
}

.dock__item {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--on-surface);
  cursor: pointer;
}

.dock__item:hover {
  background: color-mix(in srgb, var(--on-surface) 10%, transparent);
}

.dock__item.is-active {
  background: var(--secondary-container);
  color: var(--on-secondary-container);
}

/* 头像比图标略大，但仍然在同一个胶囊里。 */
.dock__item--avatar {
  width: 40px;
  height: 40px;
}

.dock__avatar {
  --m3e-avatar-size: 32px;
}
</style>
