<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{
  showToast: (msg: string) => void;
  searchQuery: string;
}>();

interface RelayInfo {
  id: string;
  name: string;
  address: string;
}

interface IpInfo {
  region: string;
  isp: string;
}

const AUTO_ID = "__auto__";

function loadCachedRelayList(): RelayInfo[] {
  try {
    const cached = localStorage.getItem("relay_list");
    return cached ? JSON.parse(cached) : [];
  } catch { return []; }
}

const relayList = ref<RelayInfo[]>(loadCachedRelayList());
const selectedRelay = ref<string | null>(localStorage.getItem("relay_selected") || AUTO_ID);
watch(selectedRelay, (val) => {
  localStorage.setItem("relay_selected", val || AUTO_ID);
});
const customRelay = ref<{ address: string; latency: number; pinging: boolean } | null>(null);
const ipInfoMap = ref<Record<string, IpInfo>>(loadCachedIpInfo());

function loadCachedIpInfo(): Record<string, IpInfo> {
  try {
    const cached = localStorage.getItem("relay_ipinfo");
    return cached ? JSON.parse(cached) : {};
  } catch { return {}; }
}

let pingTimer: ReturnType<typeof setTimeout> | null = null;

interface RelayEntry {
  id: string;
  name: string;
  address: string;
  isAuto: boolean;
}

const autoEntry: RelayEntry = { id: AUTO_ID, name: "自动选择", address: "自动选择最优中继节点", isAuto: true };

const filteredEntries = computed(() => {
  const q = props.searchQuery.toLowerCase();
  const matches = (entry: RelayEntry) => {
    if (entry.isAuto) return !q || "自动选择".includes(q) || "自动选择最优中继节点".includes(q);
    const info = ipInfoMap.value[entry.address];
    const ipMatch = info ? `${info.region} ${info.isp}`.toLowerCase().includes(q) : false;
    return !q || entry.name.toLowerCase().includes(q) || entry.address.toLowerCase().includes(q) || ipMatch;
  };
  const entries: RelayEntry[] = [autoEntry, ...relayList.value.map(r => ({ ...r, isAuto: false }))];
  return entries.filter(matches);
});

function extractHost(addr: string): string {
  const idx = addr.lastIndexOf(':');
  return idx > 0 ? addr.slice(0, idx) : addr;
}

async function fetchIpInfo(addr: string) {
  if (ipInfoMap.value[addr]) return;
  const host = extractHost(addr);
  try {
    const info = await invoke<IpInfo>("get_ip_info", { host });
    ipInfoMap.value[addr] = info;
  } catch {
    ipInfoMap.value[addr] = { region: "", isp: "" };
  }
}

watch([() => props.searchQuery], ([q]) => {
  if (pingTimer) clearTimeout(pingTimer);
  customRelay.value = null;
  const hasAddr = /[.:]/.test(q);
  if (!hasAddr) return;
  customRelay.value = { address: q, latency: 0, pinging: true };
  pingTimer = setTimeout(async () => {
    try {
      const ms = await invoke<number>("ping_relay", { address: q });
      customRelay.value = { address: q, latency: ms, pinging: false };
      fetchIpInfo(q);
      try { localStorage.setItem("relay_ipinfo", JSON.stringify(ipInfoMap.value)); } catch {}
    } catch {
      customRelay.value = null;
    }
  }, 500);
});

onUnmounted(() => {
  if (pingTimer) clearTimeout(pingTimer);
});

async function loadRelays() {
  try {
    const relays = await invoke<RelayInfo[]>("get_relays");
    relayList.value = relays;
    localStorage.setItem("relay_list", JSON.stringify(relays));
    // 批量获取 IP 信息，完成后一次性保存
    await Promise.allSettled(relays.map(r => fetchIpInfo(r.address)));
    try { localStorage.setItem("relay_ipinfo", JSON.stringify(ipInfoMap.value)); } catch {}
  } catch {
    // 静默失败
  }
}

defineExpose({ loadRelays });
</script>

<template>
  <div class="relay-list">
    <div
      v-for="entry in filteredEntries"
      :key="entry.id"
      class="relay-card"
      :class="{ selected: selectedRelay === entry.id }"
      @click="selectedRelay = entry.id"
    >
      <div class="relay-card-left">
        <i v-if="entry.isAuto" class="bi bi-router"></i>
        <i v-else class="bi bi-hdd-network"></i>
        <div>
          <span class="relay-name">{{ entry.name }}</span>
          <span class="relay-addr">
            {{ entry.address }}
            <span v-if="ipInfoMap[entry.address]?.region" class="relay-ipinfo">
              {{ ipInfoMap[entry.address].region }} · {{ ipInfoMap[entry.address].isp }}
            </span>
            <span v-else-if="ipInfoMap[entry.address]" class="relay-ipinfo unknown">未知</span>
          </span>
        </div>
      </div>
      <i v-if="selectedRelay === entry.id" class="bi bi-check-circle-fill check-icon"></i>
    </div>

    <div
      v-if="customRelay && !customRelay.pinging"
      class="relay-card"
      :class="{ selected: selectedRelay === customRelay.address }"
      @click="selectedRelay = customRelay!.address"
    >
      <div class="relay-card-left">
        <i class="bi bi-plug"></i>
        <div>
          <span class="relay-name">{{ customRelay.address }}</span>
          <span class="relay-addr">
            {{ customRelay.latency }}ms
            <span v-if="ipInfoMap[customRelay.address]?.region" class="relay-ipinfo">
              {{ ipInfoMap[customRelay.address].region }} · {{ ipInfoMap[customRelay.address].isp }}
            </span>
            <span v-else-if="ipInfoMap[customRelay.address]" class="relay-ipinfo unknown">未知</span>
          </span>
        </div>
      </div>
      <i v-if="selectedRelay === customRelay.address" class="bi bi-check-circle-fill check-icon"></i>
    </div>

    <div v-if="customRelay && customRelay.pinging" class="text-muted" style="padding:20px 0;text-align:center">
      正在探测...
    </div>

    <div v-if="filteredEntries.length === 0 && !customRelay" class="text-muted" style="padding:30px 0;text-align:center">
      暂无匹配的中继服务器
    </div>
  </div>
</template>

<style scoped>
.relay-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 16px;
}

.relay-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 14px 16px;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.relay-card:hover {
  border-color: var(--accent-primary);
  background: rgba(255, 255, 255, 0.05);
}

.relay-card-left {
  display: flex;
  align-items: center;
  gap: 14px;
  font-size: 22px;
  color: var(--accent-primary);
}

.relay-card-left div {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.relay-name {
  color: var(--text-primary);
  font-size: 14px;
  font-weight: 500;
}

.relay-addr {
  color: var(--text-muted);
  font-size: 12px;
}

.relay-ipinfo {
  color: var(--accent-primary);
  opacity: 0.8;
  font-size: 11px;
  margin-left: 4px;
}

.relay-ipinfo.unknown {
  color: var(--text-muted);
  opacity: 0.6;
}

.relay-card.selected {
  border-color: var(--accent-primary);
  background: rgba(0, 102, 204, 0.08);
  box-shadow: 0 2px 12px rgba(0, 102, 204, 0.2);
}

.check-icon {
  font-size: 18px;
  color: var(--accent-primary);
  flex-shrink: 0;
}
</style>