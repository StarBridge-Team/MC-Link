<script setup lang="ts">
/**
 * 二级导航列表（M3 Navigation Drawer 风格）。
 *
 * 只负责展示与派发选中事件；选中态由父组件（路由参数）驱动，
 * 组件内部不持有状态——否则路由前进/后退时会出现两套"当前项"。
 */
export interface NavListItem {
  id: string;
  icon: string;
  label: string;
}

defineProps<{
  items: NavListItem[];
  active: string;
}>();

const emit = defineEmits<{ select: [id: string] }>();
</script>

<template>
  <aside class="nav-list">
    <button
      v-for="item in items"
      :key="item.id"
      class="nav-list__item"
      :class="{ 'is-active': item.id === active }"
      type="button"
      @click="emit('select', item.id)"
    >
      <i :class="item.icon" />
      <span class="nav-list__label">{{ item.label }}</span>
    </button>
  </aside>
</template>

<style scoped>
.nav-list {
  width: var(--nav-width);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: var(--sp-3) var(--sp-2);
  overflow-y: auto;
}

.nav-list__item {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  height: 44px;
  padding: 0 var(--sp-4);
  border-radius: var(--r-full);
  color: var(--text-secondary);
  font-size: var(--fs-body);
  text-align: left;
  transition: background-color var(--motion-medium) var(--ease-standard),
    color var(--motion-medium) var(--ease-standard);
}

.nav-list__item i {
  font-size: var(--fs-title);
  flex-shrink: 0;
}

.nav-list__item:hover {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
  color: var(--text-primary);
}

.nav-list__item.is-active {
  background: var(--secondary-container);
  color: var(--on-secondary-container);
  font-weight: var(--fw-semibold);
}

.nav-list__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
