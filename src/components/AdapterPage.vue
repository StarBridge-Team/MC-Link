<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const props = defineProps<{
  showToast: (msg: string) => void;
}>();

interface AdapterStatus {
  installed: boolean;
  running: boolean;
  starting: boolean;
  port: number | null;
}

type UiState = "not_installed" | "downloading" | "installed" | "starting" | "ready";

interface Adapter {
  id: string;
  name: string;
  description: string;
  icon: string;
  uiState: UiState;
}

const loading = ref(false);

const terracottaUiState = ref<UiState>("not_installed");

const downloadProgress = ref(0);
const downloadingId = ref<string | null>(null);

const adapters = computed<Adapter[]>(() => [
  {
    id: "mc-link",
    name: "MC Link",
    description: "跨版本 Minecraft 联机隧道，支持局域网穿透与快速匹配",
    icon: "bi-link-45deg",
    uiState: "ready",
  },
  {
    id: "taoli",
    name: "陶瓦联机 (Terracotta)",
    description: "基于陶瓦协议的高效联机适配器，低延迟、高吞吐",
    icon: "bi-cpu",
    uiState: terracottaUiState.value,
  },
]);

const PAGE_SIZE = 4;
const currentPage = ref(1);
const totalPages = computed(() => Math.max(1, Math.ceil(adapters.value.length / PAGE_SIZE)));
const currentPageItems = computed(() => {
  const start = (currentPage.value - 1) * PAGE_SIZE;
  return adapters.value.slice(start, start + PAGE_SIZE);
});

function goToPage(page: number) {
  if (page >= 1 && page <= totalPages.value) {
    currentPage.value = page;
  }
}

async function fetchStatus() {
  if (loading.value) return;

  try {
    const status: AdapterStatus = await invoke("get_adapter_status");
    if (!status.installed) {
      terracottaUiState.value = "not_installed";
    } else if (status.starting) {
      terracottaUiState.value = "starting";
    } else if (status.running) {
      terracottaUiState.value = "ready";
    } else {
      terracottaUiState.value = "installed";
    }
  } catch {
    terracottaUiState.value = "not_installed";
  }
}

let unlistenProgress: (() => void) | null = null;

onMounted(async () => {
  fetchStatus();

  unlistenProgress = await listen<number>("download-progress", (event) => {
    downloadProgress.value = event.payload;
  });
});

onUnmounted(() => {
  if (unlistenProgress) {
    unlistenProgress();
  }
});

async function downloadAdapter() {
  if (loading.value) return;
  loading.value = true;
  downloadingId.value = "taoli";
  downloadProgress.value = 0;
  terracottaUiState.value = "downloading";
  try {
    await invoke("download_adapter");
    props.showToast("陶瓦联机已安装并启动");
    terracottaUiState.value = "ready";
  } catch (e: any) {
    props.showToast("下载失败: " + e);
    terracottaUiState.value = "not_installed";
  } finally {
    loading.value = false;
    downloadingId.value = null;
    downloadProgress.value = 0;
  }
}

async function startAdapter() {
  if (loading.value) return;
  loading.value = true;
  terracottaUiState.value = "starting";
  try {
    const result: string = await invoke("start_adapter");
    props.showToast(result);
    terracottaUiState.value = "ready";
  } catch (e: any) {
    props.showToast(e.toString());
    terracottaUiState.value = "installed";
  } finally {
    loading.value = false;
  }
}

async function stopAdapter() {
  if (loading.value) return;
  loading.value = true;
  try {
    const result: string = await invoke("stop_adapter");
    props.showToast(result);
    terracottaUiState.value = "installed";
  } catch (e: any) {
    props.showToast(e.toString());
  } finally {
    loading.value = false;
  }
}

function statusLabel(state: UiState): string {
  switch (state) {
    case "not_installed": return "未安装";
    case "downloading": return "下载中";
    case "installed": return "已安装";
    case "starting": return "启动中";
    case "ready": return "就绪";
  }
}

function statusClass(state: UiState): string {
  switch (state) {
    case "ready": return "status-ready";
    case "starting":
    case "downloading": return "status-starting";
    case "installed": return "status-installed";
    default: return "status-not-installed";
  }
}
</script>

<template>
  <div class="page-wrapper">
    <div class="page-header">
      <h2>适配器</h2>
      <span class="adapter-count">{{ adapters.length }} 个适配器</span>
    </div>

    <div class="adapter-grid">
      <div
        v-for="adapter in currentPageItems"
        :key="adapter.id"
        class="adapter-card"
      >
        <div class="card-icon">
          <i :class="['bi', adapter.icon]"></i>
        </div>
        <div class="card-body">
          <div class="card-name">{{ adapter.name }}</div>
          <div class="card-desc">{{ adapter.description }}</div>
        </div>
        <div class="card-footer">
          <div class="card-footer-left">
            <template v-if="adapter.uiState === 'downloading' && downloadingId === adapter.id && downloadProgress > 0">
              <div class="progress-bar-wrapper">
                <div class="progress-bar" :style="{ width: downloadProgress + '%' }"></div>
              </div>
              <span class="progress-text">{{ downloadProgress }}%</span>
            </template>
            <span v-else :class="['card-status', statusClass(adapter.uiState)]">
              {{ statusLabel(adapter.uiState) }}
            </span>
          </div>

          <template v-if="adapter.id === 'taoli'">
            <button
              v-if="adapter.uiState === 'not_installed'"
              class="card-action-btn download-btn"
              :disabled="loading"
              @click="downloadAdapter"
            >
              <i class="bi bi-download"></i>
            </button>
            <button
              v-else-if="adapter.uiState === 'installed'"
              class="card-action-btn launch-btn"
              :disabled="loading"
              @click="startAdapter"
            >
              <i class="bi bi-play-fill"></i> 启动
            </button>
            <button
              v-else-if="adapter.uiState === 'ready'"
              class="card-action-btn stop-btn"
              :disabled="loading"
              @click="stopAdapter"
            >
              <i class="bi bi-stop-fill"></i> 停止
            </button>
          </template>
        </div>
      </div>
    </div>

    <div v-if="totalPages > 1" class="pagination">
      <button
        class="page-btn"
        :disabled="currentPage <= 1"
        @click="goToPage(currentPage - 1)"
      >
        <i class="bi bi-chevron-left"></i>
      </button>
      <button
        v-for="p in totalPages"
        :key="p"
        :class="['page-btn', { active: p === currentPage }]"
        @click="goToPage(p)"
      >
        {{ p }}
      </button>
      <button
        class="page-btn"
        :disabled="currentPage >= totalPages"
        @click="goToPage(currentPage + 1)"
      >
        <i class="bi bi-chevron-right"></i>
      </button>
    </div>
  </div>
</template>

<style scoped>
.page-wrapper {
  padding: 20px 28px;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.page-header {
  display: flex;
  align-items: baseline;
  gap: 12px;
  margin-bottom: 24px;
}

.page-header h2 {
  font-size: 22px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
}

.adapter-count {
  font-size: 13px;
  color: var(--text-muted);
}

.adapter-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;
  flex: 1;
}

.adapter-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 20px;
  display: flex;
  flex-direction: column;
  transition: all 0.2s ease;
  cursor: default;
}

.adapter-card:hover {
  border-color: var(--border-hover);
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
  transform: translateY(-2px);
}

.card-icon {
  width: 44px;
  height: 44px;
  border-radius: 10px;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  color: #fff;
  margin-bottom: 14px;
}

.card-body {
  flex: 1;
}

.card-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 6px;
}

.card-desc {
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--text-muted);
}

.card-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--border-color);
}

.card-footer-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
}

.progress-bar-wrapper {
  width: 100px;
  height: 6px;
  background: var(--border-color);
  border-radius: 3px;
  overflow: hidden;
  flex-shrink: 0;
}

.progress-bar {
  height: 100%;
  background: var(--accent-primary);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.progress-text {
  font-size: 11px;
  font-weight: 600;
  color: var(--accent-primary);
  white-space: nowrap;
}

.card-status {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  padding: 2px 10px;
  border-radius: 20px;
  font-weight: 500;
}

.status-ready {
  background: rgba(34, 197, 94, 0.15);
  color: #22c55e;
}

.status-starting {
  background: rgba(96, 165, 250, 0.15);
  color: #60a5fa;
}

.status-installed {
  background: rgba(96, 165, 250, 0.15);
  color: #60a5fa;
}

.status-not-installed {
  background: rgba(160, 160, 176, 0.15);
  color: var(--text-muted);
}

.card-action-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 12px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  border: none;
  cursor: pointer;
  transition: all 0.15s ease;
}

.card-action-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.download-btn {
  background: var(--accent-primary);
  color: #fff;
}

.download-btn:hover:not(:disabled) {
  background: var(--accent-hover);
}

.launch-btn {
  background: rgba(34, 197, 94, 0.15);
  color: #22c55e;
}

.launch-btn:hover:not(:disabled) {
  background: #22c55e;
  color: #fff;
}

.stop-btn {
  background: rgba(239, 68, 68, 0.15);
  color: #ef4444;
}

.stop-btn:hover:not(:disabled) {
  background: #ef4444;
  color: #fff;
}

.pagination {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  margin-top: 20px;
  padding-bottom: 8px;
}

.page-btn {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s ease;
}

.page-btn:hover:not(:disabled) {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
}

.page-btn.active {
  background: var(--accent-primary);
  border-color: var(--accent-primary);
  color: #fff;
}

.page-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
</style>
