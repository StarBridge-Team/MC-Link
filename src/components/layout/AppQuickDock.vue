<script setup lang="ts">
import { computed, onMounted } from "vue";

import { openSettingsWindow } from "../../lib/api/window";
import { useAccountStore } from "../../stores/account";

/**
 * 左侧导航下方的快捷入口：任务中心 / 账号（头像）/ 设置。
 *
 * # 全部使用组件库组件
 *
 * 三项都是 `<m3e-icon-button>`（账号项内部放 `<m3e-avatar>`），不再手写
 * `<button>` + 自定义悬停/圆角——那等于在组件库之外又维护一套按钮规范。
 * 外层 `div` 只负责竖排布局（与 Nav 同宽、同一条竖线），不是"组件"。
 *
 * # 为什么不是 Nav
 *
 * 「设置」打开独立窗口，不是主窗口路由；用 `m3e-nav-item` 会让它看起来像当前选中的一页。
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
    <m3e-icon-button size="large" title="任务中心" @click="emit('tasks')">
      <m3e-icon name="task_alt"></m3e-icon>
    </m3e-icon-button>

    <m3e-icon-button size="large" title="账号" @click="openSettingsWindow('general')">
      <m3e-avatar class="dock__avatar">{{ initial }}</m3e-avatar>
    </m3e-icon-button>

    <m3e-icon-button size="large" title="设置" @click="openSettingsWindow()">
      <m3e-icon name="settings"></m3e-icon>
    </m3e-icon-button>
  </div>
</template>

<style scoped>
.dock {
  /* 只负责竖排：与 Nav 同宽、同一条竖线，间距交给组件自身。 */
  flex: none;
  width: var(--rail-width);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-2) 0 var(--sp-3);
}

.dock__avatar {
  --m3e-avatar-size: 28px;
}
</style>
