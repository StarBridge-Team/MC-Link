<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import TabBar from "./common/TabBar.vue";
import McLinkRelay from "./relay/McLinkRelay.vue";
import RevampRelay from "./relay/RevampRelay.vue";

defineProps<{
  showToast: (msg: string) => void;
}>();

const activeTab = ref<"mclink" | "revamp">(
  (localStorage.getItem("relay_tab") as "mclink" | "revamp") || "mclink"
);
watch(activeTab, (val) => localStorage.setItem("relay_tab", val));

const searchQuery = ref("");

const mclinkRef = ref<InstanceType<typeof McLinkRelay> | null>(null);
const revampRef = ref<InstanceType<typeof RevampRelay> | null>(null);

const tabs = [
  { value: "mclink", label: "MC Link", icon: "bi-link-45deg" },
  { value: "revamp", label: "MC Link revamp", icon: "bi-arrow-repeat" },
];

onMounted(() => {
  mclinkRef.value?.loadRelays();
  revampRef.value?.fetchRevampNodes();
});
</script>

<template>
  <div>
    <TabBar :tabs="tabs" v-model:activeTab="activeTab" />

    <div class="relay-search-wrap">
      <i class="bi bi-search"></i>
      <input
        v-model="searchQuery"
        class="relay-search"
        :placeholder="activeTab === 'mclink' ? '搜索或输入节点地址' : '搜索节点名称、地址或贡献者'"
      />
    </div>

    <McLinkRelay
      v-show="activeTab === 'mclink'"
      ref="mclinkRef"
      :show-toast="showToast"
      :search-query="searchQuery"
    />

    <RevampRelay
      v-show="activeTab === 'revamp'"
      ref="revampRef"
      :show-toast="showToast"
      :search-query="searchQuery"
    />
  </div>
</template>

<style scoped>
.relay-search-wrap {
  position: relative;
  display: flex;
  align-items: center;
  background: rgba(255, 255, 255, 0.08);
  border-radius: 8px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  transition: all 0.2s ease;
}

.relay-search-wrap:focus-within {
  background: rgba(255, 255, 255, 0.12);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

.relay-search-wrap i:first-child {
  position: absolute;
  left: 12px;
  font-size: 14px;
  color: var(--text-muted);
  pointer-events: none;
  z-index: 1;
}

.relay-search {
  width: 100%;
  padding: 12px 14px 12px 34px;
  border: none;
  border-radius: 8px;
  font-size: 13px;
  box-sizing: border-box;
  background: transparent;
  color: var(--text-primary);
  outline: none;
}

.relay-search::placeholder {
  color: var(--text-muted);
}
</style>