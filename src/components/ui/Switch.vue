<script setup lang="ts">
interface Props {
  modelValue?: boolean;
  disabled?: boolean;
  class?: string;
}

defineProps<Props>();
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  change: [value: boolean];
}>();
</script>

<template>
  <label :class="['uswitch', $props.class]">
    <input
      type="checkbox"
      :checked="modelValue"
      :disabled="disabled"
      @change="emit('update:modelValue', ($event.target as HTMLInputElement).checked); emit('change', ($event.target as HTMLInputElement).checked)"
    />
    <span class="uswitch-slider"></span>
  </label>
</template>

<style scoped>
.uswitch {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 24px;
  cursor: pointer;
  flex-shrink: 0;
}
.uswitch input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}
.uswitch-slider {
  position: absolute;
  inset: 0;
  background: var(--bg-tertiary);
  border-radius: 12px;
  border: 1px solid var(--border-color);
  transition: all calc(0.2s * var(--anim-speed, 1)) ease;
}
.uswitch-slider::before {
  content: '';
  position: absolute;
  left: 2px;
  top: 2px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--text-muted);
  transition: all calc(0.2s * var(--anim-speed, 1)) ease;
}
.uswitch input:checked + .uswitch-slider {
  background: var(--accent-primary);
  border-color: var(--accent-primary);
}
.uswitch input:checked + .uswitch-slider::before {
  background: #fff;
  transform: translateX(20px);
}
.uswitch input:disabled + .uswitch-slider {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>