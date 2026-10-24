<script setup lang="ts">
/**
 * 房间总览：所有进行中的会话。
 *
 * 这是多房间的唯一入口——主窗口不承载任何单个房间的细节，
 * 每个房间的内容在它自己的窗口里（点击"打开窗口"聚焦过去）。
 */
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";

import EmptyState from "../components/ui/EmptyState.vue";
import { showError } from "../composables/useToast";
import type { RoomPhase, RoomSnapshot } from "../ipc/contracts/room";
import { useRoomsStore } from "../stores/rooms";

const { t } = useI18n();
const store = useRoomsStore();

const rooms = computed<RoomSnapshot[]>(() => store.active);

const PHASE_LABEL: Record<RoomPhase, string> = {
  starting: "room.phase.starting",
  linking: "room.phase.linking",
  online: "room.phase.online",
  stopping: "room.phase.stopping",
  closed: "room.phase.closed",
};

onMounted(() => {
  // 窗口重载后靠这一次拉取恢复；之后由事件增量更新。
  void store.refresh().catch((e) => console.warn("[rooms] 拉取房间列表失败", e));
});

function phaseLabel(phase: RoomPhase): string {
  return t(PHASE_LABEL[phase]);
}

function roleLabel(role: RoomSnapshot["role"]): string {
  return role === "host" ? t("connect.host") : t("connect.guest");
}

/** 已运行时长（mm:ss）。超过一小时显示 h:mm:ss。 */
function elapsed(createdAt: number): string {
  const seconds = Math.max(0, Math.floor((Date.now() - createdAt) / 1000));
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

async function focus(session: RoomSnapshot) {
  await store.focus(session.sessionId);
}

async function disconnect(session: RoomSnapshot) {
  try {
    await store.leave(session.sessionId);
  } catch (e) {
    showError(e instanceof Error ? e.message : String(e));
  }
}
</script>

<template>
  <div class="rooms">
    <div v-if="rooms.length === 0" class="rooms__empty">
      <EmptyState
        icon="meeting_room"
        :title="t('rooms.empty')"
        :desc="t('rooms.emptyHint')"
      />
    </div>

    <ul v-else class="rooms__list">
      <li v-for="session in rooms" :key="session.sessionId">
        <m3e-card variant="elevated" class="room-card">
          <div slot="content" class="room-card__body">
            <div class="room-card__main">
              <div class="room-card__code">{{ session.roomCode || t("rooms.noCode") }}</div>
              <div class="room-card__meta">
                <m3e-badge :class="session.phase === 'online' ? 'badge--ok' : 'badge--pending'">
                  {{ phaseLabel(session.phase) }}
                </m3e-badge>
                <m3e-badge class="badge--role">{{ roleLabel(session.role) }}</m3e-badge>
                <span class="room-card__fact">
                  {{ t("rooms.members") }} · {{ session.members.length }}
                </span>
                <span class="room-card__fact">{{ elapsed(session.createdAt) }}</span>
                <span v-if="session.port" class="room-card__fact mono">
                  :{{ session.port }}
                </span>
              </div>
            </div>

            <div class="room-card__actions">
              <m3e-button variant="tonal" @click="focus(session)">
                <m3e-icon slot="icon" name="open_in_new" />
                {{ t("rooms.focus") }}
              </m3e-button>
              <m3e-button variant="outlined" @click="disconnect(session)">
                <m3e-icon slot="icon" name="link_off" />
                {{ t("rooms.disconnect") }}
              </m3e-button>
            </div>
          </div>
        </m3e-card>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.rooms {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--sp-4) var(--sp-6) var(--sp-6);
}

.rooms__empty {
  height: 100%;
  display: grid;
  place-items: center;
}

.rooms__list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

.room-card {
  --m3e-elevated-card-container-color: var(--card-bg, var(--surface-container));
}

.room-card__body {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  flex-wrap: wrap;
  padding: var(--sp-4);
}

.room-card__main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.room-card__code {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  word-break: break-all;
}

.room-card__meta {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-wrap: wrap;
}

.room-card__fact {
  font-size: var(--fs-xs);
  color: var(--text-muted);
}

.badge--ok {
  --m3e-badge-container-color: var(--tertiary-container, #eaddff);
  --m3e-badge-color: var(--on-tertiary-container, #2a1a5e);
}
.badge--pending {
  --m3e-badge-container-color: var(--surface-variant);
  --m3e-badge-color: var(--text-muted);
}
.badge--role {
  --m3e-badge-container-color: var(--primary-container);
  --m3e-badge-color: var(--on-primary-container);
}

.room-card__actions {
  display: flex;
  gap: var(--sp-2);
  flex-wrap: wrap;
}

.mono {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
}
</style>
