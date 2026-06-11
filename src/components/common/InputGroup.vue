<script setup lang="ts">
defineProps<{
  label?: string;
  hint?: string;
  error?: string;
  required?: boolean;
}>();
</script>

<template>
  <div :class="['input-group', { 'input-group--error': error }]">
    <label v-if="label" class="input-group__label">
      {{ label }}
      <span v-if="required" class="input-group__required">*</span>
    </label>
    <div class="input-group__content">
      <slot />
    </div>
    <p v-if="hint && !error" class="input-group__hint">{{ hint }}</p>
    <p v-if="error" class="input-group__error">{{ error }}</p>
  </div>
</template>

<style scoped>
.input-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.input-group__label {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  letter-spacing: 0.3px;
}

.input-group__required {
  color: var(--accent-primary, #0066cc);
  margin-left: 2px;
}

.input-group__content {
  display: flex;
  align-items: center;
  gap: 8px;
}

.input-group__hint {
  margin: 0;
  font-size: 11.5px;
  color: var(--text-muted);
  line-height: 1.4;
}

.input-group__error {
  margin: 0;
  font-size: 11.5px;
  color: #ef4444;
  line-height: 1.4;
}

.input-group--error .input-group__label {
  color: #ef4444;
}
</style>