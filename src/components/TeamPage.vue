<script setup lang="ts">
import { ref, computed } from "vue";
import TeamDetails from "./TeamDetails.vue";

withDefaults(defineProps<{
  showToast?: (msg: string) => void;
  playerName?: string;
}>(), {
  showToast: () => {},
  playerName: "玩家",
});

interface TeamInfo {
  name: string;
  host: string;
  memberCount: number;
  roomName: string;
  teamId: string;
}

const searchQuery = ref("");
const teams = ref<TeamInfo[]>([]);
const selectedTeam = ref<TeamInfo | null>(null);

const filteredTeams = computed(() => {
  if (!searchQuery.value.trim()) return teams.value;
  const q = searchQuery.value.toLowerCase();
  return teams.value.filter(t =>
    t.name.toLowerCase().includes(q) || t.host.toLowerCase().includes(q)
  );
});

function selectTeam(team: TeamInfo) {
  selectedTeam.value = team;
}
</script>

<template>
  <div class="team-layout">
    <div class="team-sidebar">
      <div class="sidebar-search">
        <i class="bi bi-search"></i>
        <input
          type="text"
          v-model="searchQuery"
          placeholder="搜索队伍..."
          class="search-input"
        />
      </div>

      <div class="sidebar-list">
        <div v-if="filteredTeams.length === 0" class="sidebar-empty">
          {{ searchQuery ? '未找到匹配的队伍' : '暂无队伍' }}
        </div>
        <div
          v-for="(team, i) in filteredTeams"
          :key="i"
          :class="['sidebar-item', { active: selectedTeam === team }]"
          @click="selectTeam(team)"
        >
          <div class="sidebar-item-icon">
            <i class="bi bi-people-fill"></i>
          </div>
          <div class="sidebar-item-body">
            <div class="sidebar-item-name">{{ team.name }}</div>
            <div class="sidebar-item-meta">
              <span>{{ team.host }}</span>
              <span class="sidebar-item-count">{{ team.memberCount }} 人</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="team-content">
      <TeamDetails
        v-if="selectedTeam"
        :team-id="selectedTeam.teamId"
        :team-name="selectedTeam.name"
        :host="selectedTeam.host"
        :member-count="selectedTeam.memberCount"
        :player-name="playerName"
      />
      <div v-else class="content-empty">
        <i class="bi bi-flag-fill"></i>
        <p>选择一个队伍查看详情</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.team-layout {
  display: flex;
  height: 100%;
  gap: 24px;
}

.team-sidebar {
  width: 200px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
}

.sidebar-search {
  position: relative;
  margin-bottom: 12px;
}

.sidebar-search i {
  position: absolute;
  left: 12px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text-muted);
  font-size: 13px;
}

.search-input {
  box-sizing: border-box;
  width: 100%;
  padding: 8px 12px 8px 34px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-size: 13px;
  transition: border-color 0.2s ease;
}

.search-input:focus {
  outline: none;
  border-color: var(--accent-primary);
}

.sidebar-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.sidebar-empty {
  text-align: center;
  color: var(--text-muted);
  font-size: 13px;
  padding: 24px 0;
}

.sidebar-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 8px;
  cursor: pointer;
  transition: background-color 0.2s ease;
}

.sidebar-item:hover {
  background: var(--bg-hover);
}

.sidebar-item.active {
  background: rgba(0, 102, 204, 0.1);
}

.sidebar-item-icon {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  color: #fff;
  flex-shrink: 0;
}

.sidebar-item-body {
  flex: 1;
  min-width: 0;
}

.sidebar-item-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sidebar-item-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--text-muted);
}

.sidebar-item-count {
  margin-left: auto;
}

.team-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.content-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-muted);
}

.content-empty i {
  font-size: 48px;
  opacity: 0.3;
}

.content-empty p {
  font-size: 14px;
  margin: 0;
}
</style>