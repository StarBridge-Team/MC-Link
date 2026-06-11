<script setup lang="ts">
import { ref, computed } from "vue";
import Dialog from "./common/Dialog.vue";
import Input from "./ui/Input.vue";

const props = defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
}>();

const emit = defineEmits<{
  nameChange: [name: string];
}>();

const greeting = computed(() => {
  const h = new Date().getHours();
  if (h >= 6 && h < 12) return "早上好";
  if (h >= 12 && h < 14) return "中午好";
  if (h >= 14 && h < 18) return "下午好";
  return "晚上好";
});

const showAccountDialog = ref(false);
const editingName = ref("");
const showPlayerListDialog = ref(false);
const playerList = ref<{ name: string; role: string }[]>([]);

function openAccountDialog() {
  editingName.value = props.playerName;
  showAccountDialog.value = true;
}

function saveAccount() {
  const name = editingName.value.trim();
  if (!name) {
    props.showToast("名字不能为空");
    return;
  }
  localStorage.setItem("player_name", name);
  emit("nameChange", name);
  showAccountDialog.value = false;
  props.showToast(`已设置名字: ${name}`);
}
</script>

<template>
  <div class="home-root">
    <div class="greeting-header">
      <div class="greeting-avatar" @click="openAccountDialog">
        <i class="bi bi-person-circle"></i>
      </div>
      <div class="greeting-text">
        <span>{{ greeting }}</span>，
        <span class="greeting-name">{{ playerName }}</span>，
        <span>我们要做点什么？</span>
      </div>
    </div>

    <div class="home-hint">
      <div class="hint-section">
        <div class="hint-icon"><i class="bi bi-hdd-stack"></i></div>
        <div class="hint-content">
          <strong>房主</strong>：请先在 Minecraft 中开启局域网联机，然后前往<em>联机</em>页面创建房间
        </div>
      </div>
      <div class="hint-section">
        <div class="hint-icon"><i class="bi bi-person-plus"></i></div>
        <div class="hint-content">
          <strong>成员</strong>：前往<em>联机</em>页面，输入房主分享的房间名和密码即可加入
        </div>
      </div>
      <div class="hint-section">
        <div class="hint-icon"><i class="bi bi-plug"></i></div>
        <div class="hint-content">
          <strong>适配器</strong>：在<em>适配器</em>页面下载陶瓦联机以获得更多联机方式
        </div>
      </div>
    </div>

    <Dialog :show="showAccountDialog" title="账号" small @close="showAccountDialog = false">
      <label class="dialog-label">你的名字</label>
      <Input type="text" v-model="editingName" placeholder="输入你的名字" maxlength="16" @keyup.enter="saveAccount" />
      <p class="dialog-desc">此名字会展示给其他玩家，仅 MC Link 联机时可见</p>
      <template #actions>
        <Button @click="showAccountDialog = false">取消</Button>
        <Button variant="primary" @click="saveAccount">保存</Button>
      </template>
    </Dialog>

    <Dialog :show="showPlayerListDialog" title="队伍" small @close="showPlayerListDialog = false">
      <div v-if="playerList.length === 0" class="empty-list">暂无在线玩家</div>
      <div v-else class="player-list">
        <div v-for="(p, i) in playerList" :key="i" class="player-item">
          <div class="player-avatar">
            <i class="bi bi-person-fill"></i>
          </div>
          <div class="player-info">
            <span class="player-name">{{ p.name }}</span>
            <span class="player-role">{{ p.role === "host" ? "房主" : "成员" }}</span>
          </div>
        </div>
      </div>
      <template #actions>
        <Button @click="showPlayerListDialog = false">关闭</Button>
      </template>
    </Dialog>
  </div>
</template>

<style scoped>
.home-root {
  height: 100%;
  display: flex;
  flex-direction: column;
}

.greeting-header {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 0 0 16px;
  margin-bottom: 4px;
}

.greeting-avatar {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 26px;
  color: #fff;
  cursor: pointer;
  flex-shrink: 0;
  transition: opacity 0.15s ease;
}

.greeting-avatar:hover {
  opacity: 0.85;
}

.greeting-text {
  font-size: 16px;
  font-weight: 500;
  color: var(--text-primary);
  line-height: 1.4;
}

.greeting-name {
  color: var(--accent-primary);
  font-weight: 600;
}

.home-hint {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.hint-section {
  display: flex;
  align-items: flex-start;
  gap: 14px;
  padding: 14px 16px;
  border-radius: 10px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
}

.hint-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  color: #fff;
  flex-shrink: 0;
  margin-top: 2px;
}

.hint-content {
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-primary);
}

.hint-content strong {
  color: var(--text-primary);
  font-weight: 600;
}

.hint-content em {
  font-style: normal;
  color: var(--accent-primary);
  font-weight: 500;
}

/* Dialog scoped styles for content */
.dialog-label {
  display: block;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.dialog-desc {
  font-size: 11.5px;
  color: var(--text-muted);
  margin: 8px 0 0;
  line-height: 1.4;
}

.player-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 240px;
  overflow-y: auto;
}

.player-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 8px;
  background: var(--bg-tertiary);
}

.player-avatar {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  color: #fff;
  flex-shrink: 0;
}

.player-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.player-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.player-role {
  font-size: 11px;
  color: var(--text-muted);
}

.empty-list {
  text-align: center;
  color: var(--text-muted);
  padding: 20px 0;
  font-size: 14px;
}
</style>