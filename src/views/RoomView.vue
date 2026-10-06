<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { useConnect } from "../composables/useConnect";
import type { RoomMember } from "../lib/api/types";
import EmptyState from "../components/ui/EmptyState.vue";

/**
 * 房间视图：联机成功后的独立页面（外壳按路由 meta 收起左侧导航栏）。
 *
 * 两条数据来源，刻意分开：
 * - **房间信息**来自 `connect-event`（房间码 / 角色 / 延迟），是核心侧掌握的事实；
 * - **成员列表**来自适配器自报的 `connect_status`（陶瓦的 `profiles`）。
 *
 * 成员卡片**按数组实际长度渲染**，不写死 8 个格子、不造空位：目前适配器只报告本机
 * 一个档案，将来它报告更多就自动多出几张卡。
 */
const { t } = useI18n();
const router = useRouter();
const {
  mode,
  role,
  roomCode,
  status,
  selectedId,
  currentGameName,
  refreshStatus,
  stop,
} = useConnect();

const HOST_PHASE = /host/;
const STATE_OK = /-ok$/;

/** 适配器自报的成员（拿不到就是空数组）。 */
const players = computed<RoomMember[]>(() => status.value?.players ?? []);

/** 房间是否已就绪：适配器状态机走到 `*-ok`。 */
const ready = computed(() => STATE_OK.test(String(status.value?.phase ?? "")));

const roleLabel = computed(() =>
  role.value === "host" ? t("connect.host") : t("connect.guest"),
);

const phaseLabel = computed(() => {
  const phase = String(status.value?.phase ?? "");
  if (!phase) return t("room.phaseUnknown");
  return phase;
});

/** 成员卡片上的角色标记：优先用适配器给的 `kind`，否则靠本机/房主推断。 */
function memberRole(m: RoomMember): string {
  const kind = m.kind.toUpperCase();
  if (kind.includes("HOST")) return t("connect.host");
  if (kind.includes("GUEST")) return t("connect.guest");
  if (m.isSelf && role.value === "host") return t("connect.host");
  if (m.isSelf) return t("connect.guest");
  return HOST_PHASE.test(kind) ? t("connect.host") : t("connect.guest");
}

/** 名字拿不到时用设备号兜底，别让卡片空着。 */
function memberName(m: RoomMember): string {
  if (m.name) return m.name;
  return m.machineId ? `${m.machineId.slice(0, 8)}…` : t("room.unknownPlayer");
}

/** 设备号太长，只显示前 8 位。 */
function shortId(id: string): string {
  return id ? `${id.slice(0, 8)}…` : "";
}

// ---- 适配器状态轮询 ----
//
// 成员会陆续加入，状态得跟着变；房间视图停留时间可能很长，所以用轮询而不是只取一次。
// 与房主页的扫描轮询同理：在途去重，防止慢响应叠加。
const POLL_INTERVAL = 3000;
let timer: number | null = null;

onMounted(() => {
  void refreshStatus();
  timer = window.setInterval(() => void refreshStatus(), POLL_INTERVAL);
});

onUnmounted(() => {
  if (timer !== null) window.clearInterval(timer);
  timer = null;
});

// ---- 退出房间 ----
const leaving = ref(false);

/**
 * 退出房间。
 *
 * **不在这里跳转**：`stop()` 的 promise 只代表"命令已受理"，后端拆房间（停适配器、
 * 解除耦合器广播、推 `stopped`）还在进行，此刻跳走会让界面停在旧的连接状态。
 * 跳转交给 `App.vue` 的 `mode` 监听——它在收到真正的 `idle` 之后才回联机页。
 *
 * 按钮只负责"别让人连点"，并加一条兜底：若后端迟迟不回 `stopped`（例如适配器进程
 * 已崩溃），这里兜底回联机页，避免用户被困在房间视图里无路可走。
 */
const LEAVE_FALLBACK = 8000;

/**
 * 兜底定时器的句柄。
 *
 * 必须在卸载时清掉：用户点退出后可能立刻用其它方式离开房间视图（导航、快捷键），
 * 此时定时器仍会在 8 秒后触发 `router.push`，把已经切到别的页面的用户**强行拽回**
 * 联机页。
 */
let leaveTimer: number | null = null;

onUnmounted(() => {
  if (leaveTimer !== null) {
    window.clearTimeout(leaveTimer);
    leaveTimer = null;
  }
});

async function leave() {
  if (leaving.value) return;
  leaving.value = true;
  try {
    await stop();
  } catch {
    // 出错也要放人走：`useConnect` 已把错误弹成吐司，这里不重复处理。
  }
  leaveTimer = window.setTimeout(() => {
    leaveTimer = null;
    if (mode.value !== "idle") {
      console.warn("[联机] 未收到 stopped 事件，兜底退出房间视图");
      void router.push({ name: "connect" });
    }
    leaving.value = false;
  }, LEAVE_FALLBACK);
}
</script>

<template>
  <div class="room">
    <!-- 顶部：房间码与角色，联机中最重要的两个信息 -->
    <header class="room__head">
      <div class="room__title">
        <span class="room__code">{{ roomCode || t("room.noRoom") }}</span>
        <div class="room__badges">
          <m3e-badge size="large" class="badge--role">
            <m3e-icon slot="icon" :name="role === 'host' ? 'wifi_tethering' : 'group'" />
            {{ roleLabel }}
          </m3e-badge>
          <m3e-badge size="large" :class="ready ? 'badge--ok' : 'badge--pending'">
            {{ phaseLabel }}
          </m3e-badge>
        </div>
      </div>

      <div class="room__roominfo">
        <div class="room__field">
          <span class="room__field-label">{{ t("room.game") }}</span>
          <span class="room__field-value">{{ currentGameName }}</span>
        </div>
        <div class="room__field">
          <span class="room__field-label">{{ t("room.adapter") }}</span>
          <span class="room__field-value mono">{{ selectedId || "—" }}</span>
        </div>
        <div class="room__field">
          <span class="room__field-label">{{ t("room.localPort") }}</span>
          <span class="room__field-value mono">{{ status?.port ?? "—" }}</span>
        </div>
      </div>
    </header>

    <!-- 中部：成员网格。列数按实际人数选，不写死 4×2、不造空位。 -->
    <main class="room__players">
      <div v-if="players.length === 0" class="room__empty">
        <EmptyState
          icon="group"
          :title="t('room.noMembers')"
          :desc="t('room.noMembersHint')"
        />
      </div>
      <div v-else class="room__grid" :style="{ '--cols': Math.min(players.length, 4) }">
        <m3e-card
          v-for="(m, i) in players"
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

    <!-- 底部：退出 -->
    <footer class="room__foot">
      <m3e-button variant="filled" :disabled="leaving" @click="leave">
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
  padding: var(--sp-6);
}

/* ---------- 顶部：房间信息 ---------- */
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

/* 徽章只覆盖配色，形状交给组件 */
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

/* ---------- 中部：成员网格 ---------- */
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

/*
 * 列数由实际人数决定（--cols 在模板里按 players.length 算），最多 4 列。
 * 不写死 8 个格子：人少时不会留下大片空框，人多时自动折行。
 */
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
  /* 名字可能很长（可能是整条 MOTD），限制两行即可 */
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

/* ---------- 底部：退出 ---------- */
.room__foot {
  flex: none;
  display: flex;
  justify-content: center;
}
</style>
