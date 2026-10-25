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
  /* 窗口尺寸与面板严格相等：任何内缩边距都会露出 WebView 的底色（一片白）。 */
  width: 100vw;
  height: 100vh;
  /* 与 Rust 的 PADDING 对应：上下各 10px。 */
  padding: 10px 8px;
  box-sizing: border-box;
  overflow: hidden;
  /* 30px 圆角。窗口本身也被 `SetWindowRgn` 裁成同样的圆角（见
   * `window::widget::apply_round_region`），否则窗口的方形底色会从圆角外露出来。
   * 这里的 CSS 圆角负责内容自身的绘制边界。 */
  border-radius: 30px;
  /* 与主窗口卡片同款表面（`--card-bg`）：比 `surface-container` 更"浮"一层，
   * 在深色/低对比配色下不会糊成一片。 */
  background: var(--card-bg, var(--md-sys-color-surface-container-high, var(--surface)));
  /* 不用阴影，改用 1px 描边界定边界。
   * 原因：面板与页面底色同属 surface 家族，明度差只有几个点，靠颜色分不开；
   * 而阴影在透明 + 圆角裁剪的窗口上会被裁掉一半，反而更脏。 */
  border: 1px solid color-mix(in srgb, var(--on-surface) 12%, transparent);
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
