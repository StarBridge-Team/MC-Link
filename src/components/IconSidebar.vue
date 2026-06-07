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
  <div class="icon-sidebar">
    <div class="sidebar-top">
      <div
        v-for="item in iconItems"
        :key="item.id"
        :class="['sidebar-icon-item', { active: activeIcon === item.id }]"
        :title="item.title"
        @click="emit('iconChange', item.id)"
      >
        <i :class="['bi', item.icon]"></i>
      </div>
    </div>
  </div>
</template>

<style scoped>
.icon-sidebar {
  width: 60px;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 10px 0;
  -webkit-app-region: no-drag;
}

.sidebar-top {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}

.sidebar-icon-item {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.2s ease;
  font-size: 18px;
  color: rgba(255, 255, 255, 0.45);
  position: relative;
}

.sidebar-icon-item:hover {
  background: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.8);
}

.sidebar-icon-item.active {
  background: rgba(255, 255, 255, 0.12);
  color: #fff;
}

.sidebar-icon-item.active::before {
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

@media (prefers-color-scheme: light) {
  .sidebar-icon-item {
    color: rgba(0, 0, 0, 0.4);
  }
  .sidebar-icon-item:hover {
    background: rgba(0, 0, 0, 0.06);
    color: rgba(0, 0, 0, 0.75);
  }
  .sidebar-icon-item.active {
    background: rgba(0, 0, 0, 0.08);
    color: #000;
  }
}

[data-theme="light"] .sidebar-icon-item {
  color: rgba(0, 0, 0, 0.4);
}

[data-theme="light"] .sidebar-icon-item:hover {
  background: rgba(0, 0, 0, 0.06);
  color: rgba(0, 0, 0, 0.75);
}

[data-theme="light"] .sidebar-icon-item.active {
  background: rgba(0, 0, 0, 0.08);
  color: #000;
}
</style>
