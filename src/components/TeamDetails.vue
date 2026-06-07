<script setup lang="ts">
import { ref } from "vue";
import TeamChat from "./TeamChat.vue";

defineProps<{
  teamId: string;
  teamName: string;
  host: string;
  memberCount: number;
  playerName: string;
  chatServerUrl?: string;
}>();

type Tab = "chat" | "manage" | "settings";
const activeTab = ref<Tab>("chat");
</script>

<template>
  <div class="team-details">
    <!-- 页面顶栏 -->
    <div class="details-topbar">
      <div class="topbar-tabs">
        <button
          :class="['tab-btn', { active: activeTab === 'chat' }]"
          @click="activeTab = 'chat'"
        >
          <i class="bi bi-chat-dots"></i>
          队伍聊天
        </button>
        <button
          :class="['tab-btn', { active: activeTab === 'manage' }]"
          @click="activeTab = 'manage'"
        >
          <i class="bi bi-people"></i>
          队伍管理
        </button>
        <button
          :class="['tab-btn', { active: activeTab === 'settings' }]"
          @click="activeTab = 'settings'"
        >
          <i class="bi bi-gear"></i>
          队伍设置
        </button>
      </div>
    </div>

    <!-- Tab 内容 -->
    <div class="details-content">
      <!-- 队伍聊天 -->
      <div v-if="activeTab === 'chat'" class="tab-pane">
        <TeamChat
          :team-id="teamId"
          :team-name="teamName"
          :player-name="playerName"
          :chat-server-url="chatServerUrl"
        />
      </div>

      <!-- 队伍管理 -->
      <div v-if="activeTab === 'manage'" class="tab-pane tab-pane-scroll">
        <div class="tab-section">
          <h3 class="section-title">成员列表</h3>
          <div class="member-card">
            <div class="member-avatar">
              <i class="bi bi-crown-fill"></i>
            </div>
            <div class="member-info">
              <span class="member-name">{{ host }}</span>
              <span class="member-role tag-leader">队长</span>
            </div>
          </div>
          <div class="member-card">
            <div class="member-avatar member-avatar-plain">
              <i class="bi bi-person-fill"></i>
            </div>
            <div class="member-info">
              <span class="member-name">{{ playerName }}</span>
              <span class="member-role tag-member">成员</span>
            </div>
          </div>
          <p class="tab-hint">共 {{ memberCount }} 人 · 队长可管理成员</p>
        </div>

        <div class="tab-section">
          <h3 class="section-title">邀请</h3>
          <div class="invite-box">
            <input type="text" class="invite-input" :value="teamId" readonly />
            <button class="btn btn-small">复制</button>
          </div>
          <p class="tab-hint">分享队伍ID给其他玩家加入</p>
        </div>
      </div>

      <!-- 队伍设置 -->
      <div v-if="activeTab === 'settings'" class="tab-pane tab-pane-scroll">
        <div class="tab-section">
          <h3 class="section-title">队伍名称</h3>
          <div class="input-group">
            <input type="text" class="dialog-input" :value="teamName" disabled />
          </div>
        </div>

        <div class="tab-section">
          <h3 class="section-title">聊天服务器</h3>
          <div class="input-group">
            <input type="text" class="dialog-input" :value="chatServerUrl || 'mk.aini2.cn:8878'" disabled />
          </div>
          <p class="tab-hint">连接到中央聊天服务器</p>
        </div>

        <div class="tab-section">
          <h3 class="section-title">危险操作</h3>
          <div class="danger-card">
            <p>离开队伍后需要重新邀请才能加入</p>
            <button class="btn btn-small btn-danger">离开队伍</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.team-details {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-secondary);
  border-radius: 12px;
  overflow: hidden;
}

/* ===== 顶栏 ===== */
.details-topbar {
  flex-shrink: 0;
  padding: 0 18px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
}

.topbar-tabs {
  display: flex;
  gap: 4px;
  padding: 10px 0;
}

.tab-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--text-muted);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.tab-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.tab-btn.active {
  background: var(--accent-primary);
  color: #fff;
}

.tab-btn i {
  font-size: 15px;
}

/* ===== 内容 ===== */
.details-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--bg-primary);
}

.tab-pane {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.tab-pane-scroll {
  overflow-y: auto;
  padding: 20px 24px;
  gap: 24px;
}

.tab-section {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
}

.tab-hint {
  font-size: 12px;
  color: var(--text-muted);
  margin: 4px 0 0;
}

/* ===== 成员卡片 ===== */
.member-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: 10px;
  background: var(--bg-tertiary);
}

.member-avatar {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: #f59e0b;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  color: #fff;
  flex-shrink: 0;
}

.member-avatar-plain {
  background: var(--accent-primary);
}

.member-info {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 1;
  min-width: 0;
}

.member-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.member-role {
  font-size: 10px;
  padding: 2px 10px;
  border-radius: 20px;
  font-weight: 500;
}

.tag-leader {
  background: rgba(255, 193, 7, 0.15);
  color: #f59e0b;
}

.tag-member {
  background: rgba(0, 102, 204, 0.12);
  color: var(--accent-primary);
}

/* ===== 邀请 ===== */
.invite-box {
  display: flex;
  align-items: center;
  gap: 8px;
}

.invite-input {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-size: 12px;
  font-family: monospace;
  letter-spacing: 0.5px;
}

/* ===== 按钮 ===== */
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 16px;
  border: none;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
}

.btn:hover {
  background: rgba(255, 255, 255, 0.15);
}

.btn-small {
  padding: 6px 12px;
  font-size: 12px;
}

.btn-danger {
  background: #ef4444;
  color: #fff;
}

.btn-danger:hover {
  background: #f87171;
}

/* ===== 输入 ===== */
.dialog-input {
  box-sizing: border-box;
  width: 100%;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-size: 14px;
}

.dialog-input:disabled {
  opacity: 0.6;
}

.input-group {
  display: flex;
  flex-direction: column;
}

/* ===== 危险操作 ===== */
.danger-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  border-radius: 10px;
  background: rgba(239, 68, 68, 0.08);
  border: 1px solid rgba(239, 68, 68, 0.15);
}

.danger-card p {
  margin: 0;
  font-size: 13px;
  color: var(--text-muted);
  line-height: 1.4;
}
</style>