<script setup lang="ts">
/**
 * 设置分区卡片：图标 + 标题 + 说明 +（可选）右上角动作 + 内容插槽。
 *
 * 容器外壳直接用 Varlet 的 `<var-card variant="standard">`（基础变体，靠高程抬升，
 * 不填色）——它的 MD3 主题会把
 * 填充背景映射到 `surface-container-highest` 并随明暗切换，比手写的 `.card` 盒子协调。
 * 这里只负责头部排版（图标 + 标题 + 说明）和 grid 跨列，不再自己画边框/底色/圆角。
 *
 * 设置页里每个分区都用它，保证标题层级、间距只有一处定义。
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
  <var-card variant="standard" class="setting-card" :class="{ 'setting-card--wide': wide }">
    <div class="sc-inner">
      <header class="card__head">
        <span class="icon-badge"><i class="material-symbols-rounded">{{ icon }}</i></span>
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
    </div>
  </var-card>
</template>

<style scoped>
/* var-card 已提供填充背景 / 圆角 / 外边距；这里只管内容自身的纵向间距与跨列。 */
.sc-inner {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.setting-card--wide {
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
