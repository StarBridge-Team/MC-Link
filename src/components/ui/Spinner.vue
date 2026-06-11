<script setup lang="ts">
interface Props {
  size?: number;
  inline?: boolean;
  text?: string;
  class?: string;
}

defineProps<Props>();
</script>

<template>
  <span v-if="inline" class="uspinner-inline" :class="$props.class">
    <span class="uspinner" :style="{ width: size + 'px', height: size + 'px' }"></span>
    <slot />
    <template v-if="text">{{ text }}</template>
  </span>
  <div v-else class="uspinner-block" :class="$props.class">
    <span class="uspinner" :style="{ width: size + 'px', height: size + 'px' }"></span>
    <span v-if="text" class="uspinner-text">{{ text }}</span>
  </div>
</template>

<style scoped>
.uspinner {
  display: inline-block;
  border: 2px solid var(--border-color);
  border-top-color: var(--accent-primary);
  border-radius: 50%;
  animation: uspinner-spin 0.6s linear infinite;
  box-sizing: border-box;
}
@keyframes uspinner-spin {
  to { transform: rotate(360deg); }
}

.uspinner-inline {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--text-muted);
  font-size: 13px;
}

.uspinner-block {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 30px 0;
  color: var(--text-muted);
}

.uspinner-text {
  font-size: 14px;
}
</style>