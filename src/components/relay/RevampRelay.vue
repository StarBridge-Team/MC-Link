<script setup lang="ts">
import { ref, computed } from "vue";
import { getNodes, type RevampNode } from "../../lib/revamp";
import Button from "../ui/Button.vue";

const props = defineProps<{
  showToast: (msg: string) => void;
  searchQuery: string;
}>();

const revampNodes = ref<RevampNode[]>([]);
const revampOnline = ref(false);
const revampLoading = ref(false);
const selectedNode = ref<string | null>(localStorage.getItem("revamp_node_selected") || null);

const filteredRevampNodes = computed(() => {
  const q = props.searchQuery.toLowerCase();
  if (!q) return revampNodes.value;
  return revampNodes.value.filter(n =>
    n.name.toLowerCase().includes(q) ||
    n.ip.toLowerCase().includes(q) ||
    `${n.ip}:${n.port}`.includes(q) ||
    (n.contributor || "").toLowerCase().includes(q)
  );
});

async function fetchRevampNodes() {
  revampLoading.value = true;
  try {
    revampOnline.value = true;
    revampNodes.value = await getNodes();
  } catch {
    revampOnline.value = false;
    revampNodes.value = [];
  } finally {
    revampLoading.value = false;
  }
}

function selectRevampNode(node: RevampNode) {
  selectedNode.value = `${node.ip}:${node.port}`;
  localStorage.setItem("revamp_node_selected", selectedNode.value);
}

defineExpose({ fetchRevampNodes });
</script>

<template>
  <div class="relay-list">
    <div v-if="revampLoading" class="loading-row">
      加载节点列表...
    </div>

    <div
      v-for="node in filteredRevampNodes"
      :key="node.ip + ':' + node.port"
      class="relay-card"
      :class="{ selected: selectedNode === node.ip + ':' + node.port }"
      @click="selectRevampNode(node)"
    >
      <div class="relay-card-left">
        <i class="bi bi-hdd-network"></i>
        <div>
          <span class="relay-name">{{ node.name || node.ip }}</span>
          <span class="relay-addr">
            {{ node.ip }}:{{ node.port }}
            <span v-if="node.contributor" class="relay-contributor">
              · 贡献者: {{ node.contributor }}
            </span>
          </span>
          <span v-if="node.last_update" class="relay-update">
            最后更新: {{ node.last_update }}
          </span>
        </div>
      </div>
      <i v-if="selectedNode === node.ip + ':' + node.port" class="bi bi-check-circle-fill check-icon"></i>
    </div>

    <div v-if="revampNodes.length === 0 && !revampLoading && revampOnline" class="empty-row">
      暂无可用节点
    </div>

    <div v-if="!revampOnline && !revampLoading" class="offline-row">
      <i class="bi bi-exclamation-triangle"></i>
      无法连接 revamp 中央服务器
      <Button variant="primary" @click="fetchRevampNodes">重试</Button>
    </div>

    <div v-if="filteredRevampNodes.length === 0 && revampNodes.length > 0 && searchQuery" class="empty-row">
      没有匹配的节点
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

.relay-contributor {
  color: var(--text-muted);
  opacity: 0.7;
  font-size: 11px;
  margin-left: 2px;
}

.relay-update {
  color: var(--text-muted);
  opacity: 0.5;
  font-size: 11px;
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

.loading-row, .empty-row {
  padding: 30px 0;
  text-align: center;
  color: var(--text-muted);
  font-size: 14px;
}

.offline-row {
  padding: 30px 0;
  text-align: center;
  color: var(--text-muted);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}

.offline-row i {
  font-size: 24px;
  opacity: 0.6;
}

.retry-btn {
  margin-top: 4px;
  padding: 6px 16px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.retry-btn:hover {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
}
</style>