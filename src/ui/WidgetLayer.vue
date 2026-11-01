<script setup lang="ts">
import MediaControlWidget from "../components/widget/MediaControlWidget.vue";

/**
 * 悬浮挂件层（B7）。
 *
 * 窗口内挂件的**统一容器**：内置挂件（当前是背景音视频控制面板）与未来的
 * `widget` 类插件挂件都挂在这里。放在窗口右下角，不参与主布局——
 * 挂件的出现/消失不该挤动页面内容。
 */
</script>

<template>
  <div class="widget-layer">
    <MediaControlWidget />
    <!-- 未来：`widget` 类插件在窗口内的挂件按清单在此渲染（mount = in-window）。 -->
  </div>
</template>

<style scoped>
.widget-layer {
  position: absolute;
  right: var(--sp-4);
  bottom: var(--sp-4);
  z-index: 5;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: var(--sp-2);
  pointer-events: none;
}

/* 挂件本体要能点击；容器本身不拦截事件（避免盖住页面右下角的操作）。 */
.widget-layer > * {
  pointer-events: auto;
}
</style>
