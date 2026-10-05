<script setup lang="ts">
/**
 * M3 导航栏（Navigation Rail）。
 *
 * 用 @m3e/web 的原生 `<m3e-nav-rail>` + `<m3e-nav-item>`：选中指示器、图标/标签
 * 配色、键盘导航全部由组件按 M3 规范处理。
 *
 * 选中态**声明式绑定**（`:selected="item.id === active"`），由路由驱动：
 * 这样首次挂载即处于正确选中态，不依赖"元素是否已升级/`items` 是否已收集"的时序，
 * 也不会出现"点了一下又变灰"（那是手动按索引同步 selected 时与组件内部选择状态打架导致的）。
 */
export interface NavItem {
  id: string;
  icon: string;
  label: string;
}

defineProps<{
  items: NavItem[];
  active: string;
}>();

const emit = defineEmits<{ select: [id: string] }>();
</script>

<template>
  <m3e-nav-rail class="rail">
    <m3e-nav-item
      v-for="item in items"
      :key="item.id"
      :selected="item.id === active"
      @click="emit('select', item.id)"
    >
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
