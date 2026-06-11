<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { joinRevampRoom, getNodes, startRevampHost } from "../../lib/revamp";
import type { RevampNode } from "../../lib/revamp";
import { session } from "../../lib/session";

const props = defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
}>();

const emit = defineEmits<{
  runningChange: [value: boolean];
}>();

const U_FORMAT = /^U\/[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}$/;
const U_LOOSE = /^U\//i;

type Status =
  | "idle"
  | "checking"
  | "ready-join-mclink"
  | "ready-join-revamp"
  | "ready-host"
  | "ready-terracotta"
  | "invalid-code"
  | "error";

const inputText = ref(session.strGet("flash_input") || "");
const status = ref<Status>("idle");
const errorMessage = ref("");
const isConnecting = ref(false);
const currentMode = ref<"none" | "running">("none");
const terracottaRoomCode = ref(""); // 陶瓦联机房间码

// Revamp 节点
const revampNodes = ref<RevampNode[]>([]);
const selectedNodeIdx = ref(-1);
const showNodeSelector = ref(false);

let checkTimer: ReturnType<typeof setTimeout> | null = null;
let checkSeq = 0;

// ===== 计算属性 =====

const canAct = computed(() => {
  return ["ready-join-mclink", "ready-join-revamp", "ready-host", "ready-terracotta"].includes(status.value)
    && !isConnecting.value;
});

const actionText = computed(() => {
  switch (status.value) {
    case "ready-join-mclink": return "加入 MC Link 房间";
    case "ready-join-revamp": return "加入 Revamp 房间";
    case "ready-host": return "创建房间并开房";
    case "ready-terracotta": return "加入陶瓦联机";
    default: return "";
  }
});

const selectedNodeAddr = computed(() => {
  if (selectedNodeIdx.value < 0 || selectedNodeIdx.value >= revampNodes.value.length) return "";
  const n = revampNodes.value[selectedNodeIdx.value];
  return `${n.ip}:${n.port}`;
});

// ===== 输入检测 =====

function triggerCheck(v: string) {
  const trimmed = v.trim();
  if (!trimmed) {
    status.value = "idle";
    return;
  }

  // 房间码格式 (U/...)
  if (U_LOOSE.test(trimmed)) {
    if (U_FORMAT.test(trimmed)) {
      status.value = "ready-terracotta";
    } else {
      status.value = "invalid-code";
    }
    return;
  }

  // 房间名 → 检查存在
  const seq = ++checkSeq;
  status.value = "checking";
  invoke<{ exists: boolean; room_type: string }>("check_room_full", { roomName: trimmed })
    .then((result) => {
      if (seq !== checkSeq) return;
      if (result.exists) {
        if (result.room_type === "mc_link") {
          status.value = "ready-join-mclink";
        } else {
          status.value = "ready-join-revamp";
          loadRevampNodes();
        }
      } else {
        status.value = "ready-host";
      }
    })
    .catch(() => {
      if (seq !== checkSeq) return;
      status.value = "error";
      errorMessage.value = "无法连接服务器，请检查网络";
    });
}

async function loadRevampNodes() {
  try {
    const nodes = await getNodes();
    revampNodes.value = nodes;
    const saved = localStorage.getItem("revamp_node_selected") || "";
    selectedNodeIdx.value = nodes.findIndex(n => `${n.ip}:${n.port}` === saved);
    if (selectedNodeIdx.value < 0 && nodes.length > 0) {
      selectedNodeIdx.value = 0;
      const addr = `${nodes[0].ip}:${nodes[0].port}`;
      localStorage.setItem("revamp_node_selected", addr);
    }
  } catch {
    revampNodes.value = [];
  }
}

watch(inputText, (val) => {
  const filtered = val.replace(/[^0-9a-zA-Z\/\-]/g, "");
  if (filtered !== val) inputText.value = filtered;
  session.strSet("flash_input", filtered);
  showNodeSelector.value = false;

  if (checkTimer) clearTimeout(checkTimer);
  if (!filtered.trim()) {
    status.value = "idle";
    return;
  }
  checkTimer = setTimeout(() => triggerCheck(filtered.trim()), 400);
});

// 从剪贴板自动检测 U/ 码
onMounted(async () => {
  const saved = inputText.value.trim();
  if (!saved) {
    try {
      const text = await navigator.clipboard.readText();
      const trimmed = text.trim();
      if (U_FORMAT.test(trimmed)) {
        inputText.value = trimmed;
        return;
      }
    } catch { /* 无剪贴板权限 */ }
  }
  if (saved) {
    triggerCheck(saved);
  }
});

onUnmounted(() => {
  if (checkTimer) clearTimeout(checkTimer);
});

// ===== 动作执行 =====

async function handleAction() {
  if (isConnecting.value) return;
  if (!canAct.value) return;

  isConnecting.value = true;

  try {
    switch (status.value) {
      case "ready-join-mclink":
        await joinMcLink();
        break;
      case "ready-join-revamp":
        await joinRevamp();
        break;
      case "ready-host":
        await hostAll();
        break;
      case "ready-terracotta":
        await joinTerracotta();
        break;
    }
  } catch (e: any) {
    props.showToast(e.toString());
  } finally {
    isConnecting.value = false;
  }
}

async function joinMcLink() {
  const roomName = inputText.value.trim();
  const password = "quick";
  const selectedRelay = localStorage.getItem("relay_selected") || "__auto__";
  const result = await invoke("start_online", {
    roomName, password, selectedRelay,
    adapters: ["mc-link"], isHost: false, playerName: props.playerName,
  });
  props.showToast(result as string);
  currentMode.value = "running";
  emit("runningChange", true);
}

async function joinRevamp() {
  const roomName = inputText.value.trim();
  const password = "quick";
  const result = await joinRevampRoom({ roomId: roomName, password, playerName: props.playerName });
  if (result.status === "ok") {
    const addr = `${result.node_ip}:${result.node_port}`;
    await navigator.clipboard.writeText(addr);
    props.showToast(`已复制节点地址: ${addr}`);
    currentMode.value = "running";
    emit("runningChange", true);
  } else {
    props.showToast(`加入失败: ${result.status}`);
  }
}

async function hostAll() {
  const roomName = inputText.value.trim();
  const password = "quick";
  const selectedRelay = localStorage.getItem("relay_selected") || "__auto__";

  // 1. 启动 MC Link 房主
  props.showToast("正在创建 MC Link 房间...");
  const mcLinkResult = await invoke("start_online", {
    roomName, password, selectedRelay,
    adapters: ["mc-link"], isHost: true, playerName: props.playerName,
  });
  props.showToast(mcLinkResult as string);

  // 2. 启动陶瓦联机房主
  props.showToast("正在创建陶瓦联机房间...");
  try {
    const terracottaResult: any = await invoke("start_terracotta_host", {
      roomCode: "",
      playerName: props.playerName,
    });
    if (terracottaResult.state === "host-ok") {
      terracottaRoomCode.value = terracottaResult.room || "";
      props.showToast(`陶瓦联机房间已创建! 房间码: ${terracottaRoomCode.value}`);
    }
  } catch (e: any) {
    props.showToast(`陶瓦联机启动失败: ${e.toString()}`);
  }

  // 3. 可选：启动 Revamp 房主（如有节点配置）
  if (localStorage.getItem("revamp_node_selected")) {
    try {
      const nodeAddr = localStorage.getItem("revamp_node_selected") || "";
      const [nodeIp, nodePort] = nodeAddr.split(":");
      await startRevampHost({
        localPort: 25565,
        nodeIp,
        nodePort,
        roomId: roomName,
        password,
        playerName: props.playerName,
      });
      props.showToast("Revamp 房间已创建");
    } catch (e: any) {
      props.showToast(`Revamp 启动失败: ${e.toString()}`);
    }
  }

  currentMode.value = "running";
  emit("runningChange", true);
}

async function joinTerracotta() {
  const roomCode = inputText.value.trim();
  const result: any = await invoke("start_terracotta_guest", {
    roomCode,
    playerName: props.playerName,
  });
  if (result.state === "guest-ok") {
    props.showToast(`已连接! 本地地址: ${result.url || "未知"}`);
    currentMode.value = "running";
    emit("runningChange", true);
  } else if (result.state === "exception") {
    props.showToast(`陶瓦联机异常 (代码: ${result.type || "未知"})`);
  }
}

function stopRunning() {
  invoke("stop_online").catch(() => {});
  currentMode.value = "none";
  terracottaRoomCode.value = "";
  emit("runningChange", false);
}

function copyRoomCode() {
  navigator.clipboard.writeText(terracottaRoomCode.value);
  props.showToast('已复制房间码');
}

function selectNode(idx: number) {
  selectedNodeIdx.value = idx;
  const node = revampNodes.value[idx];
  const addr = `${node.ip}:${node.port}`;
  localStorage.setItem("revamp_node_selected", addr);
  showNodeSelector.value = false;
  props.showToast(`已选择节点: ${node.name}`);
}
</script>

<template>
  <div class="flash-root">
    <!-- 运行中状态 -->
    <div v-if="currentMode === 'running'" class="running-section">
      <div class="running-icon"><i class="bi bi-check-circle-fill"></i></div>
      <div class="running-title">已联机</div>
      <div class="running-room">房间: {{ inputText.trim() }}</div>
      <div v-if="terracottaRoomCode" class="running-code">
        <span class="code-label">陶瓦联机房间码</span>
        <div class="code-value">{{ terracottaRoomCode }}</div>
        <button class="copy-btn" @click="copyRoomCode">
          <i class="bi bi-clipboard"></i> 复制
        </button>
      </div>
      <button class="stop-btn" @click="stopRunning">
        <i class="bi bi-stop-fill"></i> 停止联机
      </button>
    </div>

    <!-- 输入区 -->
    <template v-else>
      <div class="input-section">
        <div class="input-wrap" :class="{ focused: inputText.length > 0 }">
          <i class="bi bi-search input-icon"></i>
          <input
            v-model="inputText"
            placeholder="输入房间名或房间码…"
            autofocus
            autocomplete="off"
            spellcheck="false"
          />
          <button v-if="inputText" class="clear-btn" @click="inputText = ''">
            <i class="bi bi-x-lg"></i>
          </button>
        </div>

        <!-- 状态信息 -->
        <div class="status-area">
          <div v-if="status === 'checking'" class="status status-checking">
            <span class="spinner"></span>
            <span>正在查询房间…</span>
          </div>

          <div v-else-if="status === 'ready-join-mclink'" class="status status-ready">
            <i class="bi bi-hdd-stack"></i>
            <span>检测到 <strong>MC Link</strong> 房间「{{ inputText.trim() }}」，将以成员身份加入</span>
          </div>

          <div v-else-if="status === 'ready-join-revamp'" class="status status-ready">
            <i class="bi bi-arrow-repeat"></i>
            <span>检测到 <strong>Revamp</strong> 房间「{{ inputText.trim() }}」，将以成员身份加入</span>
          </div>

          <div v-else-if="status === 'ready-host'" class="status status-ready">
            <i class="bi bi-hdd-stack"></i>
            <span>房间「{{ inputText.trim() }}」不存在，将在 <strong>MC Link + 陶瓦联机</strong> 创建房间</span>
          </div>

          <div v-else-if="status === 'ready-terracotta'" class="status status-ready">
            <i class="bi bi-person-plus"></i>
            <span>将作为房客加入 <strong>陶瓦联机</strong> 房间</span>
          </div>

          <div v-else-if="status === 'invalid-code'" class="status status-error">
            <i class="bi bi-exclamation-triangle-fill"></i>
            <span>房间码格式不正确，正确格式：U/XXXX-XXXX-XXXX-XXXX</span>
          </div>

          <div v-else-if="status === 'error'" class="status status-error">
            <i class="bi bi-exclamation-triangle-fill"></i>
            <span>{{ errorMessage }}</span>
          </div>

          <div v-else-if="status === 'idle' && !inputText" class="status status-hint">
            <span>输入房间名创建/加入联机，或输入 <strong>U/</strong> 开头的房间码加入陶瓦联机</span>
          </div>
        </div>

        <!-- Revamp 节点选择 -->
        <div v-if="status === 'ready-join-revamp'" class="node-section">
          <button class="node-toggle" @click="showNodeSelector = !showNodeSelector">
            <i class="bi bi-hdd-network"></i>
            <span>节点: {{ selectedNodeAddr || '未选择' }}</span>
            <i :class="['bi', showNodeSelector ? 'bi-chevron-up' : 'bi-chevron-down']"></i>
          </button>
          <div v-if="showNodeSelector" class="node-list">
            <button
              v-for="(node, i) in revampNodes"
              :key="i"
              :class="['node-item', { active: i === selectedNodeIdx }]"
              @click="selectNode(i)"
            >
              <span class="node-name">{{ node.name }}</span>
              <span class="node-addr">{{ node.ip }}:{{ node.port }}</span>
            </button>
            <div v-if="revampNodes.length === 0" class="node-empty">暂无可用节点</div>
          </div>
        </div>

        <!-- 操作按钮 -->
        <button
          v-if="canAct"
          class="action-btn"
          :disabled="isConnecting"
          @click="handleAction"
        >
          <template v-if="isConnecting">
            <span class="spinner"></span> 处理中…
          </template>
          <template v-else>
            <i :class="[
              status === 'ready-host' ? 'bi-play-fill' :
              status === 'ready-terracotta' ? 'bi-person-plus' :
              'bi-box-arrow-in-right'
            ]"></i>
            {{ actionText }}
          </template>
        </button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.flash-root {
  width: 100%;
  max-width: 520px;
  margin: 32px auto 0;
  padding: 0 16px;
}

/* ===== 输入区 ===== */
.input-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.input-wrap {
  display: flex;
  align-items: center;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 0 4px 0 14px;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}
.input-wrap.focused {
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-primary) 15%, transparent);
}

.input-icon {
  color: var(--text-muted);
  font-size: 15px;
  flex-shrink: 0;
}

.input-wrap input {
  flex: 1;
  padding: 12px 10px;
  font-size: 15px;
  background: transparent;
  border: none;
  outline: none;
  color: var(--text-primary);
  min-width: 0;
}
.input-wrap input::placeholder {
  color: var(--text-muted);
}

.clear-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--text-muted);
  font-size: 12px;
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.1s ease;
}
.clear-btn:hover {
  background: var(--bg-hover);
  color: var(--text-secondary);
}

/* ===== 状态区 ===== */
.status-area {
  min-height: 32px;
}

.status {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  padding: 6px 12px;
  border-radius: 8px;
  line-height: 1.4;
}

.status-hint {
  color: var(--text-muted);
}

.status-checking {
  color: var(--text-secondary);
}

.status-ready {
  background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
  color: var(--text-primary);
}
.status-ready i {
  color: var(--accent-primary);
  font-size: 14px;
}

.status-error {
  background: color-mix(in srgb, #e74c3c 10%, transparent);
  color: #e74c3c;
}
.status-error i {
  font-size: 14px;
}

/* ===== Spinner ===== */
.spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid var(--text-muted);
  border-top-color: var(--accent-primary);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* ===== 节点选择 ===== */
.node-section {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.node-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
  transition: background 0.1s ease;
}
.node-toggle:hover {
  background: var(--bg-hover);
}
.node-toggle i:first-child {
  color: var(--accent-primary);
}
.node-toggle i:last-child {
  margin-left: auto;
  color: var(--text-muted);
}

.node-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  max-height: 200px;
  overflow-y: auto;
}

.node-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
  text-align: left;
  transition: background 0.08s ease;
}
.node-item:hover {
  background: var(--bg-hover);
}
.node-item.active {
  background: color-mix(in srgb, var(--accent-primary) 15%, transparent);
}

.node-name {
  font-weight: 500;
}
.node-addr {
  color: var(--text-muted);
  font-size: 12px;
}

.node-empty {
  padding: 16px;
  text-align: center;
  color: var(--text-muted);
  font-size: 13px;
}

/* ===== 操作按钮 ===== */
.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 12px 24px;
  border: none;
  border-radius: 10px;
  background: var(--accent-primary);
  color: #fff;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.15s ease;
  margin-top: 4px;
}
.action-btn:hover:not(:disabled) {
  opacity: 0.9;
}
.action-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* ===== 运行中 ===== */
.running-section {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 24px 0;
  text-align: center;
}

.running-icon {
  font-size: 48px;
  color: #2ecc71;
}

.running-title {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
}

.running-room {
  font-size: 14px;
  color: var(--text-secondary);
}

.running-code {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 16px 24px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  margin-top: 8px;
}
.code-label {
  font-size: 12px;
  color: var(--text-muted);
}
.code-value {
  font-size: 18px;
  font-weight: 700;
  letter-spacing: 1px;
  color: var(--accent-primary);
  font-family: monospace;
}
.copy-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 14px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-hover);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.1s ease;
}
.copy-btn:hover {
  background: var(--accent-primary);
  color: #fff;
  border-color: var(--accent-primary);
}

.stop-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 24px;
  border: none;
  border-radius: 8px;
  background: #e74c3c;
  color: #fff;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: opacity 0.15s ease;
}
.stop-btn:hover {
  opacity: 0.9;
}
</style>