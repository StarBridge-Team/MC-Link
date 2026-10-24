<script setup lang="ts">
/**
 * 房间窗口外壳。
 *
 * 会话 ID 从**窗口 label**（`room-<sessionId>`）取，不从 URL 取：
 * label 是 Rust 侧创建窗口时写死的身份，路由参数只是镜像。
 * 这样窗口被拖到别的屏幕、刷新、甚至被系统恢复，都不会丢上下文。
 */
import { computed, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useI18n } from "vue-i18n";

import RoomView from "../views/RoomView.vue";
import { useRoomsStore } from "../stores/rooms";
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

function closeWindow() {
  try {
    void getCurrentWindow().close();
  } catch {
    /* 非 Tauri 环境忽略 */
  }
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
      <!-- 拖拽区：只给标题这一条，别加在容器上，否则里面的按钮全失效 -->
      <header class="room-window__bar" data-tauri-drag-region>
        <span class="room-window__title" data-tauri-drag-region>{{ title }}</span>
        <m3e-button variant="text" class="room-window__close" @click="closeWindow">
          <m3e-icon slot="icon" name="close" />
        </m3e-button>
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
  /* 与主窗口同一套底色/材质变量（由 useSettings.applyAll 写入 root）。 */
  background: color-mix(
    in srgb,
    var(--app-bg, var(--surface)) calc(var(--app-opacity) * 100%),
    transparent
  );
  backdrop-filter: blur(var(--app-material-blur));
  -webkit-backdrop-filter: blur(var(--app-material-blur));
}

.room-window__bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  height: 40px;
  padding: 0 var(--sp-2) 0 var(--sp-4);
  user-select: none;
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

.room-window__close {
  flex: none;
}

.room-window__missing {
  flex: 1;
  display: grid;
  place-items: center;
  color: var(--text-muted);
  font-size: var(--fs-sm);
}
</style>
