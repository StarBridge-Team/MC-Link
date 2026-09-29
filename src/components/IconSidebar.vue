<script setup lang="ts">
interface IconSidebarProps {
  activeIcon: string;
  iconItems: Array<{ id: string; icon: string; title: string }>;
  playerName: string;
}

defineProps<IconSidebarProps & { showToast: (msg: string) => void }>();
const emit = defineEmits<{
  iconChange: [icon: string];
}>();
</script>

<template>
  <el-menu
    class="icon-sidebar"
    :default-active="activeIcon"
    :collapse="true"
    @select="(idx: string) => emit('iconChange', idx)"
  >
    <el-menu-item
      v-for="item in iconItems"
      :key="item.id"
      :index="item.id"
      :title="item.title"
    >
      <i :class="['bi', item.icon]"></i>
    </el-menu-item>
  </el-menu>
</template>

<style scoped>
.icon-sidebar {
  width: 60px;
  border-right: none;
  background: transparent;
  padding: var(--sp-3) 0;
  -webkit-app-region: no-drag;
  flex-shrink: 0;
}

.icon-sidebar :deep(.el-menu-item) {
  width: 40px;
  height: 40px;
  margin: 0 auto var(--sp-3);
  border-radius: var(--r-md);
  padding: 0 !important;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--fs-2xl);
  color: var(--text-muted);
}

.icon-sidebar :deep(.el-menu-item:hover) {
  background: var(--bg-hover);
  color: var(--text-secondary);
}

.icon-sidebar :deep(.el-menu-item.is-active) {
  background: var(--bg-soft-hover);
  color: var(--text-primary);
}

.icon-sidebar :deep(.el-menu-item.is-active)::before {
  content: '';
  position: absolute;
  left: -10px;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 20px;
  background: var(--accent-primary);
  border-radius: 0 3px 3px 0;
}

.icon-sidebar :deep(.el-menu-item .bi) {
  font-size: var(--fs-2xl);
}
</style>
