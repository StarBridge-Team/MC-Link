<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
import FlashConnect from "./FlashConnect.vue";
import GaojiConnect from "./GaojiConnect.vue";
import ConnectionStatus from "./ConnectionStatus.vue";
import {
  startP2PConnection,
  stopP2PConnection,
  getP2PStatus,
  onP2PEvent,
  type P2PStatus,
  type P2PEvent,
  type StartP2PArgs,
} from "../../lib/api/connect";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { local, KEYS } from "../../lib/persist";

const props = defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
  isRunning: boolean;
}>();

const emit = defineEmits<{
  runningChange: [value: boolean];
}>();

const activeTab = ref<"quick" | "advanced">(
  (local.getString(KEYS.activeTab) as "quick" | "advanced") || "quick"
);
const gaojiRef = ref<InstanceType<typeof GaojiConnect> | null>(null);

const status = ref<P2PStatus | null>(null);
const logs = ref<Array<{ text: string; level: "info" | "success" | "warn" | "error" }>>([]);
let unlisten: UnlistenFn | null = null;

function onTabChange(name: string) {
  activeTab.value = name as "quick" | "advanced";
  local.setString(KEYS.activeTab, name);
}

function setRunning(v: boolean) {
  if (props.isRunning === v) return;
  emit("runningChange", v);
}

const stage = computed(() => status.value?.stage ?? "idle");

function levelFor(stage: P2PEvent["stage"], text: string) {
  if (stage === "error") return "error" as const;
  if (stage === "stopped") return "warn" as const;
  if (text.includes("✅") || text.includes("成功") || stage === "connected") return "success" as const;
  if (text.includes("⚠") || text.includes("失败")) return "warn" as const;
  return "info" as const;
}

async function startConnection(args: StartP2PArgs) {
  logs.value = [];
  status.value = null;
  setRunning(true);
  try {
    await startP2PConnection(args);
  } catch (e: any) {
    logs.value.push({ text: `启动连接失败: ${e}`, level: "error" });
    setRunning(false);
  }
}

async function stopConnection() {
  try {
    await stopP2PConnection();
  } catch (e: any) {
    props.showToast(`停止失败: ${e}`);
  }
}

function startAdvanced() {
  gaojiRef.value?.triggerStart();
}

onMounted(async () => {
  unlisten = await onP2PEvent((evt: P2PEvent) => {
    status.value = evt.status;
    logs.value.push({ text: evt.message, level: levelFor(evt.stage, evt.message) });
    if (logs.value.length > 300) logs.value.shift();

    if (evt.stage === "connected") setRunning(true);
    if (evt.stage === "error" || evt.stage === "stopped") setRunning(false);
  });
  // 初始拉取一次状态
  try {
    status.value = await getP2PStatus();
  } catch { /* 后端尚未就绪 */ }
});

onUnmounted(() => {
  if (unlisten) unlisten();
});

defineExpose({ startConnection, stopConnection });
</script>

<template>
  <div class="connect-root">
    <el-tabs v-model="activeTab" class="connect-tabs" @tab-change="onTabChange">
      <el-tab-pane name="quick">
        <template #label>
          <span class="tab-label"><i class="bi bi-lightning"></i> 快速模式</span>
        </template>
      </el-tab-pane>
      <el-tab-pane name="advanced">
        <template #label>
          <span class="tab-label">
            <i class="bi bi-gear"></i> 高级模式
            <el-button
              v-if="activeTab === 'advanced' && stage !== 'connecting' && stage !== 'connected'"
              type="primary"
              size="small"
              class="start-btn"
              @click.stop="startAdvanced"
            >
              <i class="bi bi-play-fill"></i> 开始联机
            </el-button>
          </span>
        </template>
      </el-tab-pane>
    </el-tabs>

    <Transition name="tab-fade" mode="out-in">
      <div v-if="activeTab === 'quick'" key="quick" class="tab-content">
        <FlashConnect
          :show-toast="showToast"
          :player-name="playerName"
          :stage="stage"
          @start="startConnection"
          @stop="stopConnection"
        />
      </div>

      <div v-else-if="activeTab === 'advanced'" key="advanced" class="tab-content">
        <GaojiConnect
          ref="gaojiRef"
          :show-toast="showToast"
          :player-name="playerName"
          :stage="stage"
          @start="startConnection"
          @stop="stopConnection"
        />
      </div>
    </Transition>

    <ConnectionStatus :status="status" :logs="logs" @stop="stopConnection" />
  </div>
</template>

<style scoped>
.connect-root {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
}

.connect-tabs :deep(.el-tabs__header) {
  margin: 0;
}

.connect-tabs :deep(.el-tabs__nav-wrap::after) {
  background-color: var(--border-color);
}

.connect-tabs :deep(.el-tabs__item) {
  color: var(--text-muted);
  font-size: var(--fs-md);
  font-weight: var(--fw-medium);
}

.connect-tabs :deep(.el-tabs__item.is-active) {
  color: var(--accent-primary);
}

.connect-tabs :deep(.el-tabs__active-bar) {
  background-color: var(--accent-primary);
}

.tab-label {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
}

.start-btn {
  margin-left: var(--sp-3);
}

.tab-fade-enter-active,
.tab-fade-leave-active {
  transition: opacity var(--motion-base) var(--ease),
              transform var(--motion-base) var(--ease);
}
.tab-fade-enter-from {
  opacity: 0;
  transform: translateY(6px);
}
.tab-fade-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

.tab-content {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
</style>
