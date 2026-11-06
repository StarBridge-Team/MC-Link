<script setup lang="ts">
import { computed, onMounted } from "vue";

import { openSettingsWindow } from "../../lib/api/window";
import { useAccountStore } from "../../stores/account";

/**
 * 左侧导航下方的快捷入口：任务中心 / 账号（头像）/ 设置。
 *
 * # 为什么在 Nav 下方、竖着排、但不是 Nav
 *
 * 用户要求它"像 Nav 一样竖向排列"，位置在左下角——因此它和 `NavRail` 同宽、
 * 同一条竖线，视觉上像导航的延伸。
 *
 * 但它**不能是导航项**：
 * - 「设置」打开独立窗口，不是主窗口路由，用 `m3e-nav-item` 会让它看起来像当前选中页；
 * - 「任务中心」是浮层；
 * - 「账号」是入口 + 身份显示。
 *
 * 所以这里用普通按钮，只是排布与 Nav 对齐。
 */
const emit = defineEmits<{ tasks: [] }>();

const account = useAccountStore();

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
    <button class="dock__item" type="button" title="任务中心" @click="emit('tasks')">
      <i class="material-symbols-rounded">task_alt</i>
      <span class="dock__label">任务</span>
    </button>

    <button
      class="dock__item"
      type="button"
      title="账号"
      @click="openSettingsWindow('general')"
    >
      <m3e-avatar class="dock__avatar">{{ initial }}</m3e-avatar>
      <span class="dock__label">账号</span>
    </button>

    <button class="dock__item" type="button" title="设置" @click="openSettingsWindow()">
      <i class="material-symbols-rounded">settings</i>
      <span class="dock__label">设置</span>
    </button>
  </div>
</template>

<style scoped>
.dock {
  /* 与 Nav 同宽、同一条竖线；不再绝对定位，由父级 flex 排在 Nav 下方。 */
  flex: none;
  width: var(--rail-width);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-1);
  padding: var(--sp-2) 0 var(--sp-3);
}

.dock__item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  width: 56px;
  padding: var(--sp-2) 0;
  border: none;
  border-radius: var(--shape-corner-m, 12px);
  background: transparent;
  color: var(--on-surface);
  cursor: pointer;
}

.dock__item:hover {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
}

.dock__label {
  font-size: 11px;
  line-height: 1.2;
  color: var(--text-secondary);
}

.dock__avatar {
  --m3e-avatar-size: 28px;
}
</style>
