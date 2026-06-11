<script setup lang="ts">
interface Props {
  modelValue?: number;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  showValue?: boolean;
  class?: string;
}

withDefaults(defineProps<Props>(), {
  min: 0,
  max: 100,
  step: 1,
});
const emit = defineEmits<{
  "update:modelValue": [value: number];
  input: [value: number];
}>();
</script>

<template>
  <div :class="['uslider', $props.class]">
    <input
      type="range"
      :min="min"
      :max="max"
      :step="step"
      :value="modelValue"
      :disabled="disabled"
      class="uslider-input"
      @input="emit('update:modelValue', Number(($event.target as HTMLInputElement).value)); emit('input', Number(($event.target as HTMLInputElement).value))"
    />
    <span v-if="showValue" class="uslider-value">{{ modelValue }}</span>
  </div>
</template>

<style scoped>
.uslider {
  display: flex;
  align-items: center;
  gap: 10px;
}
.uslider-input {
  flex: 1;
  max-width: 200px;
  -webkit-appearance: none;
  appearance: none;
  height: 6px;
  border-radius: 3px;
  background: var(--bg-tertiary);
  outline: none;
  border: none;
}
.uslider-input::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--accent-primary);
  cursor: pointer;
  border: 2px solid #fff;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
  transition: transform calc(0.15s * var(--anim-speed, 1)) ease;
}
.uslider-input::-webkit-slider-thumb:hover {
  transform: scale(1.15);
}
.uslider-input:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.uslider-value {
  min-width: 36px;
  text-align: right;
  font-family: 'Cascadia Code', monospace;
  font-size: 12px;
  color: var(--text-muted);
}
</style>