<script setup lang="ts">
import type { WidgetNode } from "../../lib/api/types";

/**
 * 声明式挂件节点的渲染器。
 *
 * 只认后端白名单里的类型（`plugin/widget.rs` 的 `KINDS`）。这里**刻意不提供**
 * `v-html`、动态组件名、外部 URL 之类的入口——渲染层的自由度就是安全边界。
 *
 * 自引用渲染 `row` / `column` 的 children；Vue 允许组件按文件名引用自己。
 */
const props = defineProps<{
  node: WidgetNode;
  /** 按钮点击：交给宿主统一上报。 */
  onAction: (action: string) => void;
}>();
</script>

<template>
  <span v-if="node.type === 'text'" class="w-text">{{ node.text }}</span>

  <h4 v-else-if="node.type === 'heading'" class="w-heading">{{ node.text }}</h4>

  <span
    v-else-if="node.type === 'badge'"
    class="w-badge"
    :class="node.tone ? `w-badge--${node.tone}` : ''"
  >
    {{ node.text }}
  </span>

  <button
    v-else-if="node.type === 'button'"
    type="button"
    class="w-button"
    :class="node.tone ? `w-button--${node.tone}` : ''"
    :disabled="node.disabled"
    @click="node.action && props.onAction(node.action)"
  >
    {{ node.label || node.text }}
  </button>

  <div v-else-if="node.type === 'row'" class="w-row">
    <WidgetNodeView
      v-for="(child, index) in node.children ?? []"
      :key="child.id ?? index"
      :node="child"
      :on-action="props.onAction"
    />
  </div>

  <div v-else-if="node.type === 'column'" class="w-column">
    <WidgetNodeView
      v-for="(child, index) in node.children ?? []"
      :key="child.id ?? index"
      :node="child"
      :on-action="props.onAction"
    />
  </div>

  <hr v-else-if="node.type === 'divider'" class="w-divider" />

  <div v-else-if="node.type === 'progress'" class="w-progress">
    <div class="w-progress__bar" :style="{ width: `${node.percent ?? 0}%` }" />
  </div>

  <div v-else-if="node.type === 'spacer'" class="w-spacer" />

  <!-- 未知类型不渲染：后端已经过滤过一轮，这里是第二道防线 -->
  <template v-else />
</template>

<style scoped>
.w-text {
  font-size: var(--fs-base);
  color: var(--text-primary);
  line-height: var(--lh-normal);
  white-space: pre-wrap;
  word-break: break-word;
}

.w-heading {
  margin: 0;
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.w-badge {
  display: inline-flex;
  align-items: center;
  padding: 0 var(--sp-2);
  border-radius: var(--shape-corner-s);
  background: var(--secondary-container);
  color: var(--on-secondary-container);
  font-size: var(--fs-caption);
}

.w-badge--ok {
  background: var(--tertiary-container);
  color: var(--on-tertiary-container);
}

.w-badge--warn {
  background: var(--error-container);
  color: var(--on-error-container);
}

.w-button {
  align-self: flex-start;
  padding: var(--sp-2) var(--sp-4);
  border: none;
  border-radius: var(--shape-corner-full, 999px);
  background: var(--primary);
  color: var(--on-primary);
  font-size: var(--fs-label);
  cursor: pointer;
}

.w-button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.w-button--danger {
  background: var(--error);
  color: var(--on-error);
}

.w-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  flex-wrap: wrap;
}

.w-column {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.w-divider {
  width: 100%;
  margin: 0;
  border: none;
  border-top: 1px solid var(--outline-variant, rgba(128, 128, 128, 0.24));
}

.w-progress {
  width: 100%;
  height: 6px;
  overflow: hidden;
  border-radius: 999px;
  background: var(--surface-variant, rgba(128, 128, 128, 0.2));
}

.w-progress__bar {
  height: 100%;
  border-radius: inherit;
  background: var(--primary);
  transition: width 200ms ease;
}

.w-spacer {
  flex: 1 1 auto;
  min-height: var(--sp-2);
}
</style>
