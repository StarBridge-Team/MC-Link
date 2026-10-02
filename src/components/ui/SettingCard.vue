<script setup lang="ts">
/**
 * 设置分区卡片：图标 + 标题 + 说明 +（可选）右上角动作 + 内容插槽。
 *
 * 设置页里每个分区都用它，保证标题层级、间距与圆角只有一处定义。
 * M3 里这类容器是 "filled card"：用 `surface-container-low` 而不是阴影。
 */
defineProps<{
  icon: string;
  title: string;
  desc?: string;
  /** 为 true 时占满整行（多列栅格里用）。 */
  wide?: boolean;
}>();
</script>

<template>
  <section class="card" :class="{ 'card--wide': wide }">
    <header class="card__head">
      <span class="icon-badge"><i :class="icon" /></span>
      <div class="card__titles">
        <h2 class="card__title">{{ title }}</h2>
        <p v-if="desc" class="card__desc">{{ desc }}</p>
      </div>
      <div v-if="$slots.action" class="card__action">
        <slot name="action" />
      </div>
    </header>
    <div v-if="$slots.default" class="card__body">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  padding: var(--sp-5);
  border-radius: var(--r-lg);
  background: var(--surface-container-low);
  border: 1px solid var(--outline-variant);
  transition: border-color var(--motion-medium) var(--ease-standard);
}

.card:hover {
  border-color: color-mix(in srgb, var(--primary) 40%, var(--outline-variant));
}

.card--wide {
  grid-column: 1 / -1;
}

.card__head {
  display: flex;
  align-items: flex-start;
  gap: var(--sp-3);
}

.card__titles {
  flex: 1;
  min-width: 0;
}

.card__title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  line-height: var(--lh-tight);
}

.card__desc {
  margin-top: 2px;
  font-size: var(--fs-label);
  color: var(--text-muted);
  line-height: var(--lh-normal);
}

.card__action {
  flex-shrink: 0;
}

.card__body {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}
</style>
