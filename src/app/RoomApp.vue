<script setup lang="ts">
/**
 * 房间窗口外壳。
 *
 * 会话 ID 从**窗口 label**（`room-<sessionId>`）取，不从 URL 取：
 * label 是 Rust 侧创建窗口时写死的身份，路由参数只是镜像。
 * 这样窗口被拖到别的屏幕、刷新、甚至被系统恢复，都不会丢上下文。
 */
import { computed, onMounted, ref } from "vue";
// `getCurrentWindow()` 是纯 JS 读取注入的窗口元数据（label），不走 IPC，因此不受
// capability 限制；受限制的是 `close()` / `startDragging()` 那类插件命令。
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useI18n } from "vue-i18n";

import { closeSelfWindow, dragWindow } from "../lib/api/window";
import RoomView from "../views/RoomView.vue";
import { useRoomsStore } from "../stores/rooms";
import TitleBarButton from "../ui/TitleBarButton.vue";
import { useThemeBinding } from "../ui/themeBinding";

const { t } = useI18n();
const store = useRoomsStore();
const { settings, colorScheme, themeEl, scheme, color, variant, contrast } = useThemeBinding();

const sessionId = ref("");

function resolveSessionId(): string {
  const prefix = "room-";
  try {
    const label = getCurrentWindow().label;
    if (label.startsWith(prefix)) return label.slice(prefix.length);
  } catch {
    // 浏览器预览（vite dev 直接开 room.html）没有 Tauri 窗口，退回查询串。
  }
  return new URLSearchParams(window.location.search).get("session") ?? "";
}

const title = computed(() => {
  const session = store.get(sessionId.value);
  return session?.roomCode || t("room.title");
});

onMounted(async () => {
  sessionId.value = resolveSessionId();
  await settings.load();
  settings.applyAll();
  colorScheme.attach(themeEl.value);
  if (!sessionId.value) return;
  try {
    await store.pull(sessionId.value);
  } catch (e) {
    // 事件桥会继续补；这里失败只说明"挂载时后端还没准备好"。
    console.warn("[room] 拉取会话失败", e);
  }
});

/**
 * 自绘标题栏拖动：调后端 `drag_window`，不用 `data-tauri-drag-region`。
 *
 * 那个属性背后是 `plugin:window|start_dragging`，需要给房间窗口单独配 capability；
 * 漏配时没有任何报错，表现就是"标题栏拖不动"。自己的命令不受 ACL 限制。
 */
function startDrag() {
  void dragWindow().catch(() => undefined);
}

function closeWindow() {
  void closeSelfWindow().catch(() => undefined);
}
</script>

<template>
  <m3e-theme
    ref="themeEl"
    :color="color"
    :variant="variant"
    :contrast="contrast"
    :scheme="scheme"
    motion="standard"
  >
    <div class="room-window">
      <!-- 拖拽区只包标题，按钮在它外面，避免点关闭时同时触发拖动 -->
      <header class="room-window__bar">
        <div class="room-window__drag" @pointerdown="startDrag">
          <span class="room-window__title">{{ title }}</span>
        </div>
        <!-- 与主窗口同一个按钮组件：尺寸、圆角、悬停红都一致 -->
        <TitleBarButton
          icon="close"
          danger
          :title="t('titlebar.close')"
          @click="closeWindow"
        />
      </header>

      <RoomView v-if="sessionId" :session-id="sessionId" />
      <div v-else class="room-window__missing">{{ t("room.missing") }}</div>
    </div>
  </m3e-theme>
</template>

<style scoped>
.room-window {
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  /* 与主窗口同一套底色/材质变量（由 useSettings.applyAll 写入 root）。
   *
   * 两个变量都给兜底值：`var(--app-opacity)` 缺失时 `calc()` 整条声明会失效，
   * 结果就是「透明窗口 + 什么都没有」——看起来像窗口没加载。兜底成 1 至少是实心色。 */
  background: color-mix(
    in srgb,
    var(--app-bg, var(--surface)) calc(var(--app-opacity, 1) * 100%),
    transparent
  );
  backdrop-filter: blur(var(--app-material-blur, 0px));
  -webkit-backdrop-filter: blur(var(--app-material-blur, 0px));
}

.room-window__bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  height: var(--titlebar-height);
  padding: 0 var(--sp-2) 0 var(--sp-4);
  color: var(--text-secondary);
  user-select: none;
}

.room-window__drag {
  flex: 1;
  min-width: 0;
  height: 100%;
  display: flex;
  align-items: center;
  /* 拖动区给个默认光标，避免看起来像可点击的文字 */
  cursor: default;
}

.room-window__title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}

.room-window__missing {
  flex: 1;
  display: grid;
  place-items: center;
  color: var(--text-muted);
  font-size: var(--fs-sm);
}
</style>
