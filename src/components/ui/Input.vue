<script setup lang="ts">
interface Props {
  modelValue?: string | number;
  type?: "text" | "password" | "number";
  placeholder?: string;
  disabled?: boolean;
  readonly?: boolean;
  size?: "default" | "sm" | "lg";
  clearable?: boolean;
  class?: string;
}

withDefaults(defineProps<Props>(), {
  size: "default",
});

const emit = defineEmits<{
  "update:modelValue": [value: string | number];
  input: [value: string | number];
  clear: [];
  focus: [FocusEvent];
  blur: [FocusEvent];
}>();

function onInput(e: Event) {
  const val = (e.target as HTMLInputElement).value;
  emit("update:modelValue", val);
  emit("input", val);
}

function onClear() {
  emit("update:modelValue", "");
  emit("input", "");
  emit("clear");
}

function onFocus(e: FocusEvent) {
  emit("focus", e);
}

function onBlur(e: FocusEvent) {
  emit("blur", e);
}
</script>

<template>
  <div :class="['uinput-wrap', `uinput--${size}`, $props.class]">
    <slot name="prefix" />
    <input
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :readonly="readonly"
      class="uinput"
      @input="onInput"
      @focus="onFocus"
      @blur="onBlur"
      v-bind="$attrs"
    />
    <button
      v-if="clearable && modelValue !== '' && modelValue !== undefined && !disabled && !readonly"
      class="uinput-clear"
      @click="onClear"
      tabindex="-1"
    >
      <i class="bi bi-x-circle-fill"></i>
    </button>
    <slot name="suffix" />
  </div>
</template>

<style scoped>
.uinput-wrap {
  display: flex;
  align-items: center;
  position: relative;
  transition: all calc(0.2s * var(--anim-speed, 1)) ease;
  gap: 0;
}

/* ---- default ---- */
.uinput--default {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
}
.uinput--default:focus-within {
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 2px var(--shadow-accent);
}

/* ---- sm ---- */
.uinput--sm {
  background: transparent;
  border-radius: 0;
  border: none;
  border-bottom: 1px solid var(--border-color);
}
.uinput--sm:focus-within {
  border-bottom-color: var(--accent-primary);
}

/* ---- lg ---- */
.uinput--lg {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
}
.uinput--lg:focus-within {
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 2px var(--shadow-accent);
}

/* ---- input element ---- */
.uinput {
  width: 100%;
  padding: 10px 14px;
  border: none;
  border-radius: inherit;
  font-size: 13px;
  box-sizing: border-box;
  background: transparent;
  color: var(--text-primary);
  outline: none;
  font-family: inherit;
}

.uinput--sm .uinput {
  padding: 8px 0;
  font-size: 13px;
}

.uinput--lg .uinput {
  padding: 14px 18px;
  font-size: 15px;
}

.uinput::placeholder {
  color: var(--text-muted);
}
.uinput:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.uinput[readonly] {
  opacity: 0.7;
  cursor: default;
}

/* ---- clear button ---- */
.uinput-clear {
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px 10px 4px 0;
  font-size: 14px;
  flex-shrink: 0;
  opacity: 0.6;
  transition: opacity calc(0.2s * var(--anim-speed, 1)) ease;
}
.uinput-clear:hover {
  opacity: 1;
  color: var(--text-secondary);
}
</style>