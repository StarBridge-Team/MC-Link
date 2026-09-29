<script setup lang="ts">
import { ref, computed, watch } from "vue";
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

const PKEY = "gaoji_p2p_";
const code = ref(localStorage.getItem(PKEY + "code") || "");
const mode = ref<"create" | "join">(
  (localStorage.getItem(PKEY + "mode") as "create" | "join") || "create"
);
const signalingAddr = ref(localStorage.getItem(PKEY + "signaling") || "");
const stunAddr = ref(localStorage.getItem(PKEY + "stun") || "");
const natStunServers = ref(localStorage.getItem(PKEY + "nat_stun") || "");
const appType = ref(localStorage.getItem(PKEY + "app_type") || "GameTcp");

const appTypeOptions = [
  { value: "GameTcp", label: "Minecraft Java (TCP)" },
  { value: "GameUdp", label: "Minecraft 基岩版 (UDP)" },
  { value: "RemoteDesktop", label: "远程桌面 (RDP/VNC)" },
  { value: "FileTransfer", label: "文件传输" },
  { value: "VoIP", label: "语音通话" },
  { value: "Stream", label: "流媒体推流" },
  { value: "CustomTcp", label: "自定义 TCP" },
  { value: "CustomUdp", label: "自定义 UDP" },
  { value: "Chat", label: "P2P 聊天" },
];

watch(code, (v) => localStorage.setItem(PKEY + "code", v));
watch(signalingAddr, (v) => localStorage.setItem(PKEY + "signaling", v));
watch(stunAddr, (v) => localStorage.setItem(PKEY + "stun", v));
watch(natStunServers, (v) => localStorage.setItem(PKEY + "nat_stun", v));
watch(appType, (v) => localStorage.setItem(PKEY + "app_type", v));
watch(mode, (v) => localStorage.setItem(PKEY + "mode", v));

const busy = computed(() => props.stage === "connecting" || props.stage === "connected");
const canStart = computed(() => code.value.trim().length > 0 && !busy.value);

function triggerStart() {
  if (!canStart.value) {
    if (!code.value.trim()) props.showToast("请输入邀请码");
    return;
  }
  const natList = natStunServers.value
    .split(/[,，\s]+/)
    .map((s) => s.trim())
    .filter(Boolean);
  emit("start", {
    mode: mode.value,
    code: code.value.trim(),
    signalingAddr: signalingAddr.value.trim() || undefined,
    stunAddr: stunAddr.value.trim() || undefined,
    natStunServers: natList.length ? natList : undefined,
    appType: appType.value,
  });
}

function stop() {
  emit("stop");
}

defineExpose({ triggerStart });
</script>

<template>
  <div class="gaoji-root">
    <div class="form-grid">
      <!-- 左：角色与邀请码 -->
      <div class="form-left">
        <div class="section-title">连接配置</div>
        <div class="mode-options">
          <button :class="['mode-btn', { 'is-active': mode === 'create' }]" @click="mode = 'create'">
            <i class="bi bi-hdd-stack mode-btn__icon"></i>
            <span class="mode-btn__title">创建</span>
          </button>
          <button :class="['mode-btn', { 'is-active': mode === 'join' }]" @click="mode = 'join'">
            <i class="bi bi-person-plus mode-btn__icon"></i>
            <span class="mode-btn__title">加入</span>
          </button>
        </div>

        <div class="field">
          <label class="field__label">邀请码 <span class="req">*</span></label>
          <el-input v-model="code" placeholder="如 MC2026" clearable @keyup.enter="triggerStart" />
        </div>

        <div class="field">
          <label class="field__label">应用类型</label>
          <el-select v-model="appType" class="field__select">
            <el-option v-for="o in appTypeOptions" :key="o.value" :label="o.label" :value="o.value" />
          </el-select>
        </div>
      </div>

      <!-- 右：高级服务器 -->
      <div class="form-right">
        <div class="section-title">服务器参数<span class="section-title__hint">（留空使用默认）</span></div>
        <div class="field">
          <label class="field__label">信令服务器</label>
          <el-input v-model="signalingAddr" placeholder="host:port" />
        </div>
        <div class="field">
          <label class="field__label">打洞 STUN</label>
          <el-input v-model="stunAddr" placeholder="host:port" />
        </div>
        <div class="field">
          <label class="field__label">NAT 检测 STUN</label>
          <el-input
            v-model="natStunServers"
            type="textarea"
            :rows="2"
            placeholder="逗号分隔，如 stun1:3478, stun2:3478"
          />
        </div>
      </div>
    </div>

    <div class="actions">
      <el-button v-if="!busy" type="primary" size="large" class="action-btn" :disabled="!canStart" @click="triggerStart">
        <i class="bi bi-play-fill"></i> 开始联机
      </el-button>
      <el-button v-else type="danger" size="large" class="action-btn" @click="stop">
        <i class="bi bi-stop-fill"></i> 停止联机
      </el-button>
    </div>
  </div>
</template>

<style scoped>
.gaoji-root {
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
}

.form-grid {
  display: flex;
  gap: var(--sp-6);
  flex-wrap: wrap;
}
.form-left,
.form-right {
  flex: 1;
  min-width: 280px;
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.section-title {
  font-size: var(--fs-sm);
  font-weight: var(--fw-semibold);
  color: var(--text-secondary);
}
.section-title__hint {
  color: var(--text-muted);
  font-weight: var(--fw-regular);
  margin-left: var(--sp-2);
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
  padding: var(--sp-3) var(--sp-2);
  border-radius: var(--r-md);
  border: 1px solid var(--border-color);
  background: var(--bg-soft);
  cursor: pointer;
  transition: all var(--motion-base) var(--ease);
  color: var(--text-secondary);
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
  font-size: var(--fs-xl);
  color: var(--accent-primary);
}
.mode-btn__title {
  font-size: var(--fs-sm);
  font-weight: var(--fw-semibold);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
.field__label {
  font-size: var(--fs-xs);
  font-weight: var(--fw-semibold);
  color: var(--text-muted);
}
.req { color: var(--status-danger); }
.field__select { width: 100%; }

.actions {
  display: flex;
}
.action-btn {
  width: 100%;
  transition: transform var(--motion-base) var(--ease);
}
.action-btn:hover { transform: translateY(-1px); }
</style>
