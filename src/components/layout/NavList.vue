<script setup lang="ts">
/**
 * 二级导航（M3 Navigation Drawer 风格）。
 *
 * 用 @m3e/web 的 `<m3e-nav-menu>` + `<m3e-nav-menu-item>`。选中态由父组件（路由参数）
 * **声明式驱动**（`:selected="item.id === active"`），组件内部不持有状态——
 * 否则路由前进/后退时会出现两套"当前项"，也容易出现选中态不同步（点完变灰）。
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
  <m3e-nav-menu class="nav-list">
    <m3e-nav-menu-item
      v-for="item in items"
      :key="item.id"
      :selected="item.id === active"
      @click="emit('select', item.id)"
    >
      <m3e-icon slot="icon" :name="item.icon" />
      <span slot="label">{{ item.label }}</span>
    </m3e-nav-menu-item>
  </m3e-nav-menu>
</template>

<style scoped>
.nav-list {
  width: var(--nav-width);
  flex-shrink: 0;
  overflow-y: auto;
}
</style>
