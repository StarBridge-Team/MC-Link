<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
}>();

const emit = defineEmits<{
  runningChange: [value: boolean];
  stateChange: [state: { quickState: string; quickMode: string }];
}>();

const currentMode = ref<"none" | "running">("none");
const isConnecting = ref(false);
const quickInput = ref(localStorage.getItem("flash_input") || "");
const quickPassword = ref(localStorage.getItem("flash_password") || "");
const quickState = ref<"idle" | "checking" | "ready" | "error">("idle");
const quickMode = ref<"host" | "member" | "terracotta" | "">("");
const quickLabel = ref("");
const quickDesc = ref("");

const U_FORMAT = /^U\/[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}$/;

let quickCheckTimer: ReturnType<typeof setTimeout> | null = null;
let quickCheckSeq = 0;

function triggerCheck(v: string) {
  if (U_FORMAT.test(v)) {
    quickMode.value = "terracotta";
    quickLabel.value = "陶瓦联机";
    quickDesc.value = "将以成员身份加入此房间";
    quickState.value = "ready";
    return;
  }
  const seq = ++quickCheckSeq;
  invoke<boolean>("check_room_exists", { roomName: v })
    .then((exists) => {
      if (seq !== quickCheckSeq) return;
      if (exists) {
        quickMode.value = "member";
        quickLabel.value = "MC Link";
        quickDesc.value = "房间已存在，将以成员身份加入";
      } else {
        quickMode.value = "host";
        quickLabel.value = "MC Link";
        quickDesc.value = "房间不存在，将以房主身份创建";
      }
      quickState.value = "ready";
    })
    .catch(() => {
      if (seq !== quickCheckSeq) return;
      quickMode.value = "host";
      quickLabel.value = "MC Link";
      quickDesc.value = "无法连接中央服务器，将以房主模式启动";
      quickState.value = "ready";
    });
}

const isMcLink = computed(() => quickMode.value === "host" || quickMode.value === "member");

watch(quickInput, (val) => {
  localStorage.setItem("flash_input", val);
  if (quickCheckTimer) clearTimeout(quickCheckTimer);
  if (!val.trim()) {
    quickState.value = "idle";
    quickMode.value = "";
    quickLabel.value = "";
    quickDesc.value = "";
    return;
  }
  quickState.value = "checking";
  quickCheckTimer = setTimeout(() => triggerCheck(val.trim()), 500);
});

watch(quickPassword, (val) => {
  localStorage.setItem("flash_password", val);
});

watch([quickState, quickMode], () => {
  emit("stateChange", { quickState: quickState.value, quickMode: quickMode.value });
});

// 切换页面回来后自动检测输入框内容
onMounted(() => {
  if (quickInput.value.trim()) {
    quickState.value = "checking";
    triggerCheck(quickInput.value.trim());
  }
});

function onQuickEnter() {
  if (quickState.value === "ready") startQuick();
}

async function startQuick() {
  if (!quickInput.value.trim() || quickState.value !== "ready") return;
  const v = quickInput.value.trim();

  isConnecting.value = true;
  try {
    if (quickMode.value === "terracotta") {
      const result: any = await invoke("start_terracotta_guest", {
        roomCode: v,
        playerName: props.playerName,
      });
      if (result.state === "guest-ok") {
        props.showToast(`已连接! 本地地址: ${result.url || "未知"}`);
        currentMode.value = "running";
        emit("runningChange", true);
      } else if (result.state === "exception") {
        props.showToast(`陶瓦联机异常 (代码: ${result.type || "未知"})`);
      }
    } else if (quickMode.value === "member") {
      const selectedRelay = localStorage.getItem("relay_selected") || "__auto__";
      const result = await invoke("start_online", {
        roomName: v,
        password: quickPassword.value || "quick",
        selectedRelay,
        adapters: ["mc-link"],
        isHost: false,
        playerName: props.playerName,
      });
      props.showToast(result as string);
      currentMode.value = "running";
      emit("runningChange", true);
    } else if (quickMode.value === "host") {
      const selectedRelay = localStorage.getItem("relay_selected") || "__auto__";
      const result = await invoke("start_online", {
        roomName: v,
        password: quickPassword.value || "quick",
        selectedRelay,
        adapters: ["mc-link"],
        isHost: true,
        playerName: props.playerName,
      });
      props.showToast(result as string);
      currentMode.value = "running";
      emit("runningChange", true);
    }
  } catch (e: any) {
    props.showToast(e.toString());
    currentMode.value = "none";
    emit("runningChange", false);
  } finally {
    isConnecting.value = false;
  }
}

defineExpose({ quickState, quickMode, startQuick });
</script>

<template>
  <div class="quick-root">
    <div class="quick-body">
      <div class="quick-title">输入房间名或房间码</div>
      <input
        type="text"
        v-model="quickInput"
        placeholder="U/XXXX-XXXX-XXXX-XXXX"
        class="quick-input"
        @keydown.enter="onQuickEnter"
      />

      <input
        v-if="isMcLink && quickState === 'ready'"
        type="password"
        v-model="quickPassword"
        placeholder="输入房间密码"
        class="quick-input quick-password"
        @keydown.enter="onQuickEnter"
      />

      <p v-if="quickState === 'idle'" class="quick-hint">输入后自动检测房间状态，按 Enter 快速开始</p>

      <div v-else-if="quickState === 'checking'" class="quick-result-line checking">
        <span class="quick-spinner"></span>
        <span>检测中...</span>
      </div>

      <div v-else-if="quickState === 'ready'" class="quick-result-line">
        <span class="quick-result-text">{{ quickLabel }} — {{ quickDesc }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.quick-root {
  width: 100%;
  max-width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 24px;
  padding: 48px 24px;
  box-sizing: border-box;
}

.quick-body {
  width: 100%;
  text-align: left;
}

.quick-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 14px;
}

.quick-input {
  box-sizing: border-box;
  width: 100%;
  padding: 14px 18px;
  border-radius: 12px;
  border: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-size: 15px;
  transition: border-color 0.15s ease;
}

.quick-input:focus {
  outline: none;
  border-color: var(--accent-primary);
}

.quick-input::placeholder {
  color: var(--text-muted);
}

.quick-password {
  margin-top: 10px;
}

.quick-hint {
  margin-top: 10px;
  font-size: 12px;
  color: var(--text-muted);
  text-align: left;
}

.quick-result-line {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 10px;
}

.quick-result-line.checking {
  color: var(--text-muted);
}

.quick-spinner {
  display: inline-block;
  width: 16px;
  height: 16px;
  border: 2px solid var(--border-color);
  border-top-color: var(--accent-primary);
  border-radius: 50%;
  animation: quick-spin 0.6s linear infinite;
}

@keyframes quick-spin {
  to { transform: rotate(360deg); }
}

.quick-result-text {
  font-size: 13px;
  color: var(--text-secondary);
}
</style>
