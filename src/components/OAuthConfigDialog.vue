<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

interface OAuthProvider {
  name: string;
  client_id: string;
  client_secret: string;
  auth_url: string;
  token_url: string;
  userinfo_url: string;
  scopes: string;
}

defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  close: [];
  saved: [];
}>();

const config = ref<OAuthProvider>({
  name: "",
  client_id: "",
  client_secret: "",
  auth_url: "",
  token_url: "",
  userinfo_url: "",
  scopes: "openid profile email",
});

const saving = ref(false);

onMounted(async () => {
  try {
    const existing = await invoke<OAuthProvider | null>("get_oauth_config");
    if (existing) {
      config.value = existing;
    }
  } catch {
    // 使用默认值
  }
});

async function save() {
  saving.value = true;
  try {
    await invoke("save_oauth_config", { config: config.value });
    emit("saved");
    emit("close");
  } catch (e: any) {
    console.error("保存 OAuth 配置失败:", e);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="dialog-overlay" @click.self="emit('close')">
      <div class="dialog-card">
        <div class="dialog-header">
          <h3>OAuth2 单点登录配置</h3>
          <button class="dialog-close" @click="emit('close')">
            <i class="bi bi-x"></i>
          </button>
        </div>

        <div class="dialog-body">
          <p class="hint-text">配置你的 OAuth2 提供商信息，支持任何标准 OAuth2 协议的提供商。</p>

          <div class="form-group">
            <label>提供商名称</label>
            <input type="text" v-model="config.name" placeholder="例如：GitHub、自建" class="form-input" />
          </div>

          <div class="form-group">
            <label>Client ID</label>
            <input type="text" v-model="config.client_id" placeholder="客户端 ID" class="form-input" />
          </div>

          <div class="form-group">
            <label>Client Secret</label>
            <input type="password" v-model="config.client_secret" placeholder="客户端密钥" class="form-input" />
          </div>

          <div class="form-group">
            <label>授权端点 URL (Authorization Endpoint)</label>
            <input type="url" v-model="config.auth_url" placeholder="https://example.com/oauth/authorize" class="form-input" />
          </div>

          <div class="form-group">
            <label>Token 端点 URL</label>
            <input type="url" v-model="config.token_url" placeholder="https://example.com/oauth/token" class="form-input" />
          </div>

          <div class="form-group">
            <label>用户信息端点 URL (Userinfo Endpoint)</label>
            <input type="url" v-model="config.userinfo_url" placeholder="https://example.com/oauth/userinfo" class="form-input" />
          </div>

          <div class="form-group">
            <label>Scope（空格分隔）</label>
            <input type="text" v-model="config.scopes" placeholder="openid profile email" class="form-input" />
          </div>
        </div>

        <div class="dialog-actions">
          <button class="btn" @click="emit('close')">取消</button>
          <button class="btn btn-primary" :disabled="saving || !config.name || !config.client_id" @click="save">
            {{ saving ? '保存中...' : '保存' }}
          </button>
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
  width: 500px;
  max-height: 85vh;
  overflow-y: auto;
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.35);
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
  transition: all 0.15s ease;
}

.dialog-close:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.dialog-body {
  padding: 16px 24px 0;
}

.hint-text {
  font-size: 13px;
  color: var(--text-muted);
  margin: 0 0 16px;
  line-height: 1.5;
}

.form-group {
  margin-bottom: 14px;
}

.form-group label {
  display: block;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 5px;
}

.form-input {
  box-sizing: border-box;
  width: 100%;
  padding: 9px 12px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-size: 13px;
  transition: border-color 0.15s ease;
}

.form-input:focus {
  outline: none;
  border-color: var(--accent-primary);
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 20px 24px;
}

.btn {
  padding: 8px 20px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.btn:hover {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
}

.btn-primary {
  background: var(--accent-primary);
  border-color: var(--accent-primary);
  color: #fff;
}

.btn-primary:hover {
  background: var(--accent-hover);
}

.btn-primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
