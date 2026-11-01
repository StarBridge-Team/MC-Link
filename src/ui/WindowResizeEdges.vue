<script setup lang="ts">
/**
 * 无边框窗口的边缘缩放热区（**仅 Linux**）。
 *
 * # 为什么只有 Linux 需要
 *
 * Windows 上 tao 在 `WM_NCHITTEST` 里就把窗口四边/四角报成 `HTLEFT`/`HTTOPRIGHT` 之类，
 * 系统自己接管缩放，页面什么都不用做。GTK 侧没有这一手：`decorations: false` 的窗口
 * 没有任何地方能按下拖出尺寸，表现为"窗口拉不动边"。
 *
 * # 为什么是 6px
 *
 * 热区压在内容之上，太厚会抢掉标题栏按钮（关闭键贴着右边）、左侧导航轨的点击。
 * 6px 与多数 Linux 桌面自带边框的抓取宽度相当。
 *
 * # 为什么按下就 stopPropagation
 *
 * 顶边那条压在标题栏上，而标题栏自己也有"按下即拖动"（`drag_window`）。不拦住
 * 就会同时进入缩放拖动和窗口拖动两条状态，表现为窗口原地抖一下。
 */
import { computed } from "vue";

import { resizeDrag, type ResizeEdge } from "../lib/api/window";

/** 8 条边：四边 + 四角。四角单独存在，否则角落会被两条边切成一半。 */
const EDGES: ResizeEdge[] = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];

/**
 * UA 判断平台：WebView2（Windows）含 `Windows NT`，WebKitGTK 含 `Linux`。
 * 只在"要不要铺热区"这一个布尔上用，出错的后果只是多/少一层热区，不影响主路径。
 */
const enabled = computed(() => navigator.userAgent.includes("Linux"));

function onPointerDown(event: PointerEvent, edge: ResizeEdge) {
  // 只认左键：中键/右键在热区上没有意义，还会和上下文菜单抢。
  if (event.button !== 0) return;
  event.stopPropagation();
  event.preventDefault();
  void resizeDrag(edge).catch(() => undefined);
}
</script>

<template>
  <div v-if="enabled" class="resize-edges" aria-hidden="true">
    <div
      v-for="edge in EDGES"
      :key="edge"
      class="resize-edges__strip"
      :class="`resize-edges__strip--${edge}`"
      @pointerdown="onPointerDown($event, edge)"
    />
  </div>
</template>

<style scoped>
/* 父容器必须是 `position: relative`（`.shell` / `.room-window` 都是）。 */
.resize-edges {
  position: absolute;
  inset: 0;
  z-index: 60;
  /* 只有 6px 的边条吃事件，中间照常交给页面 */
  pointer-events: none;
  --resize-edge: 6px;
}

.resize-edges__strip {
  position: absolute;
  pointer-events: auto;
}

/* 四边：两端各让出一个角，避免与四条角热区重叠 */
.resize-edges__strip--n,
.resize-edges__strip--s {
  left: var(--resize-edge);
  right: var(--resize-edge);
  height: var(--resize-edge);
  cursor: ns-resize;
}

.resize-edges__strip--w,
.resize-edges__strip--e {
  top: var(--resize-edge);
  bottom: var(--resize-edge);
  width: var(--resize-edge);
  cursor: ew-resize;
}

.resize-edges__strip--n {
  top: 0;
}

.resize-edges__strip--s {
  bottom: 0;
}

.resize-edges__strip--w {
  left: 0;
}

.resize-edges__strip--e {
  right: 0;
}

/* 四角：尺寸与四边一致，压在最上层 */
.resize-edges__strip--nw,
.resize-edges__strip--ne,
.resize-edges__strip--sw,
.resize-edges__strip--se {
  width: var(--resize-edge);
  height: var(--resize-edge);
}

.resize-edges__strip--nw {
  top: 0;
  left: 0;
  cursor: nwse-resize;
}

.resize-edges__strip--ne {
  top: 0;
  right: 0;
  cursor: nesw-resize;
}

.resize-edges__strip--sw {
  bottom: 0;
  left: 0;
  cursor: nesw-resize;
}

.resize-edges__strip--se {
  bottom: 0;
  right: 0;
  cursor: nwse-resize;
}
</style>
