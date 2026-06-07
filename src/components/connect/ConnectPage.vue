<script setup lang="ts">
import { ref, computed, watch } from "vue";
import FlashConnect from "./FlashConnect.vue";
import GaojiConnect from "./GaojiConnect.vue";
import Kaifaing from "../Kaifaing.vue";

defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
}>();

const emit = defineEmits<{
  runningChange: [value: boolean];
}>();

const activeTab = ref<"quick" | "advanced" | "rooms">(
  (localStorage.getItem("active_tab") as "quick" | "advanced" | "rooms") || "quick"
);

watch(activeTab, (val) => localStorage.setItem("active_tab", val));

// Quick mode button state (received from FlashConnect)
const flashState = ref("idle");
const flashMode = ref("");
function onFlashStateChange(s: { quickState: string; quickMode: string }) {
  flashState.value = s.quickState;
  flashMode.value = s.quickMode;
}

const flashBtnText = computed(() => {
  if (flashMode.value === "host") return "创建并开房";
  if (flashMode.value === "member" || flashMode.value === "terracotta") return "加入房间";
  return "开始联机";
});

const flashBtnIcon = computed(() => {
  if (flashMode.value === "host") return "bi-play-fill";
  return "bi-box-arrow-in-right";
});

const flashRef = ref<InstanceType<typeof FlashConnect> | null>(null);

function startQuick() {
  flashRef.value?.startQuick?.();
}

const gaojiRef = ref<any>(null);
const gaojiRunning = ref(false);
function onGaojiRunningChange(v: boolean) {
  gaojiRunning.value = v;
  emit('runningChange', v);
}
function startAdvanced() {
  gaojiRef.value?.confirmStart?.();
}
</script>

<template>
  <div class="connect-root">
    <!-- 标签栏 -->
    <div class="tab-bar">
      <button
        :class="['tab-btn', { active: activeTab === 'quick' }]"
        @click="activeTab = 'quick'"
      >
        <i class="bi bi-lightning"></i>
        <span>快速模式</span>
      </button>
      <button
        :class="['tab-btn', { active: activeTab === 'advanced' }]"
        @click="activeTab = 'advanced'"
      >
        <i class="bi bi-gear"></i>
        <span>高级模式</span>
      </button>
      <button
        :class="['tab-btn', { active: activeTab === 'rooms' }]"
        @click="activeTab = 'rooms'"
      >
        <i class="bi bi-card-list"></i>
        <span>房间列表</span>
      </button>
      <div class="tab-bar-right">
        <button
          v-if="activeTab === 'quick'"
          class="tab-bar-btn"
          :disabled="flashState !== 'ready'"
          @click="startQuick"
        >
          <i :class="['bi', flashBtnIcon]"></i>
          {{ flashBtnText }}
        </button>
        <button
          v-if="activeTab === 'advanced' && !gaojiRunning"
          class="tab-bar-btn"
          @click="startAdvanced"
        >
          <i class="bi bi-play-fill"></i>
          开始联机
        </button>
      </div>
    </div>

    <Transition name="tab-fade" mode="out-in">
      <div v-if="activeTab === 'quick'" key="quick" class="tab-content">
        <FlashConnect
          ref="flashRef"
          :show-toast="showToast"
          :player-name="playerName"
          @running-change="emit('runningChange', $event)"
          @state-change="onFlashStateChange"
        />
      </div>

      <div v-else-if="activeTab === 'advanced'" key="advanced" class="tab-content">
        <GaojiConnect
          ref="gaojiRef"
          :show-toast="showToast"
          :player-name="playerName"
          @running-change="onGaojiRunningChange"
        />
      </div>

      <div v-else-if="activeTab === 'rooms'" key="rooms" class="tab-content">
        <Kaifaing />
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.connect-root {
  height: 100%;
  display: flex;
  flex-direction: column;
}

/* ===== 标签栏 ===== */
.tab-bar {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-bottom: 20px;
  padding-bottom: 0;
}

.tab-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 20px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  transition: all 0.15s ease;
}

.tab-btn i {
  font-size: 15px;
}

.tab-btn:hover {
  color: var(--text-secondary);
  background: var(--bg-hover);
  border-radius: 8px 8px 0 0;
}

.tab-btn.active {
  color: var(--accent-primary);
  border-bottom-color: var(--accent-primary);
}

.tab-bar-right {
  margin-left: auto;
  display: flex;
  align-items: center;
}

.tab-bar-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 20px;
  border-radius: 8px;
  border: none;
  background: var(--accent-primary);
  color: #fff;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.tab-bar-btn:hover:not(:disabled) {
  opacity: 0.9;
}

.tab-bar-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* ===== 切换动画 ===== */
.tab-fade-enter-active,
.tab-fade-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
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
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
</style>
