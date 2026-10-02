<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

/**
 * 颜色选择：色块（原生取色器）+ 十六进制输入。
 *
 * 用原生 `<input type="color">` 而不是自绘取色面板：桌面端系统取色器更顺手，
 * 也少一个需要维护的组件。十六进制输入框允许用户直接粘贴设计稿色值，
 * 但**只在格式合法时才提交**，避免把 `#GGG` 这类脏值写进配置。
 */
const props = defineProps<{
  modelValue: string;
  /** 色块尺寸。 */
  size?: "normal" | "large";
}>();

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const { t } = useI18n();

/** 原生取色器只接受 `#rrggbb`，`#rgb` 等简写要先补全。 */
const pickerValue = computed(() => normalizeHex(props.modelValue) ?? "#000000");

const draft = ref(props.modelValue);
watch(
  () => props.modelValue,
  (value) => {
    draft.value = value;
  },
);

function commitDraft() {
  const next = normalizeHex(draft.value);
  if (next && next !== props.modelValue) emit("update:modelValue", next);
  else draft.value = props.modelValue;
}

/** `#rgb` / `#rrggbb` / 无 `#` 都接受，其余返回 null。 */
function normalizeHex(input: string): string | null {
  let value = input.trim().toLowerCase();
  if (value && !value.startsWith("#")) value = `#${value}`;
  if (/^#[0-9a-f]{3}$/.test(value)) {
    return `#${value
      .slice(1)
      .split("")
      .map((c) => c + c)
      .join("")}`;
  }
  return /^#[0-9a-f]{6}$/.test(value) ? value : null;
}
</script>

<template>
  <div class="color-field" :class="{ 'color-field--large': size === 'large' }">
    <label class="color-field__swatch" :style="{ background: pickerValue }">
      <input
        class="color-field__input"
        type="color"
        :value="pickerValue"
        @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
      />
      <i class="bi bi-eyedropper" />
    </label>
    <input
      v-model="draft"
      class="color-field__hex mono"
      spellcheck="false"
      :aria-label="t('appearance.themeColor')"
      @change="commitDraft"
      @blur="commitDraft"
      @keyup.enter="commitDraft"
    />
  </div>
</template>

<style scoped>
.color-field {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
}

.color-field__swatch {
  position: relative;
  width: 32px;
  height: 32px;
  border-radius: var(--r-sm);
  border: 1px solid var(--outline-variant);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  overflow: hidden;
  color: #fff;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.45);
}

.color-field--large .color-field__swatch {
  width: 40px;
  height: 40px;
  border-radius: var(--r-md);
}

.color-field__input {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  opacity: 0;
  border: none;
  padding: 0;
  cursor: pointer;
}

.color-field__hex {
  width: 92px;
  height: 32px;
  padding: 0 var(--sp-2);
  border-radius: var(--r-sm);
  border: 1px solid var(--outline);
  background: transparent;
  color: var(--text-primary);
  text-transform: lowercase;
}

.color-field__hex:focus {
  outline: none;
  border-color: var(--primary);
}
</style>
