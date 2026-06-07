<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ElevatorText from "../ElevatorText.vue";
import TeamDialog from "../TeamDialog.vue";

const props = defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
}>();

const emit = defineEmits<{
  runningChange: [value: boolean];
}>();

const currentMode = ref<"none" | "running">("none");
const logs = ref<string[]>([]);
let unlistenLog: UnlistenFn | null = null;
let unlistenLatency: UnlistenFn | null = null;
const latencyMs = ref<number>(0);
const PERSIST_KEY = "gaoji_";
const roomName = ref(localStorage.getItem(PERSIST_KEY + "room_name") || "");
const roomPassword = ref(localStorage.getItem(PERSIST_KEY + "room_password") || "");
const roomCode = ref(localStorage.getItem(PERSIST_KEY + "room_code") || "");
const isConnecting = ref(false);
const selectedAdapters = ref<string[]>(JSON.parse(localStorage.getItem(PERSIST_KEY + "adapters") || "[]"));
const selectedMode = ref<"host" | "member">((localStorage.getItem(PERSIST_KEY + "mode") as "host" | "member") || "host");
const terracottaResult = ref<{ room?: string; url?: string } | null>(null);
const mcLinkVersion = ref("");
const terracottaInstalled = ref(false);
const showLogs = ref(true);
const showTeamDialog = ref(false);
const teamMembers = ref<{ name: string; role: string }[]>([]);

const availableAdapters = computed(() => [
  { id: "mc-link", name: "MC Link", icon: "bi-link-45deg", version: mcLinkVersion.value || "v0.2.16", status: "ready" as const },
  { id: "taoli", name: "陶瓦联机 (Terracotta)", icon: "bi-cpu", version: "v1.0.0", status: terracottaInstalled.value ? "ready" : "preparing" },
]);

const useTerracotta = computed(() => selectedAdapters.value.includes("taoli"));

const adapterLabels = computed(() => {
  return selectedAdapters.value.map(id => {
    const a = availableAdapters.value.find(a => a.id === id);
    return a ? a.name : id;
  });
});

const modeLabel = computed(() => selectedMode.value === "host" ? "房主" : "成员");
const isTeamRoom = computed(() => selectedAdapters.value.includes("mc-link") && currentMode.value === "running");

const statusText = computed(() => {
  if (isConnecting.value) return "连接中...";
  if (currentMode.value === "running") return "已连接";
  return "未连接";
});

const statusClass = computed(() => {
  if (isConnecting.value) return "status-connecting";
  if (currentMode.value === "running") return "status-connected";
  return "status-idle";
});

watch(selectedMode, (mode) => {
  if (mode === "member" && selectedAdapters.value.length > 1) {
    selectedAdapters.value = [selectedAdapters.value[0]];
  }
  localStorage.setItem(PERSIST_KEY + "mode", mode);
});

watch(selectedAdapters, (val) => {
  localStorage.setItem(PERSIST_KEY + "adapters", JSON.stringify(val));
}, { deep: true });

watch(roomName, (val) => localStorage.setItem(PERSIST_KEY + "room_name", val));
watch(roomPassword, (val) => localStorage.setItem(PERSIST_KEY + "room_password", val));
watch(roomCode, (val) => localStorage.setItem(PERSIST_KEY + "room_code", val));

onUnmounted(() => {
  if (unlistenLog) unlistenLog();
  if (unlistenLatency) unlistenLatency();
});

onMounted(async () => {
  unlistenLog = await listen<string>("app-log", (event) => {
    logs.value.push(event.payload);
    if (logs.value.length > 200) logs.value.shift();
  });
  unlistenLatency = await listen<number>("latency-update", (event) => {
    latencyMs.value = event.payload;
  });
  try {
    mcLinkVersion.value = await invoke<string>("get_app_version");
  } catch {
    mcLinkVersion.value = "v0.2.16";
  }
  try {
    const status: any = await invoke("get_adapter_status");
    terracottaInstalled.value = status.running;
  } catch {
    terracottaInstalled.value = false;
  }
});

function toggleAdapter(id: string) {
  if (selectedMode.value === "member") {
    if (selectedAdapters.value.includes(id)) {
      selectedAdapters.value = [];
    } else {
      selectedAdapters.value = [id];
    }
  } else {
    const idx = selectedAdapters.value.indexOf(id);
    if (idx === -1) {
      selectedAdapters.value.push(id);
    } else {
      selectedAdapters.value.splice(idx, 1);
    }
  }
}

async function confirmStart() {
  if (currentMode.value === "running" || isConnecting.value) {
    props.showToast("联机功能已在运行中");
    return;
  }

  if (selectedAdapters.value.length === 0) {
    props.showToast("请至少选择一个适配器");
    return;
  }

  const useMcLink = selectedAdapters.value.includes("mc-link");
  if (useMcLink && (!roomName.value || !roomPassword.value)) {
    props.showToast("请填写房间名和密码");
    return;
  }

  const hasRoomCode = useTerracotta.value;
  if (hasRoomCode && roomCode.value && !/^U\/[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}$/.test(roomCode.value)) {
    props.showToast("房间码格式错误，应为 U/XXXX-XXXX-XXXX-XXXX");
    return;
  }

  isConnecting.value = true;
  logs.value = [];

  try {
    if (useTerracotta.value) {
      if (selectedMode.value === "host") {
        const result: any = await invoke("start_terracotta_host", { roomCode: roomCode.value, playerName: props.playerName });
        terracottaResult.value = { room: result.room, url: result.url };
        if (result.state === "host-ok") {
          currentMode.value = "running";
          emit("runningChange", true);
          props.showToast(`房间已创建! 房间码: ${result.room || "未知"}`);
        } else if (result.state === "exception") {
          props.showToast(`陶瓦联机异常 (代码: ${result.type || "未知"})`);
        }
      } else {
        const result: any = await invoke("start_terracotta_guest", { roomCode: roomCode.value, playerName: props.playerName });
        terracottaResult.value = { room: result.room, url: result.url };
        if (result.state === "guest-ok") {
          currentMode.value = "running";
          emit("runningChange", true);
          props.showToast(`已连接! 本地地址: ${result.url || "未知"}`);
        } else if (result.state === "exception") {
          props.showToast(`陶瓦联机异常 (代码: ${result.type || "未知"})`);
        }
      }
    } else {
      const selectedRelay = localStorage.getItem("relay_selected") || "__auto__";
      const result = await invoke("start_online", {
        roomName: roomName.value,
        password: roomPassword.value,
        selectedRelay,
        adapters: selectedAdapters.value,
        isHost: selectedMode.value === "host",
        playerName: props.playerName,
      });
      currentMode.value = "running";
      emit("runningChange", true);
      props.showToast(result as string);
    }
  } catch (e: any) {
    props.showToast(e.toString());
    currentMode.value = "none";
    emit("runningChange", false);
  } finally {
    isConnecting.value = false;
  }
}

async function stopOnline() {
  try {
    await invoke("stop_online");
    props.showToast("联机已停止");
  } catch (e: any) {
    props.showToast("停止失败: " + e);
  } finally {
    currentMode.value = "none";
    emit("runningChange", false);
    isConnecting.value = false;
    logs.value = [];
    terracottaResult.value = null;
  }
}

async function openTeamDialog() {
  try {
    const players: any = await invoke("get_players", { roomName: roomName.value });
    teamMembers.value = (players || []).map((p: any) => ({
      name: p.name,
      role: p.role === "host" ? "队长" : "成员",
    }));
  } catch {
    teamMembers.value = [];
  }
  showTeamDialog.value = true;
}

function autoScroll(el: Event) {
  const target = el.target as HTMLElement;
  target.scrollTop = target.scrollHeight;
}
// 被 defineExpose 导出供父组件调用
void confirmStart;
defineExpose({ confirmStart, isConnecting });
</script>

<template>
  <div class="top-bar">
    <div class="top-bar-left">
      <div class="top-bar-mode">
        <ElevatorText :text="modeLabel" />模式
      </div>
      <template v-if="currentMode === 'running'">
        <span class="top-bar-sep">|</span>
        <span class="top-bar-room">{{ roomCode || roomName || '已连接' }}</span>
      </template>
      <template v-else-if="selectedAdapters.length">
        <span class="top-bar-sep">|</span>
        <span class="top-bar-room">{{ adapterLabels.join(', ') }}</span>
      </template>
    </div>
    <div class="top-bar-right">
      <span v-if="currentMode === 'running' && latencyMs > 0" class="latency-badge">{{ latencyMs }}ms</span>
      <span :class="['status-badge', statusClass]">{{ statusText }}</span>
    </div>
  </div>

  <div class="content-area">
    <div v-if="currentMode === 'none'" class="form-layout">
      <div class="form-left">
        <div class="mode-options">
          <button
            :class="['mode-btn', { active: selectedMode === 'host' }]"
            @click="selectedMode = 'host'"
          >
            <i class="bi bi-hdd-stack"></i>
            <span class="mode-btn-title">房主</span>
            <span class="mode-btn-desc">我已开启局域网联机</span>
          </button>
          <button
            :class="['mode-btn', { active: selectedMode === 'member' }]"
            @click="selectedMode = 'member'"
          >
            <i class="bi bi-person-plus"></i>
            <span class="mode-btn-title">成员</span>
            <span class="mode-btn-desc">加入他人的房间</span>
          </button>
        </div>

        <template v-if="selectedAdapters.length === 0">
          <div class="no-adapter-hint">请在右边选择一个适配器</div>
        </template>

        <template v-if="selectedAdapters.includes('mc-link')">
          <div class="input-group">
            <label>房间名</label>
            <input type="text" v-model="roomName" placeholder="输入房间名" class="form-input" />
          </div>
          <div class="input-group">
            <label>密码</label>
            <input type="password" v-model="roomPassword" placeholder="输入密码" class="form-input" />
          </div>
        </template>

        <div v-if="useTerracotta && selectedMode === 'member'" class="room-code-group">
          <label class="section-label">房间码</label>
          <input
            type="text"
            v-model="roomCode"
            placeholder="U/XXXX-XXXX-XXXX-XXXX"
            class="code-input"
          />
        </div>
        <div v-if="useTerracotta && selectedMode === 'host'" class="room-code-group">
          <label class="section-label">房间码 <span class="opt-label">（可选，留空自动生成）</span></label>
          <input
            type="text"
            v-model="roomCode"
            placeholder="U/XXXX-XXXX-XXXX-XXXX"
            class="code-input"
          />
        </div>

      </div>

      <div class="form-right">
        <label class="section-label">选择适配器</label>
        <div class="adapter-list">
          <div
            v-for="adapter in availableAdapters"
            :key="adapter.id"
            :class="['adapter-item', { selected: selectedAdapters.includes(adapter.id) }]"
            @click="toggleAdapter(adapter.id)"
          >
            <div class="adapter-item-icon">
              <i :class="['bi', adapter.icon]"></i>
            </div>
            <div class="adapter-item-body">
              <div class="adapter-item-name">{{ adapter.name }}</div>
              <div class="adapter-item-meta">
                <span class="adapter-item-version">{{ adapter.version }}</span>
                <span :class="['adapter-item-tag', `tag-${adapter.status}`]">
                  {{ adapter.status === "ready" ? "就绪" : "准备中" }}
                </span>
              </div>
            </div>
            <i class="bi bi-check-circle-fill adapter-check"></i>
          </div>
        </div>
      </div>
    </div>

    <div v-else class="running-layout">
      <div class="running-main">
        <div class="log-section" v-show="showLogs">
          <div class="log-header">
            <i class="bi bi-terminal"></i>
            <span>运行日志</span>
            <span class="log-count">{{ logs.length }} 条</span>
          </div>
          <div class="log-body" @wheel="autoScroll">
            <div v-if="logs.length === 0" class="log-empty">等待日志输出...</div>
            <div v-for="(line, i) in logs" :key="i" class="log-line">{{ line }}</div>
          </div>
        </div>
      </div>

      <div class="running-side">
      </div>
    </div>
  </div>

  <div v-if="currentMode === 'running'" class="bottom-bar">
    <div class="bottom-bar-left">
      <button v-if="isTeamRoom" class="tb-btn" title="队伍" @click="openTeamDialog">
        <i class="bi bi-flag-fill"></i>
        <span>队伍</span>
      </button>
      <button class="tb-btn" title="运行日志" @click="showLogs = !showLogs">
        <i class="bi bi-terminal"></i>
        <span>{{ showLogs ? '隐藏日志' : '运行日志' }}</span>
      </button>
      <button class="tb-btn" title="流量统计" @click="props.showToast('流量统计开发中')">
        <i class="bi bi-graph-up"></i>
        <span>流量统计</span>
      </button>
    </div>
    <div class="bottom-bar-right">
      <button class="btn btn-danger btn-stop" @click="stopOnline">
        <i class="bi bi-stop-fill"></i>
        停止联机
      </button>
    </div>
  </div>

  <TeamDialog
    :visible="showTeamDialog"
    :room-name="roomName"
    :members="teamMembers"
    @close="showTeamDialog = false"
  />
</template>

<style scoped>
.top-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 0 16px;
}

.top-bar-left {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  color: var(--text-primary);
}

.top-bar-mode {
  font-weight: 600;
  color: var(--accent-primary);
  display: inline-flex;
  align-items: center;
  gap: 1px;
}

.top-bar-sep {
  color: var(--border-color);
}

.top-bar-room {
  color: var(--text-secondary);
}

.top-bar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.latency-badge {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 20px;
  font-weight: 500;
  background: rgba(96, 165, 250, 0.15);
  color: #60a5fa;
  font-family: monospace;
}

.status-badge {
  font-size: 12px;
  padding: 3px 12px;
  border-radius: 20px;
  font-weight: 500;
}

.status-idle {
  background: var(--bg-tertiary);
  color: var(--text-muted);
}

.status-connected {
  background: rgba(34, 197, 94, 0.15);
  color: #22c55e;
}

.status-connecting {
  background: rgba(250, 204, 21, 0.15);
  color: #eab308;
}

.bottom-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 0 0;
  border-top: 1px solid var(--border-color);
  margin-top: 16px;
}

.bottom-bar-left {
  display: flex;
  align-items: center;
  gap: 4px;
}

.bottom-bar-right {
  display: flex;
  align-items: center;
}

.tb-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 14px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.tb-btn:hover {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
  background: rgba(0, 102, 204, 0.06);
}

.tb-btn i {
  font-size: 15px;
}

.btn-stop {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 20px;
  font-size: 13px;
}

.content-area {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.form-layout {
  display: flex;
  gap: 32px;
  align-items: flex-start;
  flex-wrap: wrap;
}

.form-left {
  flex: 1;
  min-width: 300px;
  max-width: 420px;
  display: flex;
  flex-direction: column;
}

.form-right {
  width: 300px;
  flex-shrink: 0;
  min-width: 240px;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  padding-top: 24px;
}

.btn-start {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 28px;
  font-size: 15px;
}

.section-label {
  display: block;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.room-code-group {
  margin-top: 16px;
}

.no-adapter-hint {
  padding: 40px 0;
  text-align: center;
  color: var(--text-muted);
  font-size: 14px;
}

.mode-options {
  display: flex;
  gap: 10px;
  margin-bottom: 20px;
}

.mode-btn {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 16px 12px;
  border-radius: 12px;
  border: 1px solid var(--border-color);
  background: transparent;
  cursor: pointer;
  transition: all 0.15s ease;
  color: var(--text-secondary);
}

.mode-btn i {
  font-size: 22px;
}

.mode-btn:hover {
  border-color: var(--accent-primary);
  background: rgba(0, 102, 204, 0.06);
}

.mode-btn.active {
  border-color: var(--accent-primary);
  background: rgba(0, 102, 204, 0.1);
  color: var(--text-primary);
}

.mode-btn-title {
  font-size: 15px;
  font-weight: 600;
  color: inherit;
}

.mode-btn-desc {
  font-size: 11px;
  color: var(--text-muted);
}

.input-group {
  margin-bottom: 12px;
}

.input-group label {
  display: block;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 6px;
}

.form-input, .code-input {
  box-sizing: border-box;
  width: 100%;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-size: 14px;
  transition: border-color 0.15s ease;
}

.code-input {
  letter-spacing: 1px;
}

.form-input:focus,
.code-input:focus {
  outline: none;
  border-color: var(--accent-primary);
}

.opt-label {
  font-weight: 400;
  color: var(--text-muted);
  text-transform: none;
  letter-spacing: 0;
}

.adapter-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.adapter-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border-radius: 10px;
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: all 0.15s ease;
  position: relative;
}

.adapter-item:hover {
  border-color: var(--accent-primary);
  background: rgba(0, 102, 204, 0.06);
}

.adapter-item.selected {
  border-color: var(--accent-primary);
  background: rgba(0, 102, 204, 0.1);
}

.adapter-item-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  color: #fff;
  flex-shrink: 0;
}

.adapter-item-body {
  flex: 1;
  min-width: 0;
}

.adapter-item-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 3px;
}

.adapter-item-meta {
  display: flex;
  align-items: center;
  gap: 8px;
}

.adapter-item-version {
  font-size: 11px;
  color: var(--text-muted);
  font-family: monospace;
}

.adapter-item-tag {
  font-size: 10px;
  padding: 1px 8px;
  border-radius: 20px;
  font-weight: 500;
}

.tag-ready {
  background: rgba(34, 197, 94, 0.15);
  color: #22c55e;
}

.tag-preparing {
  background: rgba(250, 204, 21, 0.15);
  color: #eab308;
}

.adapter-check {
  font-size: 18px;
  color: var(--accent-primary);
  opacity: 0;
  transition: opacity 0.15s ease;
}

.adapter-item.selected .adapter-check {
  opacity: 1;
}

.running-layout {
  display: flex;
  gap: 20px;
  min-height: 200px;
  flex-wrap: wrap;
}

.running-main {
  flex: 1;
  min-width: 300px;
  display: flex;
  flex-direction: column;
}

.running-side {
  width: 260px;
  flex-shrink: 0;
  min-width: 200px;
}

.log-section, .player-section {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--bg-card);
  overflow: hidden;
}

.log-header, .player-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-tertiary);
}

.log-header i, .player-header i {
  font-size: 14px;
}

.log-count {
  margin-left: auto;
  font-weight: 400;
  color: var(--text-muted);
  text-transform: none;
  letter-spacing: 0;
}

.log-body {
  flex: 1;
  padding: 8px 14px;
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
  font-size: 12px;
  line-height: 1.7;
  overflow-y: auto;
  max-height: 260px;
  min-height: 100px;
}

.log-line {
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-all;
}

.log-empty {
  color: var(--text-muted);
  text-align: center;
  padding: 24px 0;
  font-family: inherit;
  font-size: 13px;
}

.player-body {
  padding: 8px;
  overflow-y: auto;
  flex: 1;
}

.player-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 8px;
  border-radius: 8px;
}

.player-row:hover {
  background: var(--bg-hover);
}

.player-dot {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  color: #fff;
  flex-shrink: 0;
}

.player-row-info {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.player-row-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.player-row-role {
  font-size: 11px;
  color: var(--text-muted);
}

.action-bar {
  display: flex;
  justify-content: flex-end;
  padding-top: 16px;
  border-top: 1px solid var(--border-color);
  margin-top: 16px;
}

/* Dialog styles */
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  backdrop-filter: blur(4px);
}

.dialog-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 16px;
  width: 460px;
  max-height: 85vh;
  overflow-y: auto;
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.35);
}

.dialog-card-small {
  width: 380px;
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 20px 24px 0;
}

.dialog-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-close {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  transition: all 0.15s ease;
}

.dialog-close:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.dialog-section {
  padding: 16px 24px 0;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 20px 24px;
}

.player-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 240px;
  overflow-y: auto;
}

.player-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 8px;
  background: var(--bg-tertiary);
}

.player-avatar {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  color: #fff;
  flex-shrink: 0;
}

.player-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.player-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.player-role {
  font-size: 11px;
  color: var(--text-muted);
}

.empty-list {
  text-align: center;
  color: var(--text-muted);
  padding: 20px 0;
  font-size: 14px;
}
</style>
