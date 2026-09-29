<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import type { P2PStage, P2PStatus } from "../../lib/api/connect";

const props = defineProps<{
  status: P2PStatus | null;
  logs: Array<{ text: string; level: "info" | "success" | "warn" | "error" }>;
}>();

const emit = defineEmits<{ stop: [] }>();

const stage = computed<P2PStage>(() => props.status?.stage ?? "idle");

const hero = computed(() => {
  switch (stage.value) {
    case "connecting":
      return { label: "连接中", tone: "connecting", icon: "bi-arrow-repeat" };
    case "connected":
      return { label: "已连接", tone: "connected", icon: "bi-check-circle-fill" };
    case "disconnected":
      return {
        label: props.status?.lastError ? "连接失败" : "已断开",
        tone: "failed",
        icon: "bi-exclamation-circle-fill",
      };
    default:
      return { label: "未连接", tone: "idle", icon: "bi-wifi" };
  }
});

const metrics = computed(() => {
  const s = props.status;
  if (!s) return [];
  const items: Array<{ label: string; value: string }> = [];
  if (s.mode) items.push({ label: "角色", value: s.mode === "create" ? "创建者" : "加入者" });
  if (s.code) items.push({ label: "邀请码", value: s.code });
  if (s.peerAddr) items.push({ label: "对端地址", value: s.peerAddr });
  if (s.localNat || s.peerNat)
    items.push({ label: "NAT", value: `${s.localNat ?? "?"} / ${s.peerNat ?? "?"}` });
  if (s.successLayer) items.push({ label: "成功层", value: s.successLayer });
  if (s.elapsedMs != null) items.push({ label: "耗时", value: `${s.elapsedMs} ms` });
  return items;
});

const scrollRef = ref<HTMLElement | null>(null);
watch(
  () => props.logs.length,
  async () => {
    await nextTick();
    const el = scrollRef.value;
    if (el) el.scrollTop = el.scrollHeight;
  }
);

function copyError() {
  if (props.status?.lastError) navigator.clipboard.writeText(props.status.lastError);
}
</script>

<template>
  <div class="cs-root">
    <!-- 状态英雄区 -->
    <div :class="['hero', `hero--${hero.tone}`]">
      <div class="hero__glow"></div>
      <div class="hero__main">
        <div class="hero__icon"><i :class="['bi', hero.icon]"></i></div>
        <div class="hero__text">
          <div class="hero__label">{{ hero.label }}</div>
          <div class="hero__sub">
            <span v-if="status?.mode">{{ status.mode === "create" ? "创建者" : "加入者" }}</span>
            <span v-if="status?.code" class="hero__code">{{ status.code }}</span>
            <span v-else-if="stage === 'idle'" class="hero__hint">输入邀请码开始 P2P 联机</span>
          </div>
        </div>
      </div>
      <button
        v-if="stage === 'connecting' || stage === 'connected'"
        class="hero__stop"
        @click="emit('stop')"
      >
        <i class="bi bi-stop-fill"></i> 停止
      </button>
    </div>

    <!-- 指标卡 -->
    <div v-if="metrics.length" class="metrics">
      <div v-for="m in metrics" :key="m.label" class="metric">
        <div class="metric__label">{{ m.label }}</div>
        <div class="metric__value">{{ m.value }}</div>
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="status?.lastError && stage === 'disconnected'" class="error-bar" @click="copyError">
      <i class="bi bi-exclamation-triangle-fill"></i>
      <span class="error-bar__text">{{ status.lastError }}</span>
      <span class="error-bar__copy"><i class="bi bi-clipboard"></i> 复制</span>
    </div>

    <!-- 日志时间线 -->
    <div class="log">
      <div class="log__head">
        <i class="bi bi-terminal"></i>
        <span>连接日志</span>
        <span class="log__count">{{ logs.length }} 条</span>
      </div>
      <div ref="scrollRef" class="log__body">
        <div v-if="!logs.length" class="log__empty">等待连接日志…</div>
        <div
          v-for="(line, i) in logs"
          :key="i"
          :class="['log__line', `log__line--${line.level}`]"
        >
          <span class="log__dot"></span>
          <span class="log__text">{{ line.text }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cs-root {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  min-height: 0;
}

/* 英雄区 */
.hero {
  position: relative;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-4);
  padding: var(--sp-5) var(--sp-6);
  border-radius: var(--r-lg);
  border: 1px solid var(--border-color);
  background: linear-gradient(135deg, rgba(47, 107, 255, 0.08), rgba(91, 139, 255, 0.02));
  backdrop-filter: blur(8px);
}
.hero__glow {
  position: absolute;
  inset: -40% -20% auto auto;
  width: 60%;
  height: 180%;
  background: radial-gradient(circle at 70% 30%, rgba(47, 107, 255, 0.25), transparent 60%);
  filter: blur(20px);
  pointer-events: none;
}
.hero__main {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  position: relative;
}
.hero__icon {
  width: 44px;
  height: 44px;
  border-radius: var(--r-full);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--fs-xl);
  flex-shrink: 0;
  background: rgba(47, 107, 255, 0.12);
  color: var(--accent-primary);
}
.hero--connecting .hero__icon i { animation: hero-pulse 1.1s var(--ease) infinite; }
.hero--connected .hero__icon {
  background: rgba(46, 204, 113, 0.16);
  color: var(--status-success);
}
.hero--failed .hero__icon {
  background: rgba(255, 90, 95, 0.16);
  color: var(--status-danger);
}
.hero--idle .hero__icon {
  background: rgba(154, 163, 175, 0.16);
  color: var(--text-muted);
}
@keyframes hero-pulse {
  0%, 100% { transform: scale(1); opacity: 1; }
  50% { transform: scale(0.85); opacity: 0.7; }
}
.hero__text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.hero__label {
  font-size: var(--fs-xl);
  font-weight: var(--fw-bold);
  color: var(--text-primary);
  line-height: 1.2;
}
.hero__sub {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}
.hero__code {
  font-family: var(--font-mono);
  color: var(--accent-primary);
  font-weight: var(--fw-semibold);
}
.hero__hint { color: var(--text-muted); }
.hero__stop {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-4);
  border-radius: var(--r-md);
  border: 1px solid var(--status-danger);
  background: rgba(255, 90, 95, 0.08);
  color: var(--status-danger);
  font-size: var(--fs-sm);
  cursor: pointer;
  transition: background var(--motion-base) var(--ease), transform var(--motion-base) var(--ease);
}
.hero__stop:hover { background: rgba(255, 90, 95, 0.16); transform: translateY(-1px); }

/* 指标卡 */
.metrics {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: var(--sp-3);
}
.metric {
  padding: var(--sp-3) var(--sp-4);
  border-radius: var(--r-md);
  border: 1px solid var(--border-color);
  background: var(--bg-card);
}
.metric__label {
  font-size: var(--fs-xs);
  color: var(--text-muted);
  margin-bottom: 2px;
}
.metric__value {
  font-size: var(--fs-sm);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  word-break: break-all;
  font-family: var(--font-mono);
}

/* 错误条 */
.error-bar {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  border-radius: var(--r-md);
  border: 1px solid rgba(255, 90, 95, 0.4);
  background: rgba(255, 90, 95, 0.08);
  color: var(--status-danger);
  font-size: var(--fs-sm);
  cursor: pointer;
}
.error-bar i { flex-shrink: 0; }
.error-bar__text { flex: 1; min-width: 0; word-break: break-all; }
.error-bar__copy { flex-shrink: 0; color: var(--text-muted); }

/* 日志 */
.log {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-color);
  border-radius: var(--r-lg);
  background: var(--bg-card);
  overflow: hidden;
  min-height: 160px;
  flex: 1;
}
.log__head {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-3) var(--sp-4);
  font-size: var(--fs-sm);
  font-weight: var(--fw-semibold);
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-soft);
}
.log__count { margin-left: auto; color: var(--text-muted); font-weight: var(--fw-regular); }
.log__body {
  flex: 1;
  padding: var(--sp-3) var(--sp-4);
  overflow-y: auto;
  font-family: var(--font-mono);
  font-size: var(--fs-xs);
  line-height: 1.7;
  max-height: 240px;
}
.log__empty { color: var(--text-muted); text-align: center; padding: var(--sp-5) 0; }
.log__line {
  display: flex;
  align-items: flex-start;
  gap: var(--sp-2);
  color: var(--text-secondary);
  animation: log-in var(--motion-base) var(--ease);
}
.log__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-muted);
  margin-top: 7px;
  flex-shrink: 0;
}
.log__line--success { color: var(--status-success); }
.log__line--success .log__dot { background: var(--status-success); }
.log__line--warn { color: #f5a623; }
.log__line--warn .log__dot { background: #f5a623; }
.log__line--error { color: var(--status-danger); }
.log__line--error .log__dot { background: var(--status-danger); }
.log__text { white-space: pre-wrap; word-break: break-all; }
@keyframes log-in {
  from { opacity: 0; transform: translateX(-4px); }
  to { opacity: 1; transform: translateX(0); }
}
</style>
