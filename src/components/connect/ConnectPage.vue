<script setup lang="ts">
import { ref } from "vue";
import TabBar from "../common/TabBar.vue";
import FlashConnect from "./FlashConnect.vue";
import GaojiConnect from "./GaojiConnect.vue";

defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
}>();

const emit = defineEmits<{
  runningChange: [value: boolean];
}>();

const activeTab = ref<"quick" | "advanced">(
  (localStorage.getItem("active_tab") as "quick" | "advanced") || "quick"
);

const tabs = [
  { value: "quick", label: "快速模式", icon: "bi-lightning" },
  { value: "advanced", label: "高级模式", icon: "bi-gear" },
];

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
    <TabBar :tabs="tabs" v-model:activeTab="activeTab">
      <template #right>
        <button
          v-if="activeTab === 'advanced' && !gaojiRunning"
          class="tab-bar-btn"
          @click="startAdvanced"
        >
          <i class="bi bi-play-fill"></i>
          开始联机
        </button>
      </template>
    </TabBar>

    <Transition name="tab-fade" mode="out-in">
      <div v-if="activeTab === 'quick'" key="quick" class="tab-content">
        <FlashConnect
          :show-toast="showToast"
          :player-name="playerName"
          @running-change="emit('runningChange', $event)"
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
    </Transition>
  </div>
</template>

<style scoped>
.connect-root {
  height: 100%;
  display: flex;
  flex-direction: column;
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