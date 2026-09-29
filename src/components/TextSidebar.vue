<script setup lang="ts">
import { ref, watch, onMounted, computed } from 'vue';
import type { SettingSection } from '../lib/api/types';

interface TextSidebarProps {
  activeIcon: string;
  activeText: string;
  settingItems?: SettingSection[];
}

const props = defineProps<TextSidebarProps>();
const emit = defineEmits<{
  textChange: [text: string]
}>();

const showSidebarAnimation = ref(false);

onMounted(() => {
  showSidebarAnimation.value = true;
});

watch(() => props.activeIcon, (newIcon, oldIcon) => {
  if (newIcon !== oldIcon) {
    showSidebarAnimation.value = true;
    setTimeout(() => {
      showSidebarAnimation.value = false;
    }, 300);
  }
});

const connectItems = [
  { id: 'join', icon: '', label: '加入房间' },
  { id: 'create', icon: '', label: '创建房间' },
];

/** 服务端清单未到位时的兜底 */
const FALLBACK_SETTING_ITEMS: SettingSection[] = [
  { id: 'personalization', label: '个性化', icon: 'bi-palette' },
  { id: 'about', label: '关于', icon: 'bi-info-circle' },
];

const settingItems = computed(() => props.settingItems ?? FALLBACK_SETTING_ITEMS);
</script>

<template>
  <el-menu
    v-if="activeIcon === 'connect'"
    class="text-sidebar"
    :class="{ 'animate-in': showSidebarAnimation }"
    :default-active="activeText"
    @select="(idx: string) => emit('textChange', idx)"
  >
    <el-menu-item
      v-for="(item, i) in connectItems"
      :key="item.id"
      :index="item.id"
      :style="{ animationDelay: i * 0.05 + 's' }"
      class="text-item"
    >
      {{ item.label }}
    </el-menu-item>
  </el-menu>

  <el-menu
    v-if="activeIcon === 'setting'"
    class="text-sidebar"
    :class="{ 'animate-in': showSidebarAnimation }"
    :default-active="activeText"
    @select="(idx: string) => emit('textChange', idx)"
  >
    <el-menu-item
      v-for="(item, i) in settingItems"
      :key="item.id"
      :index="item.id"
      :style="{ animationDelay: i * 0.05 + 's' }"
      class="text-item"
    >
      <i v-if="item.icon" :class="['bi', item.icon]"></i>
      <span>{{ item.label }}</span>
    </el-menu-item>
  </el-menu>
</template>

<style scoped>
.text-sidebar {
  width: 180px;
  padding: var(--sp-4) var(--sp-3);
  border-right: none;
  background: transparent;
  gap: var(--sp-1);
  -webkit-app-region: no-drag;
  transition: all var(--motion-slow) var(--ease);
  flex-shrink: 0;
}

.text-sidebar :deep(.el-menu-item) {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  height: auto;
  line-height: 1.4;
  padding: var(--sp-3) var(--sp-4) !important;
  border-radius: var(--r-md);
  font-size: var(--fs-base);
  font-weight: var(--fw-medium);
  color: var(--text-muted);
  position: relative;
}

.text-sidebar :deep(.el-menu-item:hover) {
  background: var(--bg-hover);
  color: var(--text-secondary);
}

.text-sidebar :deep(.el-menu-item.is-active) {
  background: var(--bg-soft-hover);
  color: var(--text-primary);
}

.text-sidebar :deep(.el-menu-item.is-active)::before {
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

.text-sidebar :deep(.el-menu-item .bi) {
  font-size: var(--fs-xl);
  width: 20px;
  text-align: center;
}

.text-item {
  opacity: 0;
  animation: itemEnter calc(0.4s * var(--anim-speed, 1)) var(--ease-out) forwards;
}

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
