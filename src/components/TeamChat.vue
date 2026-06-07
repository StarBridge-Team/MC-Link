<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from "vue";

const props = withDefaults(defineProps<{
  teamId: string;
  teamName: string;
  playerName: string;
  chatServerUrl?: string;
}>(), {
  chatServerUrl: "mk.aini2.cn:8878",
});

interface ChatMessage {
  sender: string;
  text: string;
  time: number;
  isSelf: boolean;
}

const messages = ref<ChatMessage[]>([]);
const inputText = ref("");
const ws = ref<WebSocket | null>(null);
const connected = ref(false);
const messageList = ref<HTMLElement | null>(null);
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let connecting = false;

const displayName = computed(() => {
  return props.teamName || "队伍聊天";
});

function connect() {
  if (connecting) return;
  if (ws.value) {
    ws.value.close();
    ws.value = null;
  }

  connecting = true;
  const url = `ws://${props.chatServerUrl}`;
  const socket = new WebSocket(url);

  socket.onopen = () => {
    connecting = false;
    connected.value = true;
    socket.send(JSON.stringify({
      type: "join",
      team_id: props.teamId,
      sender: props.playerName,
    }));
  };

  socket.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data);
      if (data.type === "chat" && data.team_id === props.teamId) {
        messages.value.push({
          sender: data.sender,
          text: data.text,
          time: data.time,
          isSelf: data.sender === props.playerName,
        });
        scrollToBottom();
      }
    } catch {
      // ignore
    }
  };

  socket.onclose = () => {
    connecting = false;
    connected.value = false;
    ws.value = null;
    clearReconnectTimer();
    reconnectTimer = setTimeout(() => {
      if (!ws.value) connect();
    }, 5000);
  };

  socket.onerror = () => {
    connecting = false;
    socket.close();
  };

  ws.value = socket;
}

function clearReconnectTimer() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
}

function sendMessage() {
  const text = inputText.value.trim();
  if (!text || !ws.value || ws.value.readyState !== WebSocket.OPEN) return;

  ws.value.send(JSON.stringify({
    type: "chat",
    team_id: props.teamId,
    sender: props.playerName,
    text,
    time: Math.floor(Date.now() / 1000),
  }));

  messages.value.push({
    sender: props.playerName,
    text,
    time: Math.floor(Date.now() / 1000),
    isSelf: true,
  });

  inputText.value = "";
  scrollToBottom();
}

function scrollToBottom() {
  nextTick(() => {
    if (messageList.value) {
      messageList.value.scrollTop = messageList.value.scrollHeight;
    }
  });
}

function formatTime(timestamp: number): string {
  const d = new Date(timestamp * 1000);
  return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}

watch(() => props.teamId, () => {
  clearReconnectTimer();
  if (ws.value) ws.value.close();
  connecting = false;
  connect();
});

watch(() => props.chatServerUrl, () => {
  clearReconnectTimer();
  if (ws.value) ws.value.close();
  connecting = false;
  connect();
});

onMounted(() => {
  connect();
});

onUnmounted(() => {
  clearReconnectTimer();
  connecting = false;
  if (ws.value) {
    if (ws.value.readyState === WebSocket.OPEN) {
      ws.value.send(JSON.stringify({
        type: "leave",
        team_id: props.teamId,
      }));
    }
    ws.value.close();
    ws.value = null;
  }
});
</script>

<template>
  <div class="team-chat">
    <div class="chat-header">
      <div class="header-left">
        <i class="bi bi-chat-dots-fill"></i>
        <span class="header-title">{{ displayName }}</span>
        <span :class="['status-dot', { online: connected }]"></span>
      </div>
      <div class="header-icons">
        <i class="bi bi-bell-fill header-icon" title="通知"></i>
        <i class="bi bi-search header-icon" title="搜索"></i>
        <i class="bi bi-three-dots-vertical header-icon" title="更多"></i>
      </div>
    </div>

    <div ref="messageList" class="chat-messages">
      <div v-if="messages.length === 0" class="chat-empty">
        <i class="bi bi-chat-dots"></i>
        <p>暂无消息，发送第一条消息吧</p>
      </div>
      <div
        v-for="(msg, i) in messages"
        :key="i"
        :class="['message-row', { 'message-self': msg.isSelf }]"
      >
        <div class="message-bubble">
          <div v-if="!msg.isSelf" class="message-sender">{{ msg.sender }}</div>
          <div class="message-text">{{ msg.text }}</div>
          <div class="message-time">{{ formatTime(msg.time) }}</div>
        </div>
      </div>
    </div>

    <div class="chat-input-area">
      <div class="input-toolbar">
        <button class="toolbar-btn" title="图片/视频">
          <i class="bi bi-image"></i>
        </button>
        <button class="toolbar-btn" title="表情">
          <i class="bi bi-emoji-smile"></i>
        </button>
        <button class="toolbar-btn" title="语音">
          <i class="bi bi-mic"></i>
        </button>
      </div>
      <div class="input-wrapper">
        <input
          type="text"
          v-model="inputText"
          placeholder="输入消息..."
          class="chat-input"
          @keyup.enter="sendMessage"
          maxlength="500"
        />
        <button
          :class="['send-btn', { active: inputText.trim().length > 0 }]"
          @click="sendMessage"
          :disabled="!inputText.trim()"
        >
          <i class="bi bi-send-fill"></i>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.team-chat {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-primary);
  border-radius: 12px;
  overflow: hidden;
}

.chat-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.header-left i {
  font-size: 18px;
  color: var(--accent-primary);
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #ef4444;
  transition: background 0.3s ease;
}

.status-dot.online {
  background: #22c55e;
  box-shadow: 0 0 6px rgba(34, 197, 94, 0.5);
}

.header-icons {
  display: flex;
  align-items: center;
  gap: 6px;
}

.header-icon {
  width: 34px;
  height: 34px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  color: var(--text-muted);
  font-size: 16px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.header-icon:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  background: var(--bg-primary);
}

.chat-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  gap: 10px;
  opacity: 0.6;
}

.chat-empty i {
  font-size: 48px;
}

.chat-empty p {
  font-size: 13px;
  margin: 0;
}

.message-row {
  display: flex;
  flex-direction: column;
  max-width: 75%;
}

.message-self {
  align-self: flex-end;
}

.message-row:not(.message-self) {
  align-self: flex-start;
}

.message-bubble {
  padding: 10px 14px;
  border-radius: 14px;
  background: var(--bg-tertiary);
  position: relative;
  word-break: break-word;
}

.message-self .message-bubble {
  background: var(--accent-primary);
  color: #fff;
  border-bottom-right-radius: 4px;
}

.message-row:not(.message-self) .message-bubble {
  border-bottom-left-radius: 4px;
}

.message-sender {
  font-size: 11px;
  font-weight: 600;
  color: var(--accent-primary);
  margin-bottom: 4px;
}

.message-self .message-sender {
  color: rgba(255, 255, 255, 0.85);
}

.message-text {
  font-size: 14px;
  line-height: 1.5;
  color: var(--text-primary);
}

.message-self .message-text {
  color: #fff;
}

.message-time {
  font-size: 10px;
  color: var(--text-muted);
  margin-top: 4px;
  text-align: right;
  opacity: 0.7;
}

.message-self .message-time {
  color: rgba(255, 255, 255, 0.7);
}

.chat-input-area {
  flex-shrink: 0;
  padding: 10px 18px 14px;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.input-toolbar {
  display: flex;
  align-items: center;
  gap: 4px;
}

.toolbar-btn {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-muted);
  font-size: 16px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.toolbar-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.input-wrapper {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 4px 4px 4px 14px;
  transition: border-color 0.2s ease;
}

.input-wrapper:focus-within {
  border-color: var(--accent-primary);
}

.chat-input {
  flex: 1;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 14px;
  padding: 8px 0;
  min-height: 20px;
}

.chat-input::placeholder {
  color: var(--text-muted);
}

.send-btn {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 8px;
  background: var(--bg-tertiary);
  color: var(--text-muted);
  font-size: 16px;
  cursor: pointer;
  transition: all 0.2s ease;
  flex-shrink: 0;
}

.send-btn.active {
  background: var(--accent-primary);
  color: #fff;
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.send-btn:not(:disabled):hover {
  background: var(--accent-hover);
  color: #fff;
}
</style>