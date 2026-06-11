<script setup lang="ts">
interface Props {
  class?: string;
}

defineProps<Props>();
</script>

<template>
  <div
    :class="['ucard', $props.class]"
    v-bind="$attrs"
  >
    <slot />
  </div>
</template>

<style scoped>
.ucard {
  background: var(--bg-card, rgba(255, 255, 255, 0.06));
  border: 1px solid var(--border-color, rgba(255, 255, 255, 0.08));
  border-radius: 12px;
  padding: 20px 24px;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.1);
}
</style>