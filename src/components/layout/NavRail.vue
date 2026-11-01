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
 *
 * # 底部菜单
 *
 * `bottomItems` 渲染在导航栏**底部**（账号 / 个性化 / 设置）。用第二个
 * `<m3e-nav-rail>` 而不是往主 rail 里塞占位元素：组件内部的排布未知，
 * 依赖 `margin-top: auto` 可能失效；独立一个 rail 则一定能贴在底部。
 */
export interface NavItem {
  id: string;
  icon: string;
  label: string;
}

defineProps<{
  items: NavItem[];
  active: string;
  /** 底部固定项；不传则不渲染底部区域。 */
  bottomItems?: NavItem[];
}>();

const emit = defineEmits<{ select: [id: string] }>();
</script>

<template>
  <div class="rail-wrap">
    <m3e-nav-rail class="rail rail--main">
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

    <m3e-nav-rail v-if="bottomItems && bottomItems.length > 0" class="rail rail--bottom">
      <m3e-nav-item
        v-for="item in bottomItems"
        :key="item.id"
        :selected="item.id === active"
        @click="emit('select', item.id)"
      >
        <m3e-icon slot="icon" :name="item.icon" />
        {{ item.label }}
      </m3e-nav-item>
    </m3e-nav-rail>
  </div>
</template>

<style scoped>
.rail-wrap {
  display: flex;
  flex-direction: column;
  height: 100%;
  flex-shrink: 0;
}

.rail {
  flex-shrink: 0;
}

.rail--main {
  flex: 1;
  min-height: 0;
}

/* 贴底：主 rail 占满剩余高度，底部 rail 自然落在下面。 */
.rail--bottom {
  margin-top: auto;
  padding-top: var(--sp-2);
}
</style>
