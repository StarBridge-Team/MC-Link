<script setup lang="ts">
interface Props {
  modelValue?: string;
  options: { value: string; label: string }[];
  name?: string;
  class?: string;
}

defineProps<Props>();
const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();
</script>

<template>
  <div :class="['uradio-group', $props.class]">
    <label
      v-for="opt in options"
      :key="opt.value"
      :class="['uradio', { 'uradio--active': modelValue === opt.value }]"
    >
      <input
        type="radio"
        :name="name"
        :value="opt.value"
        :checked="modelValue === opt.value"
        @change="emit('update:modelValue', opt.value)"
      />
      <span class="uradio-dot"></span>
      <span class="uradio-label">{{ opt.label }}</span>
    </label>
  </div>
</template>

<style scoped>
.uradio-group {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}
.uradio {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  font-size: 13px;
  color: var(--text-secondary);
}
.uradio input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}
.uradio-dot {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 2px solid var(--border-color);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: border-color 0.2s ease;
  flex-shrink: 0;
  box-sizing: border-box;
}
.uradio-dot::after {
  content: '';
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: transparent;
  transition: background 0.2s ease;
}
.uradio--active .uradio-dot {
  border-color: var(--accent-primary);
}
.uradio--active .uradio-dot::after {
  background: var(--accent-primary);
}
.uradio--active {
  color: var(--text-primary);
}
.uradio:hover .uradio-dot {
  border-color: var(--accent-primary);
}
</style>