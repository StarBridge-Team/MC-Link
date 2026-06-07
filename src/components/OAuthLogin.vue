<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import OAuthConfigDialog from "./OAuthConfigDialog.vue";

interface OAuthUser {
  id: string;
  name: string;
  avatar_url: string | null;
  email: string | null;
  provider: string;
}

const props = defineProps<{
  showToast: (msg: string) => void;
}>();

const user = ref<OAuthUser | null>(null);
const loading = ref(false);
const showConfig = ref(false);

async function refreshStatus() {
  try {
    user.value = await invoke<OAuthUser | null>("get_oauth_user");
  } catch {
    user.value = null;
  }
}

onMounted(refreshStatus);

async function login() {
  loading.value = true;
  try {
    const result = await invoke<OAuthUser>("oauth_login");
    user.value = result;
    props.showToast(`登录成功: ${result.name}`);
  } catch (e: any) {
    props.showToast("登录失败: " + e);
  } finally {
    loading.value = false;
  }
}

async function logout() {
  try {
    await invoke("oauth_logout");
    user.value = null;
    props.showToast("已登出");
  } catch (e: any) {
    props.showToast("登出失败: " + e);
  }
}

function onConfigSaved() {
  props.showToast("OAuth 配置已保存");
}
</script>

<template>
  <div class="oauth-section">
    <template v-if="user">
      <div class="user-profile">
        <div class="user-avatar">
          <img v-if="user.avatar_url" :src="user.avatar_url" alt="" />
          <i v-else class="bi bi-person-circle"></i>
        </div>
        <div class="user-info">
          <div class="user-name">{{ user.name }}</div>
          <div class="user-provider">{{ user.provider }}</div>
        </div>
        <button class="icon-btn" title="登出" @click="logout">
          <i class="bi bi-box-arrow-right"></i>
        </button>
      </div>
    </template>
    <template v-else>
      <div class="login-section">
        <button class="login-btn" :disabled="loading" @click="login">
          <i class="bi bi-shield-lock"></i>
          <span>{{ loading ? '登录中...' : 'SSO 登录' }}</span>
        </button>
        <button class="icon-btn config-btn" title="OAuth 配置" @click="showConfig = true">
          <i class="bi bi-gear"></i>
        </button>
      </div>
    </template>

    <OAuthConfigDialog
      :visible="showConfig"
      @close="showConfig = false"
      @saved="onConfigSaved"
    />
  </div>
</template>

<style scoped>
.oauth-section {
  padding: 4px 0;
}

.user-profile {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
}

.user-avatar {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  overflow: hidden;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--accent-primary);
  color: #fff;
  font-size: 18px;
}

.user-avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.user-info {
  flex: 1;
  min-width: 0;
}

.user-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.user-provider {
  font-size: 10px;
  color: var(--text-muted);
}

.icon-btn {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  transition: all 0.15s ease;
  flex-shrink: 0;
}

.icon-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.login-section {
  display: flex;
  align-items: center;
  gap: 6px;
}

.login-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.login-btn:hover {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
  background: rgba(0, 102, 204, 0.06);
}

.login-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.login-btn i {
  font-size: 14px;
}

.config-btn {
  font-size: 13px;
}
</style>
