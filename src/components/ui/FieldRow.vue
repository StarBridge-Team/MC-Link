<script setup lang="ts">
/**
 * 一行设置项：左侧「标题 + 说明」，右侧控件（默认插槽）。
 *
 * 用于开关、单个输入框这类窄控件；需要整行空间时不要用它，直接排版。
 */
defineProps<{
  label: string;
  desc?: string;
}>();
</script>

<template>
  <div class="field-row">
    <div class="field-row__label">
      <span class="field-row__title">{{ label }}</span>
      <span v-if="desc" class="field-row__desc">{{ desc }}</span>
    </div>
    <div class="field-row__control">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.field-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-4);
  min-height: 40px;
}

.field-row__label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.field-row__title {
  font-size: var(--fs-body);
  color: var(--text-primary);
}

.field-row__desc {
  font-size: var(--fs-label);
  color: var(--text-muted);
  line-height: var(--lh-normal);
}

.field-row__control {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}
</style>
