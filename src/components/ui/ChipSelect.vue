<script setup lang="ts" generic="T extends string">
/**
 * 单选芯片组（M3 chips / segmented buttons）。
 *
 * 泛型约束到 string：这里的取值都会直接落到后端字段（如 `theme_mode`、
 * `background_fit`），用字符串联合类型能在调用处挡住拼错的取值。
 *
 * 受控组件：只发事件，不自己改值，避免出现"界面已切换但后端没保存"。
 */
export interface ChipOption<T extends string> {
  value: T;
  label: string;
  icon?: string;
}

defineProps<{
  modelValue: T;
  options: ChipOption<T>[];
  /** 小号芯片，用于二级/从属选项。 */
  small?: boolean;
  disabled?: boolean;
}>();

const emit = defineEmits<{ "update:modelValue": [value: T] }>();
</script>

<template>
  <div class="chips">
    <button
      v-for="option in options"
      :key="option.value"
      class="chip"
      :class="{ 'is-active': option.value === modelValue, 'chip--small': small }"
      type="button"
      :disabled="disabled"
      @click="emit('update:modelValue', option.value)"
    >
      <i v-if="option.icon" class="material-symbols-rounded chip__icon">{{ option.icon }}</i>
      <span>{{ option.label }}</span>
    </button>
  </div>
</template>

<style scoped>
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 36px;
  padding: 0 var(--sp-4);
  border-radius: var(--r-sm);
  border: 1px solid var(--outline);
  color: var(--text-secondary);
  font-size: var(--fs-body);
  transition: background-color var(--motion-short) var(--ease-standard),
    border-color var(--motion-short) var(--ease-standard),
    color var(--motion-short) var(--ease-standard);
}

.chip--small {
  height: 30px;
  padding: 0 var(--sp-3);
  font-size: var(--fs-label);
}

/* 芯片图标：跟随芯片字号缩放（图标字体类默认 24px，会撑大芯片）。 */
.chip__icon {
  font-size: 1.25em;
  flex-shrink: 0;
}

.chip:hover:not(:disabled) {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
  color: var(--text-primary);
}

.chip.is-active {
  background: var(--secondary-container);
  border-color: transparent;
  color: var(--on-secondary-container);
  font-weight: var(--fw-medium);
}

.chip:disabled {
  opacity: 0.4;
  cursor: default;
}
</style>
