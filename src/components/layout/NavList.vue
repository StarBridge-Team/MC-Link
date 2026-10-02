<script setup lang="ts">
/**
 * 二级导航（M3 Navigation Drawer 风格）。
 *
 * 用 @m3e/web 的 `<m3e-nav-menu>` + `<m3e-nav-menu-item>`；选中态由父组件
 * （路由参数）驱动，组件内部不持有状态——否则路由前进/后退时会出现两套"当前项"。
 */
import { onMounted, ref, watch } from "vue";

export interface NavListItem {
  id: string;
  icon: string;
  label: string;
}

const props = defineProps<{
  items: NavListItem[];
  active: string;
}>();

const emit = defineEmits<{ select: [id: string] }>();

const menuEl = ref<HTMLElement & { items?: readonly (HTMLElement & { selected: boolean })[] }>();

function syncSelection() {
  const menu = menuEl.value;
  const items = menu?.items;
  if (!items) return;
  props.items.forEach((item, i) => {
    const el = items[i] as (HTMLElement & { selected: boolean }) | undefined;
    if (el) el.selected = item.id === props.active;
  });
}

onMounted(syncSelection);
watch(() => props.active, () => requestAnimationFrame(syncSelection), { flush: "post" });

// m3e-nav-menu 不派发 change 事件（只有 rail/bar 有），
// 因此由每个 item 的 click 主动派发路由跳转。
function onItemClick(id: string) {
  emit("select", id);
}
</script>

<template>
  <m3e-nav-menu ref="menuEl" class="nav-list">
    <m3e-nav-menu-item
      v-for="item in items"
      :key="item.id"
      @click="onItemClick(item.id)"
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
