<script setup lang="ts">
/**
 * M3 导航栏（Navigation Rail）。
 *
 * 用 @m3e/web 的原生 `<m3e-nav-rail>` + `<m3e-nav-item>`：选中指示器、图标/标签
 * 配色、键盘导航全部由组件按 M3 规范处理，不再自己算滑块位置。
 *
 * 选中态由父组件（路由）驱动：`selected` 属性只在路由变化时同步一次，
 * 组件内部点选后由 rail 自己先切换，再通过 `change` 事件通知父组件改路由。
 */
import { onMounted, ref, watch } from "vue";

export interface NavItem {
  id: string;
  icon: string;
  label: string;
}

const props = defineProps<{
  items: NavItem[];
  active: string;
}>();

const emit = defineEmits<{ select: [id: string] }>();

const railEl = ref<HTMLElement & { items?: readonly (HTMLElement & { selected: boolean })[] }>();

/** 把 rail 内部的选中项对齐到当前路由。 */
function syncSelection() {
  const rail = railEl.value;
  const items = rail?.items;
  if (!items) return;
  props.items.forEach((item, i) => {
    const el = items[i] as (HTMLElement & { selected: boolean }) | undefined;
    if (el) el.selected = item.id === props.active;
  });
}

onMounted(syncSelection);
watch(() => props.active, () => requestAnimationFrame(syncSelection), { flush: "post" });

function onChange() {
  const rail = railEl.value;
  const items = rail?.items;
  if (!items) return;
  const idx = items.findIndex((el) => (el as { selected: boolean }).selected);
  if (items[idx]) emit("select", props.items[idx].id);
}
</script>

<template>
  <m3e-nav-rail ref="railEl" class="rail" @change="onChange">
    <m3e-nav-item v-for="item in items" :key="item.id">
      <m3e-icon slot="icon" :name="item.icon" />
      {{ item.label }}
    </m3e-nav-item>
  </m3e-nav-rail>
</template>

<style scoped>
.rail {
  flex-shrink: 0;
}
</style>
