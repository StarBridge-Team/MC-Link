<script setup lang="ts">
import { ref, watch, nextTick } from "vue";

export interface QuickResult {
  icon: string;
  label: string;
  desc: string;
  value: string;
  disabled?: boolean;
}

export interface GuideItem {
  key: string;
  label: string;
}

const props = withDefaults(defineProps<{
  modelValue: string;
  results: QuickResult[];
  placeholder?: string;
  inputType?: "text" | "password";
  loading?: boolean;
  disabled?: boolean;
  guideItems?: GuideItem[];
  back?: boolean;
  modeIcon?: string;
}>(), {
  placeholder: "搜索…",
  inputType: "text",
  loading: false,
  disabled: false,
  guideItems: () => [],
  back: false,
  modeIcon: "",
});

const emit = defineEmits<{
  "update:modelValue": [value: string];
  select: [result: QuickResult];
  confirm: [];
  esc: [];
}>();

const inputRef = ref<HTMLInputElement>();
const selectedIndex = ref(-1);
const panelRef = ref<HTMLDivElement>();

watch(() => props.results.length, () => {
  selectedIndex.value = props.results.length > 0 ? 0 : -1;
});

function focusInput() {
  nextTick(() => inputRef.value?.focus());
}

function onInput(e: Event) {
  const val = (e.target as HTMLInputElement).value;
  emit("update:modelValue", val);
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    if (props.results.length > 0) {
      selectedIndex.value = (selectedIndex.value + 1) % props.results.length;
      scrollIntoView();
    }
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    if (props.results.length > 0) {
      selectedIndex.value = (selectedIndex.value - 1 + props.results.length) % props.results.length;
      scrollIntoView();
    }
  } else if (e.key === "Enter") {
    e.preventDefault();
    const sel = selectedIndex.value;
    if (sel >= 0 && sel < props.results.length && !props.results[sel].disabled) {
      emit("select", props.results[sel]);
    } else {
      emit("confirm");
    }
  } else if (e.key === "Escape") {
    e.preventDefault();
    emit("esc");
  }
}

function scrollIntoView() {
  nextTick(() => {
    const panel = panelRef.value;
    if (!panel) return;
    const active = panel.querySelector(".qs-item--sel") as HTMLElement | null;
    active?.scrollIntoView({ block: "nearest" });
  });
}

function onResultClick(r: QuickResult, idx: number) {
  if (r.disabled) return;
  selectedIndex.value = idx;
  emit("select", r);
  focusInput();
}

defineExpose({ focusInput });
</script>

<template>
  <div class="qs" @click="focusInput">
    <div class="qs-panel" ref="panelRef">
      <!-- 搜索区 -->
      <div class="qs-search">
        <button v-if="back" class="qs-back" @mousedown.prevent @click="emit('esc')">
          <i class="bi bi-chevron-left"></i>
        </button>
        <div class="qs-search-icon">
          <i v-if="modeIcon" :class="['bi', modeIcon]"></i>
          <i v-else :class="inputType === 'password' ? 'bi bi-lock' : 'bi bi-search'"></i>
        </div>
        <input
          ref="inputRef"
          :type="inputType"
          :value="modelValue"
          :placeholder="placeholder"
          :disabled="disabled"
          class="qs-search-input"
          autofocus
          autocomplete="off"
          spellcheck="false"
          @input="onInput"
          @keydown="onKeydown"
        />
      </div>

      <!-- 结果区 -->
      <div v-if="results.length > 0 || loading || (inputType === 'text' && modelValue.trim())" class="qs-results">
        <div v-if="loading" class="qs-loading">
          <span class="qs-loading-dot"></span>
          <span class="qs-loading-dot"></span>
          <span class="qs-loading-dot"></span>
        </div>

        <template v-else-if="results.length > 0">
          <div
            v-for="(r, i) in results"
            :key="i"
            :class="['qs-item', {
              'qs-item--sel': selectedIndex === i,
              'qs-item--dis': r.disabled,
            }]"
            @mousedown.prevent="onResultClick(r, i)"
          >
            <div class="qs-item-icon">
              <i :class="['bi', r.icon]"></i>
            </div>
            <div class="qs-item-body">
              <span class="qs-item-label">{{ r.label }}</span>
              <span class="qs-item-desc">{{ r.desc }}</span>
            </div>
            <div v-if="!r.disabled" class="qs-item-hint">
              <i class="bi bi-arrow-return-left"></i>
            </div>
          </div>
        </template>

        <div v-else-if="modelValue.trim()" class="qs-empty">
          未找到匹配项
        </div>
      </div>

      <!-- 引导栏 -->
      <div v-if="guideItems.length > 0" class="qs-guide">
        <span v-for="g in guideItems" :key="g.key" class="qs-guide-item">
          <kbd>{{ g.key }}</kbd>
          <span>{{ g.label }}</span>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* ===== 容器 ===== */
.qs {
  width: 100%;
  max-width: 620px;
  margin: 0 auto;
  cursor: text;
}

/* ===== 面板 ===== */
.qs-panel {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 16px;
  box-shadow: 0 8px 40px rgba(0, 0, 0, 0.12), 0 2px 8px rgba(0, 0, 0, 0.06);
  overflow: hidden;
  max-height: 480px;
  display: flex;
  flex-direction: column;
}

/* ===== 搜索栏 ===== */
.qs-search {
  display: flex;
  align-items: center;
  padding: 4px 6px;
  flex-shrink: 0;
}

.qs-back {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 16px;
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.1s ease;
}
.qs-back:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.qs-search-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  flex-shrink: 0;
  color: var(--text-muted);
  font-size: 15px;
}

.qs-search-input {
  flex: 1;
  padding: 10px 8px 10px 0;
  font-size: 15px;
  background: transparent;
  border: none;
  outline: none;
  color: var(--text-primary);
  min-width: 0;
}
.qs-search-input::placeholder {
  color: var(--text-muted);
}
.qs-search-input:disabled {
  opacity: 0.5;
}

/* ===== 结果区 ===== */
.qs-results {
  overflow-y: auto;
  padding: 4px 6px;
  flex: 1;
  min-height: 0;
}

/* ---- 加载 ---- */
.qs-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 20px 0;
}

.qs-loading-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--accent-primary);
  animation: qs-bounce 1.2s ease-in-out infinite;
}
.qs-loading-dot:nth-child(2) { animation-delay: 0.2s; }
.qs-loading-dot:nth-child(3) { animation-delay: 0.4s; }

@keyframes qs-bounce {
  0%, 80%, 100% { transform: scale(0.6); opacity: 0.4; }
  40% { transform: scale(1); opacity: 1; }
}

/* ---- 空 ---- */
.qs-empty {
  text-align: center;
  padding: 24px 0;
  color: var(--text-muted);
  font-size: 13px;
}

/* ---- 结果项 ---- */
.qs-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 10px;
  cursor: pointer;
  user-select: none;
  transition: background 0.08s ease;
}
.qs-item--sel {
  background: var(--accent-primary);
}
.qs-item--sel:hover {
  background: var(--accent-primary);
}
.qs-item:not(.qs-item--sel):not(.qs-item--dis):hover {
  background: var(--bg-hover);
}
.qs-item--dis {
  opacity: 0.45;
  cursor: default;
}

.qs-item-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  font-size: 16px;
}
.qs-item--sel .qs-item-icon {
  background: rgba(255, 255, 255, 0.2);
  color: #fff;
}
.qs-item:not(.qs-item--sel) .qs-item-icon {
  background: var(--accent-primary);
  color: #fff;
}

.qs-item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.qs-item-label {
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
}
.qs-item--sel .qs-item-label {
  color: #fff;
}
.qs-item:not(.qs-item--sel) .qs-item-label {
  color: var(--text-primary);
}

.qs-item-desc {
  font-size: 11.5px;
  line-height: 1.3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qs-item--sel .qs-item-desc {
  color: rgba(255, 255, 255, 0.75);
}
.qs-item:not(.qs-item--sel) .qs-item-desc {
  color: var(--text-muted);
}

.qs-item-hint {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  flex-shrink: 0;
  font-size: 13px;
}
.qs-item--sel .qs-item-hint {
  color: rgba(255, 255, 255, 0.6);
}
.qs-item:not(.qs-item--sel) .qs-item-hint {
  color: var(--text-muted);
  opacity: 0.4;
}

/* ===== 引导栏 ===== */
.qs-guide {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 14px;
  padding: 8px 0 10px;
  flex-shrink: 0;
}

.qs-guide-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.qs-guide-item kbd {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 22px;
  height: 18px;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--bg-hover);
  color: var(--text-secondary);
  font-size: 10px;
  font-weight: 600;
  font-family: inherit;
  letter-spacing: 0.3px;
}
</style>