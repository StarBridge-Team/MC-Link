<script setup lang="ts">
/**
 * 单个房间的内容视图。
 *
 * 数据只来自 `useRoomsStore`（Rust 快照的投影），**不自己发命令查状态**——
 * 状态推进由 Rust 负责，本组件只渲染 + 低频触发一次 `sync` 让适配器报成员。
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import EmptyState from "../components/ui/EmptyState.vue";
import { showError, showSuccess } from "../composables/useToast";
import { writeClipboardText } from "../lib/api/app";
import { buildShareLink } from "../lib/shareLink";
import { useRoomsStore } from "../stores/rooms";
import type { RoomMember, RoomPhase } from "../ipc/contracts/room";

const props = defineProps<{ sessionId: string }>();

const { t } = useI18n();
const store = useRoomsStore();

const session = computed(() => store.get(props.sessionId));
const progress = computed(() => store.progressOf(props.sessionId));
const members = computed<RoomMember[]>(() => session.value?.members ?? []);
const role = computed(() => session.value?.role ?? "guest");
const phase = computed<RoomPhase>(() => session.value?.phase ?? "starting");
const roomCode = computed(() => session.value?.roomCode ?? "");
const leaving = ref(false);

const PHASE_LABEL: Record<RoomPhase, string> = {
  starting: "room.phase.starting",
  linking: "room.phase.linking",
  online: "room.phase.online",
  stopping: "room.phase.stopping",
  closed: "room.phase.closed",
};

const phaseLabel = computed(() => t(PHASE_LABEL[phase.value]));
const ready = computed(() => phase.value === "online");
const active = computed(() => phase.value !== "closed");

/** 成员卡片上的角色标记：优先用适配器给的 `kind`，否则按本机/房主推断。 */
function memberRole(m: RoomMember): string {
  const kind = m.kind.toUpperCase();
  if (kind.includes("HOST")) return t("connect.host");
  if (kind.includes("GUEST")) return t("connect.guest");
  if (m.isSelf && role.value === "host") return t("connect.host");
  if (m.isSelf) return t("connect.guest");
  return t("connect.guest");
}

function memberName(m: RoomMember): string {
  if (m.name) return m.name;
  return m.machineId ? `${m.machineId.slice(0, 8)}…` : t("room.unknownPlayer");
}

function shortId(id: string): string {
  return id ? `${id.slice(0, 8)}…` : "";
}

// ---- 成员同步 ----
// 适配器只在被问到时才报成员，因此低频轮询一次。Rust 侧只在真的变化时才推事件。
const POLL_INTERVAL = 3000;
let timer: number | null = null;

onMounted(() => {
  timer = window.setInterval(() => {
    if (!active.value) return;
    void store.sync(props.sessionId).catch(() => undefined);
  }, POLL_INTERVAL);
});

onUnmounted(() => {
  if (timer !== null) window.clearInterval(timer);
  timer = null;
});

// ---- 操作 ----
async function copyCode() {
  if (!roomCode.value) return;
  try {
    await writeClipboardText(roomCode.value);
    showSuccess(t("connect.copied"));
  } catch {
    /* 剪贴板不可用时忽略 */
  }
}

async function share() {
  if (!roomCode.value) return;
  try {
    await writeClipboardText(buildShareLink(roomCode.value));
    showSuccess(t("connect.linkCopied"));
  } catch {
    /* 忽略 */
  }
}

async function leave() {
  if (leaving.value) return;
  leaving.value = true;
  try {
    await store.leave(props.sessionId);
  } catch (e) {
    showError(e instanceof Error ? e.message : String(e));
  } finally {
    leaving.value = false;
  }
}
</script>

<template>
  <div class="room">
    <header class="room__head">
      <div class="room__title">
        <div class="room__code-row">
          <span class="room__code">{{ roomCode || t("room.noRoom") }}</span>
          <m3e-button variant="text" :disabled="!roomCode" @click="copyCode">
            <m3e-icon slot="icon" name="content_copy" />
          </m3e-button>
        </div>
        <div class="room__badges">
          <m3e-badge size="large" class="badge--role">
            <m3e-icon slot="icon" :name="role === 'host' ? 'wifi_tethering' : 'group'" />
            {{ role === "host" ? t("connect.host") : t("connect.guest") }}
          </m3e-badge>
          <m3e-badge size="large" :class="ready ? 'badge--ok' : 'badge--pending'">
            {{ phaseLabel }}
          </m3e-badge>
        </div>
      </div>

      <!-- 过程提示：来自 `mclink://room/progress`，可丢、只做临时文案 -->
      <p v-if="progress && !ready" class="room__progress">{{ progress.message }}</p>

      <div class="room__roominfo">
        <div class="room__field">
          <span class="room__field-label">{{ t("room.adapter") }}</span>
          <span class="room__field-value mono">{{ session?.adapterId || "—" }}</span>
        </div>
        <div class="room__field">
          <span class="room__field-label">{{ t("room.localPort") }}</span>
          <span class="room__field-value mono">{{ session?.port ?? "—" }}</span>
        </div>
        <div class="room__field">
          <span class="room__field-label">{{ t("room.members") }}</span>
          <span class="room__field-value">{{ members.length }}</span>
        </div>
      </div>
    </header>

    <main class="room__players">
      <div v-if="members.length === 0" class="room__empty">
        <EmptyState
          icon="group"
          :title="t('room.noMembers')"
          :desc="t('room.noMembersHint')"
        />
      </div>
      <div v-else class="room__grid" :style="{ '--cols': Math.min(members.length, 4) }">
        <m3e-card
          v-for="(m, i) in members"
          :key="m.machineId || i"
          variant="elevated"
          class="player"
          :class="{ 'player--self': m.isSelf }"
        >
          <div slot="content" class="player__body">
            <div class="player__avatar">
              <m3e-icon :name="m.isSelf ? 'person' : 'person_outline'" />
            </div>
            <div class="player__name">{{ memberName(m) }}</div>
            <div class="player__meta">
              <m3e-badge :class="m.isSelf ? 'badge--self' : 'badge--peer'">
                {{ memberRole(m) }}
              </m3e-badge>
              <span v-if="m.isSelf" class="player__tag">{{ t("room.you") }}</span>
            </div>
            <div v-if="m.machineId" class="player__id mono">{{ shortId(m.machineId) }}</div>
          </div>
        </m3e-card>
      </div>
    </main>

    <footer class="room__foot">
      <m3e-button variant="tonal" :disabled="!roomCode" @click="share">
        <m3e-icon slot="icon" name="share" />
        {{ t("room.copyInvite") }}
      </m3e-button>
      <m3e-button variant="filled" :disabled="leaving || !active" @click="leave">
        <m3e-icon slot="icon" name="logout" />
        {{ t("room.leave") }}
      </m3e-button>
    </footer>
  </div>
</template>

<style scoped>
.room {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
  padding: var(--sp-4) var(--sp-6) var(--sp-6);
}

.room__head {
  flex: none;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-3);
  text-align: center;
}

.room__title {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-2);
}

.room__code-row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}

.room__code {
  font-size: 32px;
  font-weight: var(--fw-bold);
  letter-spacing: 2px;
  color: var(--text-primary);
  word-break: break-all;
  line-height: var(--lh-tight);
}

.room__badges {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-wrap: wrap;
  justify-content: center;
}

.badge--role {
  --m3e-badge-container-color: var(--primary-container);
  --m3e-badge-color: var(--on-primary-container);
}
.badge--ok {
  --m3e-badge-container-color: var(--tertiary-container, #eaddff);
  --m3e-badge-color: var(--on-tertiary-container, #2a1a5e);
}
.badge--pending {
  --m3e-badge-container-color: var(--surface-variant);
  --m3e-badge-color: var(--text-muted);
}
.badge--self {
  --m3e-badge-container-color: var(--primary-container);
  --m3e-badge-color: var(--on-primary-container);
}
.badge--peer {
  --m3e-badge-container-color: var(--secondary-container);
  --m3e-badge-color: var(--on-secondary-container);
}

.room__progress {
  margin: 0;
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}

.room__roominfo {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: var(--sp-5);
}

.room__field {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.room__field-label {
  font-size: var(--fs-xs);
  color: var(--text-muted);
}
.room__field-value {
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}

.room__players {
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow-y: auto;
}

.room__empty {
  width: 100%;
}

.room__grid {
  display: grid;
  grid-template-columns: repeat(var(--cols, 1), minmax(0, 160px));
  gap: var(--sp-4);
  justify-content: center;
  align-content: center;
  width: 100%;
  padding: var(--sp-2);
}

.player {
  --m3e-elevated-card-container-color: var(--card-bg, var(--surface-container));
}
.player--self {
  outline: 2px solid var(--primary);
  outline-offset: -2px;
  border-radius: var(--m3e-card-shape, 12px);
}

.player__body {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-4) var(--sp-3);
  text-align: center;
}

.player__avatar {
  display: grid;
  place-items: center;
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: var(--surface-container-high);
  color: var(--text-secondary);
}
.player__avatar m3e-icon {
  font-size: 28px;
}

.player__name {
  font-size: var(--fs-md);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  word-break: break-word;
}

.player__meta {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-wrap: wrap;
  justify-content: center;
}
.player__tag {
  font-size: var(--fs-xs);
  color: var(--text-muted);
}

.player__id {
  font-size: var(--fs-xs);
  color: var(--text-muted);
  opacity: 0.75;
}

.mono {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
}

.room__foot {
  flex: none;
  display: flex;
  justify-content: center;
  gap: var(--sp-3);
}
</style>
