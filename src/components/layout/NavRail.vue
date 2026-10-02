<script setup lang="ts">
/**
 * M3 导航栏（Navigation Rail）：图标 + 文案竖排，选中项用一个**滑动**的胶囊指示器
 * 标示——active 指示会"滑"到目标位置（M3 规范），而不是每个项各自亮灭。
 *
 * 不用 Varlet 的 `var-rail-navigation`：它的选中态与槽位约定是给移动端横向
 * 折叠场景设计的，桌面端需要固定宽度 + 文案常显，自己写更可控。
 */
import { onMounted, ref, watch } from "vue";

export interface NavItem {
  id: string;
  icon: string;
  label: string;
}

const props = defineProps<{
  items: NavItem[];
  active: string;
}>();

const emit = defineEmits<{ select: [id: string] }>();

const railEl = ref<HTMLElement | null>(null);
const indicatorEls = ref<(HTMLElement | null)[]>([]);
const sliderY = ref(0);

function bindIndicator(el: unknown, index: number) {
  indicatorEls.value[index] = (el as HTMLElement | null) ?? null;
}

function syncSlider() {
  const idx = props.items.findIndex((i) => i.id === props.active);
  const el = indicatorEls.value[idx];
  if (el && railEl.value) sliderY.value = el.offsetTop;
}

onMounted(syncSlider);
watch(() => props.active, syncSlider, { flush: "post" });
</script>

<template>
  <nav ref="railEl" class="rail" role="tablist">
    <span class="rail__slider" :style="{ transform: `translate(-50%, ${sliderY}px)` }" />
    <button
      v-for="(item, index) in items"
      :key="item.id"
      class="rail__item"
      :class="{ 'is-active': item.id === active }"
      type="button"
      role="tab"
      :aria-selected="item.id === active"
      :title="item.label"
      @click="emit('select', item.id)"
    >
      <span :ref="(el) => bindIndicator(el, index)" class="rail__indicator">
        <i class="material-symbols-rounded">{{ item.icon }}</i>
      </span>
      <span class="rail__label">{{ item.label }}</span>
    </button>
  </nav>
</template>

<style scoped>
.rail {
  position: relative;
  width: var(--rail-width);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-3) var(--sp-2);
  background: transparent;
}

/* 滑动的选中指示器：在 active 项背后滑到目标位置 */
.rail__slider {
  position: absolute;
  left: 50%;
  top: 0;
  width: 56px;
  height: 32px;
  border-radius: var(--r-full);
  background: var(--secondary-container);
  transform: translate(-50%, 0);
  transition: transform var(--motion-medium) var(--ease-standard);
  z-index: 0;
  pointer-events: none;
}

.rail__item {
  position: relative;
  z-index: 1;
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 0;
  color: var(--text-secondary);
  font-size: var(--fs-label);
  transition: color var(--motion-medium) var(--ease-standard);
}

.rail__indicator {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 32px;
  border-radius: var(--r-full);
  font-size: 20px;
  transition: color var(--motion-medium) var(--ease-standard),
    background-color var(--motion-short) var(--ease-standard);
}

.rail__indicator .material-symbols-rounded {
  font-size: inherit;
}

.rail__item:hover .rail__indicator {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
  color: var(--text-primary);
}

.rail__item.is-active {
  color: var(--text-primary);
  font-weight: var(--fw-semibold);
}

.rail__item.is-active .rail__indicator {
  color: var(--on-secondary-container);
}

.rail__label {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
