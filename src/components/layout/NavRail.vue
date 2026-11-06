<script setup lang="ts">
/**
 * M3 导航栏（Navigation Rail）。
 *
 * 用 @m3e/web 的原生 `<m3e-nav-rail>` + `<m3e-nav-item>`：选中指示器、图标/标签
 * 配色、键盘导航全部由组件按 M3 规范处理。
 *
 * 选中态**声明式绑定**（`:selected="item.id === active"`），由路由驱动：
 * 这样首次挂载即处于正确选中态，不依赖"元素是否已升级/`items` 是否已收集"的时序。
 *
 * # 小窗口
 *
 * 导航项数量固定，但小窗口下高度可能不够。这里让 rail **自己滚动**
 * （`max-height: 100%` + `overflow-y: auto`），而不是把整个窗口撑出滚动条。
 * 底部入口（账号/设置）已移到右下角的 `AppQuickDock`，不再挤占这里的高度。
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
  /* 高度跟随父容器，内容超出时自己滚（而不是溢出到窗口上）。 */
  max-height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
}
</style>
