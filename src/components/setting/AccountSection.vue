<script setup lang="ts">
import { ref, onUnmounted } from "vue";
import { openUrl } from '@tauri-apps/plugin-opener';
import { desktopLoginInit, desktopLoginPoll, accountGetMe, accountGetAvatar, accountVerify } from "../../lib/api/account";

const APP_ID = "dsk_b6b41d3dba4b9b3bf5ee3678c50890ae";

const props = defineProps<{
  showToast: (msg: string) => void;
  playerName?: string;
}>();

const emit = defineEmits<{
  'name-change': [val: string];
}>();

const TOKEN_KEY = "mc_link_account_token";
const USERNAME_KEY = "mc_link_account_username";
const AVATAR_KEY = "mc_link_account_avatar";

const savedToken = ref(localStorage.getItem(TOKEN_KEY) || "");
const savedUsername = ref(localStorage.getItem(USERNAME_KEY) || "");
const savedAvatar = ref(localStorage.getItem(AVATAR_KEY) || "");
const isLoggedIn = ref(!!savedToken.value && !!savedUsername.value);

const isLoggingIn = ref(false);
let pollTimer: ReturnType<typeof setInterval> | null = null;

if (isLoggedIn.value) {
  syncPlayerName();
}

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
});

function syncPlayerName() {
  const name = savedUsername.value;
  if (name) {
    localStorage.setItem("player_name", name);
    emit('name-change', name);
  }
}

function onNameInput(val: string) {
  const str = String(val);
  localStorage.setItem("player_name", str);
  emit('name-change', str);
}

async function handleLogin() {
  if (isLoggingIn.value) return;
  isLoggingIn.value = true;
  props.showToast("正在启动桌面登录...");

  try {
    // 1. 初始化桌面登录
    const init = await desktopLoginInit(APP_ID);
    if (init.code !== 0 || !init.session) {
      props.showToast(`登录初始化失败: ${init.error || '未知错误'}`);
      isLoggingIn.value = false;
      return;
    }

    const session = init.session;
    props.showToast("请在浏览器中完成登录授权");

    // 2. 打开浏览器授权页
    const loginUrl = `http://localhost:3000/login?app_id=${APP_ID}&session=${session}`;
    openUrl(loginUrl);

    // 3. 轮询等待授权结果
    pollTimer = setInterval(async () => {
      try {
        const poll = await desktopLoginPoll(APP_ID, session);
        if (poll.code === 0 && poll.status === "authorized" && poll.token) {
          clearInterval(pollTimer!);
          pollTimer = null;

          // 获取用户信息
          let username = "user";
          let avatar = "";
          try {
            const me = await accountGetMe(poll.token);
            if (me.code === 0 && me.user) {
              username = me.user.username || username;
              // 通过后端代理获取头像（避免跨域）
              if (me.user.avatar) {
                try {
                  avatar = await accountGetAvatar(poll.token, me.user.avatar);
                } catch (e: any) {
                  props.showToast(`头像加载失败: ${typeof e === 'string' ? e : e.message || '未知错误'}`);
                  avatar = "";
                }
              }
            } else {
              props.showToast(`获取用户信息失败: ${me.error || '未知错误'}`);
            }
          } catch (e: any) {
            props.showToast(`获取用户信息出错: ${typeof e === 'string' ? e : e.message || '未知错误'}`);
          }

          localStorage.setItem(TOKEN_KEY, poll.token);
          localStorage.setItem(USERNAME_KEY, username);
          localStorage.setItem(AVATAR_KEY, avatar);
          savedToken.value = poll.token;
          savedUsername.value = username;
          savedAvatar.value = avatar;
          isLoggedIn.value = true;
          isLoggingIn.value = false;
          syncPlayerName();
          props.showToast(`登录成功，欢迎 ${username}`);
        } else if (poll.code !== 0) {
          clearInterval(pollTimer!);
          pollTimer = null;
          isLoggingIn.value = false;
          props.showToast(`登录失败: ${poll.error}`);
        }
        // status === "pending" 则继续轮询
      } catch (e: any) {
        // 网络错误时静默重试
      }
    }, 2000);
  } catch (e: any) {
    isLoggingIn.value = false;
    props.showToast(`登录失败: ${typeof e === 'string' ? e : e.toString()}`);
  }
}

function handleLogout() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(USERNAME_KEY);
  localStorage.removeItem(AVATAR_KEY);
  savedToken.value = "";
  savedUsername.value = "";
  savedAvatar.value = "";
  isLoggedIn.value = false;
  isLoggingIn.value = false;
  props.showToast("已退出登录");
}

async function checkLoginStatus() {
  const tok = localStorage.getItem(TOKEN_KEY);
  const usr = localStorage.getItem(USERNAME_KEY);
  if (tok && usr) {
    try {
      const data = await accountVerify(tok);
      if (data.status === "success") {
        savedToken.value = tok;
        savedUsername.value = usr;
        isLoggedIn.value = true;
      } else {
        localStorage.removeItem(TOKEN_KEY);
        localStorage.removeItem(USERNAME_KEY);
      }
    } catch {
      localStorage.removeItem(TOKEN_KEY);
      localStorage.removeItem(USERNAME_KEY);
    }
  }
}

function handleRegister() {
  openUrl(`http://localhost:3000/login`);
}

defineExpose({ checkLoginStatus });
</script>

<template>
  <div class="account-page">
    <el-card v-if="!isLoggedIn" class="account-card" shadow="never">
      <div class="section-title">玩家信息</div>
      <el-form-item label="玩家名" class="account-field">
        <el-input
          :model-value="playerName"
          placeholder="输入玩家名"
          maxlength="16"
          @update:model-value="onNameInput"
        />
      </el-form-item>
    </el-card>

    <el-card class="account-card" shadow="never">
      <div class="section-title">MC Link 账号</div>

      <div v-if="isLoggedIn" class="logged-in">
        <div class="login-info">
          <div class="login-icon">
            <img v-if="savedAvatar" :src="savedAvatar" class="login-avatar" />
            <i v-else class="bi bi-person-check-fill"></i>
          </div>
          <div class="login-detail">
            <div class="login-username">{{ savedUsername }}</div>
            <div class="login-status">已登录 · 流畅联机</div>
          </div>
        </div>
        <el-button @click="handleLogout">退出登录</el-button>
      </div>

      <div v-else class="login-form">
        <el-alert
          type="info"
          :closable="false"
          class="login-tip"
        >
          <template #title>
            <span class="tip-text">
              <i class="bi bi-info-circle"></i>
              登录后可获得更流畅的联机体验，还可使用 MC Link Revamp
            </span>
          </template>
        </el-alert>
        <div class="login-actions">
          <el-button type="primary" :loading="isLoggingIn" @click="handleLogin">
            {{ isLoggingIn ? '请在浏览器中授权...' : '扫码登录' }}
          </el-button>
          <el-button @click="handleRegister">注册</el-button>
        </div>
      </div>
    </el-card>
  </div>
</template>

<style scoped>
.account-page {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
  overflow-y: auto;
  min-height: 0;
}

.account-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--r-lg);
}

.account-card :deep(.el-card__body) {
  padding: var(--sp-5);
}

.section-title {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  margin-bottom: var(--sp-3);
}

.account-field {
  margin-bottom: 0;
}

.login-form {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.login-tip {
  background: var(--bg-soft);
  border: 1px solid var(--border-color);
}

.tip-text {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  color: var(--text-muted);
  font-size: var(--fs-sm);
}

.login-actions {
  display: flex;
  gap: var(--sp-3);
}

.logged-in {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-3);
}
.login-info {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  min-width: 0;
}
.login-icon {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: var(--status-success-bg);
  color: var(--status-success);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--fs-2xl);
  flex-shrink: 0;
  overflow: hidden;
}
.login-avatar {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.login-detail {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.login-username {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}
.login-status {
  font-size: var(--fs-sm);
  color: var(--status-success);
}
</style>
