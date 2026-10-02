<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
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

/**
 * 取色器拖动时的输入合并。
 *
 * 原生 `<input type="color">` 在拖动过程中每秒会触发几十次 `input`，而**每一次**都会让
 * 整条配色链路重来一遍：`<m3e-theme>` 重算 40+ 个颜色角色、重写整张 CSS 变量样式表，
 * 并且在改 color/contrast 时还会强制一次同步回流。这就是"改配色很卡"的主因。
 *
 * 用 `requestAnimationFrame` 合并到每帧最多一次：拖动时依然跟手（画面本来就是一帧
 * 一帧刷新的），但把事件洪水收敛掉了。
 */
let pendingColor: string | null = null;
let rafId = 0;

function onPickerInput(event: Event) {
  pendingColor = (event.target as HTMLInputElement).value;
  if (rafId) return;
  rafId = requestAnimationFrame(() => {
    rafId = 0;
    const next = pendingColor;
    pendingColor = null;
    if (next && next !== props.modelValue) emit("update:modelValue", next);
  });
}

onBeforeUnmount(() => {
  if (rafId) cancelAnimationFrame(rafId);
});
</script>

<template>
  <div class="color-field" :class="{ 'color-field--large': size === 'large' }">
    <label class="color-field__swatch" :style="{ background: pickerValue }">
      <input
        class="color-field__input"
        type="color"
        :value="pickerValue"
        @input="onPickerInput"
      />
      <i class="material-symbols-rounded">colorize</i>
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
