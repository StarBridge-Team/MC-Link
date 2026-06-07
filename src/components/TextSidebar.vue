<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { cn } from '../lib/utils';

interface TextSidebarProps {
  activeIcon: string;
  activeText: string;
}

const props = defineProps<TextSidebarProps>();
const emit = defineEmits<{
  textChange: [text: string]
}>();

const showSidebarAnimation = ref(false);
const previousIcon = ref('');

onMounted(() => {
  previousIcon.value = props.activeIcon;
  showSidebarAnimation.value = true;
});

watch(() => props.activeIcon, (newIcon, oldIcon) => {
  if (newIcon !== oldIcon) {
    previousIcon.value = newIcon;
    showSidebarAnimation.value = true;
    setTimeout(() => {
      showSidebarAnimation.value = false;
    }, 300);
  }
});
</script>

<template>
  <!-- 联机侧边栏 -->
  <div v-if="activeIcon === 'connect'" class="text-sidebar" :class="{ 'animate-in': showSidebarAnimation }">
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'join' })"
      @click="emit('textChange', 'join')"
    >
      加入房间
    </div>
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'create' })"
      @click="emit('textChange', 'create')"
    >
      创建房间
    </div>
  </div>
  
  <!-- 中继服务器侧边栏 -->
  <div v-if="activeIcon === 'relay'" class="text-sidebar" :class="{ 'animate-in': showSidebarAnimation }">
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'server' })"
      @click="emit('textChange', 'server')"
    >
      服务器设置
    </div>
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'logs' })"
      @click="emit('textChange', 'logs')"
    >
      运行日志
    </div>
  </div>

  <!-- 设置侧边栏 -->
  <div v-if="activeIcon === 'setting'" class="text-sidebar" :class="{ 'animate-in': showSidebarAnimation }">
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'account' })"
      @click="emit('textChange', 'account')"
    >
      <i class="bi bi-person-circle"></i>
      账号
    </div>
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'network' })"
      @click="emit('textChange', 'network')"
    >
      <i class="bi bi-wifi"></i>
      网络
    </div>
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'personalization' })"
      @click="emit('textChange', 'personalization')"
    >
      <i class="bi bi-palette"></i>
      个性化
    </div>
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'adapter' })"
      @click="emit('textChange', 'adapter')"
    >
      <i class="bi bi-plug"></i>
      适配器
    </div>
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'connector' })"
      @click="emit('textChange', 'connector')"
    >
      <i class="bi bi-hdd-network"></i>
      联机器
    </div>
    <div
      :class="cn('sidebar-text-item text-item', { active: activeText === 'about' })"
      @click="emit('textChange', 'about')"
    >
      <i class="bi bi-info-circle"></i>
      关于
    </div>
  </div>
</template>

<style scoped>
.text-sidebar {
  width: 180px;
  background: rgba(30, 30, 46, 0.12);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
  padding: 15px 10px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  -webkit-app-region: no-drag;
  transition: all 0.3s ease;
  border-right: 1px solid rgba(255, 255, 255, 0.03);
}

@media (prefers-color-scheme: light) {
  .text-sidebar {
    background: rgba(255, 255, 255, 0.25);
    border-right: 1px solid rgba(255, 255, 255, 0.15);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
  }
}

[data-theme="light"] .text-sidebar {
  background: rgba(255, 255, 255, 0.25);
  border-right: 1px solid rgba(255, 255, 255, 0.15);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
}

.sidebar-text-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.2s ease;
  font-size: 13px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.55);
  position: relative;
}

.sidebar-text-item:hover {
  background: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.8);
}

.sidebar-text-item.active {
  background: rgba(255, 255, 255, 0.12);
  color: #fff;
}

.sidebar-text-item.active::before {
  content: '';
  position: absolute;
  left: -10px;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 18px;
  background: var(--accent-primary);
  border-radius: 0 3px 3px 0;
}

.sidebar-text-item i {
  font-size: 16px;
  width: 20px;
  text-align: center;
}

@media (prefers-color-scheme: light) {
  .sidebar-text-item {
    color: rgba(0, 0, 0, 0.45);
  }
  .sidebar-text-item:hover {
    background: rgba(0, 0, 0, 0.06);
    color: rgba(0, 0, 0, 0.75);
  }
  .sidebar-text-item.active {
    background: rgba(0, 0, 0, 0.08);
    color: #000;
  }
}

[data-theme="light"] .sidebar-text-item {
  color: rgba(0, 0, 0, 0.45);
}

[data-theme="light"] .sidebar-text-item:hover {
  background: rgba(0, 0, 0, 0.06);
  color: rgba(0, 0, 0, 0.75);
}

[data-theme="light"] .sidebar-text-item.active {
  background: rgba(0, 0, 0, 0.08);
  color: #000;
}

/* 浮现动画 */
.text-item {
  opacity: 0;
  animation: itemEnter 0.4s ease-out forwards;
}

.text-item:nth-child(1) { animation-delay: 0s; }
.text-item:nth-child(2) { animation-delay: 0.05s; }
.text-item:nth-child(3) { animation-delay: 0.1s; }
.text-item:nth-child(4) { animation-delay: 0.15s; }
.text-item:nth-child(5) { animation-delay: 0.2s; }
.text-item:nth-child(6) { animation-delay: 0.25s; }

@keyframes itemEnter {
  0% {
    opacity: 0;
    transform: translateX(-15px) translateY(10px);
  }
  100% {
    opacity: 1;
    transform: translateX(0) translateY(0);
  }
}
</style>
