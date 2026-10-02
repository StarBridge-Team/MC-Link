<script setup lang="ts">
/**
 * M3 导航栏（Navigation Rail）：图标 + 文案竖排，选中项用
 * `secondary-container` 的胶囊指示器标示，与 M3 规范一致。
 *
 * 不用 Varlet 的 `var-rail-navigation`：它的选中态与槽位约定是给移动端横向
 * 折叠场景设计的，桌面端需要固定宽度 + 文案常显，自己写更可控。
 */
export interface NavItem {
  id: string;
  icon: string;
  label: string;
}

defineProps<{
  items: NavItem[];
  active: string;
}>();

const emit = defineEmits<{ select: [id: string] }>();
</script>

<template>
  <nav class="rail" role="tablist">
    <button
      v-for="item in items"
      :key="item.id"
      class="rail__item"
      :class="{ 'is-active': item.id === active }"
      type="button"
      role="tab"
      :aria-selected="item.id === active"
      :title="item.label"
      @click="emit('select', item.id)"
    >
      <span class="rail__indicator">
        <i :class="item.icon" />
      </span>
      <span class="rail__label">{{ item.label }}</span>
    </button>
  </nav>
</template>

<style scoped>
.rail {
  width: var(--rail-width);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-3) var(--sp-2);
  background: transparent;
}

.rail__item {
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
  transition: background-color var(--motion-medium) var(--ease-standard),
    color var(--motion-medium) var(--ease-standard);
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
  background: var(--secondary-container);
  color: var(--on-secondary-container);
}

.rail__label {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
