<script setup lang="ts">
/**
 * 设置分区卡片：图标 + 标题 + 说明 +（可选）右上角动作 + 内容插槽。
 *
 * 容器外壳用 @m3e/web 的 `<m3e-card variant="outlined">`——它由 M3 E 规范提供
 * 填充/描边/浮起三种变体与适配形状，颜色跟随 `m3e-theme` 的动态配色，比手写盒子协调。
 * 这里只负责头部排版（图标 + 标题 + 说明）和 grid 跨列。
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
  <m3e-card variant="elevated" actionable class="setting-card" :class="{ 'setting-card--wide': wide }">
    <div slot="content" class="sc-inner">
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
  </m3e-card>
</template>

<style scoped>
/* m3e-card 已提供背景 / 圆角 / 内边距；这里只管内容自身的纵向间距与跨列。 */
.setting-card {
  display: block;
}

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
