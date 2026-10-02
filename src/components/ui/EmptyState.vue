<script setup lang="ts">
/**
 * 空状态 / 占位提示。
 *
 * `icon` 用 Material Symbols 的 ligature 名（如 `inbox`）。
 */
defineProps<{
  icon: string;
  title: string;
  desc?: string;
}>();
</script>

<template>
  <div class="empty">
    <i class="material-symbols-rounded empty__icon">{{ icon }}</i>
    <h3 class="empty__title">{{ title }}</h3>
    <p v-if="desc" class="empty__desc">{{ desc }}</p>
    <div v-if="$slots.default" class="empty__actions">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--sp-2);
  padding: var(--sp-8) var(--sp-6);
  text-align: center;
}

.empty__icon {
  font-size: 44px;
  color: var(--text-muted);
  opacity: 0.6;
}

.empty__title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-secondary);
}

.empty__desc {
  max-width: 420px;
  font-size: var(--fs-base);
  color: var(--text-muted);
  line-height: var(--lh-loose);
}

.empty__actions {
  margin-top: var(--sp-3);
  display: flex;
  gap: var(--sp-2);
  flex-wrap: wrap;
  justify-content: center;
}
</style>
