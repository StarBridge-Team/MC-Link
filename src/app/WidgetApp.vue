<script setup lang="ts">
/**
 * 托盘面板。
 *
 * 行高必须与 Rust 侧 `window::widget::ROW_HEIGHT` 一致（40px），
 * 因为窗口高度是后端按行数算好再显示的——这里量出来再 resize 会先闪一帧错误尺寸。
 */
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useRoomsStore } from "../stores/rooms";
import type { RoomSnapshot } from "../ipc/contracts/room";
import { listLocalGames } from "../lib/api/connect";
import type { LocalGameFound } from "../lib/api/types";
import { appShowConnect } from "../lib/api/widget";
import { exitApp, hideSelfWindow, showMainWindow } from "../lib/api/window";
import { useThemeBinding } from "../ui/themeBinding";

const { t } = useI18n();
const roomsStore = useRoomsStore();
const { settings, colorScheme, themeEl, scheme, color, variant, contrast } = useThemeBinding();

const recent = ref<LocalGameFound[]>([]);

const rooms = computed<RoomSnapshot[]>(() => roomsStore.active);

onMounted(async () => {
  await settings.load();
  settings.applyAll();
  colorScheme.attach(themeEl.value);
  void roomsStore.refresh().catch(() => undefined);
  try {
    // 与 Rust 侧 `panel_height` 读的是同一份扫描缓存，条数因此能对上。
    recent.value = (await listLocalGames()).slice(0, 3);
  } catch {
    /* 没扫到游戏就不显示这一段 */
  }
});

function openMain() {
  void showMainWindow().finally(() => void hideSelfWindow());
}

function openConnect(process?: string) {
  void appShowConnect(process).finally(() => void hideSelfWindow());
}

function focusRoom(room: RoomSnapshot) {
  void roomsStore.focus(room.sessionId).finally(() => void hideSelfWindow());
}

function quit() {
  void exitApp();
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
    <div class="panel">
      <button class="row" type="button" @click="openMain">
        <m3e-icon class="row__icon" name="open_in_new" />
        <span class="row__label">{{ t("widget.open") }}</span>
      </button>

      <button
        v-for="room in rooms"
        :key="room.sessionId"
        class="row"
        type="button"
        @click="focusRoom(room)"
      >
        <m3e-icon class="row__icon" name="meeting_room" />
        <span class="row__label">{{ room.roomCode || t("rooms.noCode") }}</span>
        <span class="row__meta">{{ room.members.length }}</span>
      </button>

      <button
        v-for="game in recent"
        :key="`${game.scanner}-${game.process}`"
        class="row"
        type="button"
        @click="openConnect(game.process)"
      >
        <m3e-icon class="row__icon" name="sports_esports" />
        <span class="row__label">{{ game.game_name }}</span>
      </button>

      <button class="row row--danger" type="button" @click="quit">
        <m3e-icon class="row__icon" name="logout" />
        <span class="row__label">{{ t("widget.exit") }}</span>
      </button>
    </div>
  </m3e-theme>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  /*
   * 四周留 12px：窗口比内容大这一圈，`box-shadow` 才不会被窗口边界裁掉。
   * 数值必须与 Rust 的 `window::widget::SHADOW_MARGIN` 一致。
   */
  margin: 12px;
  width: calc(100vw - 24px);
  height: calc(100vh - 24px);
  /* 与 Rust 的 PADDING 对应：上下各 10px。 */
  padding: 10px 8px;
  box-sizing: border-box;
  overflow: hidden;
  /* 28px：与 M3 对话框/大面板同档。
   *
   * 这里能画出圆角的前提是**窗口没套材质**（见 `window::widget::ensure_tray_panel`）：
   * 材质画在窗口矩形上，会把圆角和阴影整个盖掉。
   * 也别指望 `DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE)`：它只给系统预设
   * 半径（约 8px），且阴影按窗口矩形绘制——28px 与"有阴影"两者不可兼得，只能自己画。 */
  border-radius: 28px;
  background: var(--md-sys-color-surface-container, var(--surface));
  box-shadow:
    0 8px 24px rgba(0, 0, 0, 0.22),
    0 2px 6px rgba(0, 0, 0, 0.16);
  color: var(--text-primary);
}

/*
 * 行高固定 40px：窗口高度 = 20 + 40 × 行数，由后端算。
 * 这里用原生 button 只为"行"这一种形态——它不在组件库的语义里，
 * 且必须能被精确约束到 40px。
 */
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: none;
  height: 40px;
  padding: 0 12px;
  border: none;
  border-radius: 9999px;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1);
}

.row:hover,
.row:focus-visible {
  background: color-mix(in srgb, var(--primary) 12%, transparent);
}

.row--danger:hover {
  background: color-mix(in srgb, var(--error, #b3261e) 14%, transparent);
}

.row__icon {
  flex: none;
  font-size: 18px;
  color: var(--text-secondary);
}

.row__label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--fs-sm);
}

.row__meta {
  flex: none;
  font-size: var(--fs-xs);
  color: var(--text-muted);
}
</style>
