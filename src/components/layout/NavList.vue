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

/* 二级导航从左往右逐项进入：进入设置页时列表像被"推"出来一样，
   与主内容区的交叉淡入形成方向感。初始 `both` 保证延迟期间不先闪一下。

   动画时长/延迟用项目令牌：`tokens.css` 里的 `.no-animations` 会把它们归零，
   所以「设置 → 动画」关掉后这里自动变成瞬现，不需要额外判断。 */
@keyframes nav-item-in {
  from {
    opacity: 0;
    transform: translateX(-14px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.nav-list m3e-nav-menu-item {
  animation: nav-item-in var(--motion-medium) var(--ease-standard) both;
}

.nav-list m3e-nav-menu-item:nth-child(1) { animation-delay: 0ms; }
.nav-list m3e-nav-menu-item:nth-child(2) { animation-delay: 45ms; }
.nav-list m3e-nav-menu-item:nth-child(3) { animation-delay: 90ms; }
.nav-list m3e-nav-menu-item:nth-child(4) { animation-delay: 135ms; }
.nav-list m3e-nav-menu-item:nth-child(5) { animation-delay: 180ms; }
.nav-list m3e-nav-menu-item:nth-child(n + 6) { animation-delay: 225ms; }
</style>
