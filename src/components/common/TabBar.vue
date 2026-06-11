<script setup lang="ts">
defineProps<{
  tabs: { value: string; label: string; icon?: string }[];
  activeTab: string;
}>();
const emit = defineEmits<{
  'update:activeTab': [value: string];
}>();
</script>

<template>
  <div class="tab-bar">
    <button
      v-for="tab in tabs"
      :key="tab.value"
      :class="['tab-btn', { active: activeTab === tab.value }]"
      @click="emit('update:activeTab', tab.value)"
    >
      <i v-if="tab.icon" :class="['bi', tab.icon]"></i>
      <span>{{ tab.label }}</span>
    </button>
    <div v-if="$slots.right" class="tab-bar-right">
      <slot name="right" />
    </div>
  </div>
</template>

<style scoped>
.tab-bar {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-bottom: 16px;
}

.tab-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 20px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  transition: all 0.15s ease;
}

.tab-btn i {
  font-size: 15px;
}

.tab-btn:hover {
  color: var(--text-secondary);
  background: var(--bg-hover);
  border-radius: 8px 8px 0 0;
}

.tab-btn.active {
  color: var(--accent-primary);
  border-bottom-color: var(--accent-primary);
}

.tab-bar-right {
  margin-left: auto;
  display: flex;
  align-items: center;
}
</style>