<script setup lang="ts">
defineProps<{
  visible: boolean;
  roomName: string;
  members: { name: string; role: string }[];
}>();

const emit = defineEmits<{
  close: [];
}>();
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="dialog-overlay" @click.self="emit('close')">
      <div class="dialog-card dialog-card-small">
        <div class="dialog-header">
          <h3>队伍 <span class="dialog-subtitle">{{ roomName }}</span></h3>
          <button class="dialog-close" @click="emit('close')">
            <i class="bi bi-x"></i>
          </button>
        </div>
        <div class="dialog-section">
          <div v-if="members.length === 0" class="empty-list">暂无队伍成员</div>
          <div v-else class="team-list">
            <div
              v-for="(m, i) in members"
              :key="i"
              class="team-row"
              :class="{ 'team-leader': m.role === '队长' }"
            >
              <div class="team-avatar">
                <i :class="m.role === '队长' ? 'bi bi-crown-fill' : 'bi bi-person-fill'"></i>
              </div>
              <div class="team-info">
                <span class="team-name">{{ m.name }}</span>
                <span class="team-role-tag" :class="m.role === '队长' ? 'tag-leader' : 'tag-member'">{{ m.role }}</span>
              </div>
            </div>
          </div>
        </div>
        <div class="dialog-actions">
          <button class="btn" @click="emit('close')">关闭</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  backdrop-filter: blur(4px);
}

.dialog-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 16px;
  width: 460px;
  max-height: 85vh;
  overflow-y: auto;
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.35);
}

.dialog-card-small {
  width: 380px;
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 20px 24px 0;
}

.dialog-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-subtitle {
  font-size: 14px;
  font-weight: 400;
  color: var(--text-muted);
  margin-left: 8px;
}

.dialog-close {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  transition: background-color 0.2s ease, color 0.2s ease;
}

.dialog-close:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.dialog-section {
  padding: 16px 24px 0;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 20px 24px;
}

.empty-list {
  text-align: center;
  color: var(--text-muted);
  padding: 20px 0;
  font-size: 14px;
}

.team-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 240px;
  overflow-y: auto;
}

.team-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--bg-tertiary);
}

.team-leader {
  background: rgba(255, 193, 7, 0.08);
  border: 1px solid rgba(255, 193, 7, 0.15);
}

.team-avatar {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 15px;
  color: #fff;
  flex-shrink: 0;
}

.team-leader .team-avatar {
  background: #f59e0b;
}

.team-info {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 1;
  min-width: 0;
}

.team-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.team-role-tag {
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
</style>
