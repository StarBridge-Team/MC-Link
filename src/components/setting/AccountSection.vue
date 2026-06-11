<script setup lang="ts">
import { ref } from "vue";
import { registerAccount, loginAccount } from "../../lib/revamp";
import Input from "../ui/Input.vue";
import Button from "../ui/Button.vue";
import Card from "../ui/Card.vue";
import Spinner from "../ui/Spinner.vue";

const props = defineProps<{
  showToast: (msg: string) => void;
  playerName?: string;
}>();

const emit = defineEmits<{
  'name-change': [val: string];
}>();

const revampServer = "43.248.79.27:14117";
const revampUsername = ref("");
const revampPassword = ref("");
const revampToken = ref(localStorage.getItem("revamp_token") || "");
const revampLoggedIn = ref(!!localStorage.getItem("revamp_token"));
const revampLoading = ref(false);
const revampActionText = ref("");

const savedRevampUser = localStorage.getItem("revamp_username") || "";
if (savedRevampUser) revampUsername.value = savedRevampUser;

function onNameInput(val: string | number) {
  const str = String(val);
  localStorage.setItem("player_name", str);
  emit('name-change', str);
}

async function revampRegister() {
  if (!revampUsername.value || !revampPassword.value) {
    props.showToast("请输入用户名和密码");
    return;
  }
  revampLoading.value = true;
  revampActionText.value = "注册中...";
  try {
    const data = await registerAccount(revampUsername.value, revampPassword.value);
    if (data.status === "ok" || data.status === "success") {
      props.showToast("注册成功，请登录");
    } else {
      props.showToast("注册失败: " + (data.message || "未知错误"));
    }
  } catch (e: any) {
    props.showToast("注册失败: " + e);
  } finally {
    revampLoading.value = false;
    revampActionText.value = "";
  }
}

async function revampLogin() {
  if (!revampUsername.value || !revampPassword.value) {
    props.showToast("请输入用户名和密码");
    return;
  }
  revampLoading.value = true;
  revampActionText.value = "登录中...";
  try {
    const data = await loginAccount(revampUsername.value, revampPassword.value);
    if (data.status === "ok" || data.status === "success") {
      revampToken.value = data.token;
      revampLoggedIn.value = true;
      localStorage.setItem("revamp_token", data.token);
      localStorage.setItem("revamp_username", revampUsername.value);
      props.showToast("登录成功");
    } else {
      props.showToast("登录失败: " + (data.message || "用户名或密码错误"));
    }
  } catch (e: any) {
    props.showToast("登录失败: " + e);
  } finally {
    revampLoading.value = false;
    revampActionText.value = "";
  }
}

function revampLogout() {
  revampToken.value = "";
  revampLoggedIn.value = false;
  localStorage.removeItem("revamp_token");
  props.showToast("已登出");
}
</script>

<template>
  <div class="personalization-page">
    <Card>
      <div class="pers-card-title">玩家信息</div>
      <div class="pers-row">
        <span>玩家名</span>
        <Input :model-value="playerName" placeholder="输入玩家名" maxlength="16" @input="onNameInput" />
      </div>
    </Card>

    <Card style="margin-top: 16px; position: relative;">
      <div class="pers-card-title">
        MC Link revamp 适配器账号
        <span class="revamp-badge">适配器</span>
      </div>
      <div class="revamp-server-info">
        <i class="bi bi-server"></i>
        服务器: <code>{{ revampServer }}</code>
      </div>
      <div class="pers-row">
        <span>状态</span>
        <span :class="revampLoggedIn ? 'revamp-status-online' : 'revamp-status-offline'">
          <i :class="revampLoggedIn ? 'bi bi-check-circle-fill' : 'bi bi-x-circle-fill'"></i>
          {{ revampLoggedIn ? '已登录' : '未登录' }}
        </span>
      </div>

      <div v-if="revampLoading" class="revamp-loading-overlay">
        <Spinner :size="36" />
        <span>{{ revampActionText }}</span>
      </div>

      <template v-if="!revampLoggedIn">
        <div class="pers-row">
          <span>用户名</span>
          <Input
            v-model="revampUsername"
            placeholder="输入用户名"
            maxlength="32"
          />
        </div>
        <div class="pers-row">
          <span>密码</span>
          <Input
            type="password"
            v-model="revampPassword"
            placeholder="输入密码"
            @keyup.enter="revampLogin"
          />
        </div>
        <div class="revamp-actions">
          <Button variant="primary" :disabled="revampLoading" @click="revampRegister">
            <i class="bi bi-person-plus"></i>
            注册
          </Button>
          <Button variant="primary" :disabled="revampLoading" @click="revampLogin">
            <i class="bi bi-box-arrow-in-right"></i>
            登录
          </Button>
        </div>
      </template>
      <template v-else>
        <div class="pers-row">
          <span>已登录用户</span>
          <span class="revamp-user">{{ revampUsername }}</span>
        </div>
        <div class="revamp-actions">
          <Button @click="revampLogout">
            <i class="bi bi-box-arrow-right"></i>
            登出
          </Button>
        </div>
      </template>
    </Card>
  </div>
</template>

<style scoped>
.personalization-page {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow-y: auto;
}

.pers-card {
  background: var(--bg-card, rgba(255,255,255,0.06));
  border: 1px solid var(--border-color, rgba(255,255,255,0.08));
  border-radius: 12px;
  padding: 20px 24px;
  box-shadow: 0 2px 12px rgba(0,0,0,0.1);
}

.pers-card-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 16px;
}

.pers-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 0;
  font-size: 14px;
  color: var(--text-primary);
}

.pers-row + .pers-row {
  border-top: 1px solid var(--border-color);
}

.revamp-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  background: var(--accent-primary);
  color: #fff;
  vertical-align: middle;
  margin-left: 8px;
}

.revamp-server-info {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-muted);
  margin-bottom: 8px;
  padding: 6px 0;
}

.revamp-server-info code {
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
  background: rgba(255,255,255,0.06);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
}

.revamp-status-online {
  color: #10b981;
  font-size: 14px;
  font-weight: 500;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.revamp-status-offline {
  color: var(--text-muted);
  font-size: 14px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.revamp-user {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
}

.revamp-actions {
  display: flex;
  gap: 8px;
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-color);
}

.revamp-loading-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  z-index: 10;
  backdrop-filter: blur(2px);
}

.revamp-loading-overlay span {
  font-size: 14px;
  color: #fff;
  font-weight: 500;
}
</style>