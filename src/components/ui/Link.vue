<script setup lang="ts">
interface Props {
  href?: string;
  class?: string;
}

defineProps<Props>();
</script>

<template>
  <a
    v-if="href"
    :href="href"
    target="_blank"
    rel="noopener noreferrer"
    :class="['ulink', $props.class]"
  >
    <slot />
  </a>
  <span v-else :class="['ulink', $props.class]">
    <slot />
  </span>
</template>

<style scoped>
.ulink {
  color: var(--accent-primary);
  cursor: pointer;
  text-decoration: none;
  transition: color 0.2s ease;
}
.ulink:hover {
  color: var(--accent-hover);
  text-decoration: underline;
}
</style>