<script setup lang="ts">
import { ref, computed } from "vue";
import type { P2PStage, StartP2PArgs } from "../../lib/api/connect";

const props = defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
  stage: P2PStage;
}>();

const emit = defineEmits<{
  start: [args: StartP2PArgs];
  stop: [];
}>();

const code = ref(localStorage.getItem("flash_code") || "");
const mode = ref<"create" | "join">(
  (localStorage.getItem("flash_mode") as "create" | "join") || "create"
);

const canStart = computed(() => code.value.trim().length > 0 && props.stage !== "connecting" && props.stage !== "connected");
const busy = computed(() => props.stage === "connecting" || props.stage === "connected");

function persistCode(v: string) {
  code.value = v;
  localStorage.setItem("flash_code", v);
}
function chooseMode(m: "create" | "join") {
  mode.value = m;
  localStorage.setItem("flash_mode", m);
}

function start() {
  if (!canStart.value) return;
  emit("start", { mode: mode.value, code: code.value.trim() });
}

function stop() {
  emit("stop");
}
</script>

<template>
  <div class="flash-root">
    <div class="hero-intro">
      <div class="hero-intro__title">P2P 直连联机</div>
      <div class="hero-intro__sub">基于折跃门协议的端到端穿透，无需中转</div>
    </div>

    <!-- 角色切换 -->
    <div class="mode-options">
      <button :class="['mode-btn', { 'is-active': mode === 'create' }]" @click="chooseMode('create')">
        <i class="bi bi-hdd-stack mode-btn__icon"></i>
        <span class="mode-btn__title">创建房间</span>
        <span class="mode-btn__desc">我已开局域网联机</span>
      </button>
      <button :class="['mode-btn', { 'is-active': mode === 'join' }]" @click="chooseMode('join')">
        <i class="bi bi-person-plus mode-btn__icon"></i>
        <span class="mode-btn__title">加入房间</span>
        <span class="mode-btn__desc">输入房主分享的邀请码</span>
      </button>
    </div>

    <!-- 邀请码输入 -->
    <div class="code-input">
      <el-input
        :model-value="code"
        @update:model-value="persistCode"
        class="code-field"
        :placeholder="mode === 'create' ? '输入邀请码（如 MC2026）' : '输入房主分享的邀请码'"
        clearable
        spellcheck="false"
        autocomplete="off"
        @keyup.enter="start"
      >
        <template #prefix><i class="bi bi-key"></i></template>
      </el-input>
    </div>

    <!-- 操作 -->
    <div class="actions">
      <el-button
        v-if="!busy"
        type="primary"
        size="large"
        class="action-btn"
        :disabled="!canStart"
        @click="start"
      >
        <i :class="['bi', mode === 'create' ? 'bi-play-fill' : 'bi-box-arrow-in-right']"></i>
        {{ mode === "create" ? "创建房间" : "加入房间" }}
      </el-button>
      <el-button v-else type="danger" size="large" class="action-btn" @click="stop">
        <i class="bi bi-stop-fill"></i> 停止联机
      </el-button>
    </div>

    <div class="tip">
      <i class="bi bi-info-circle"></i>
      <span>默认使用公共信令/STUN 服务器，高级模式可自定义。</span>
    </div>
  </div>
</template>

<style scoped>
.flash-root {
  width: 100%;
  max-width: 560px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
}

.hero-intro {
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.hero-intro__title {
  font-size: var(--fs-2xl);
  font-weight: var(--fw-bold);
  color: var(--text-primary);
  background: linear-gradient(120deg, var(--accent-primary), #5b8bff);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}
.hero-intro__sub {
  font-size: var(--fs-sm);
  color: var(--text-muted);
}

.mode-options {
  display: flex;
  gap: var(--sp-3);
}
.mode-btn {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-1);
  padding: var(--sp-4);
  border-radius: var(--r-lg);
  border: 1px solid var(--border-color);
  background: var(--bg-soft);
  cursor: pointer;
  transition: background var(--motion-base) var(--ease),
              border-color var(--motion-base) var(--ease),
              transform var(--motion-base) var(--ease);
  color: var(--text-secondary);
  text-align: center;
}
.mode-btn:hover {
  border-color: var(--border-hover);
  background: var(--bg-soft-hover);
  transform: translateY(-1px);
}
.mode-btn.is-active {
  border-color: var(--accent-primary);
  background: var(--status-info-bg);
  color: var(--text-primary);
}
.mode-btn__icon {
  font-size: var(--fs-2xl);
  color: var(--accent-primary);
}
.mode-btn__title {
  font-size: var(--fs-md);
  font-weight: var(--fw-semibold);
}
.mode-btn__desc {
  font-size: var(--fs-xs);
  color: var(--text-muted);
}

.code-input {
  display: flex;
}
.code-field.is-active :deep(.el-input__wrapper) {
  box-shadow: 0 0 0 2px var(--shadow-accent), 0 0 0 1px var(--accent-primary) inset;
}

.actions {
  display: flex;
}
.action-btn {
  width: 100%;
  transition: transform var(--motion-base) var(--ease);
}
.action-btn:hover { transform: translateY(-1px); }

.tip {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  font-size: var(--fs-xs);
  color: var(--text-muted);
}
.tip i { color: var(--accent-primary); }
</style>
