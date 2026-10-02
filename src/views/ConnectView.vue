<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { useConnect } from "../composables/useConnect";
import { showSuccess } from "../composables/useToast";
import type { JoinField } from "../lib/api/types";

/**
 * 联机页。
 *
 * 路径最短：房主点「创建房间」即建（无需填写）；访客点分享链接经 `mclink://join/<code>`
 * 深链自动跳到本页并预填房间码，点「加入」即可。加入表单由适配器自声明的
 * `join_fields` 动态渲染——换适配器无需改界面。
 */
const { t } = useI18n();
const {
  adapters,
  selectedId,
  joinFields,
  form,
  mode,
  role,
  roomCode,
  errorMsg,
  busy,
  showJoin,
  currentGameName,
  canJoin,
  mount,
  unmount,
  selectAdapter,
  startHost,
  join,
  stop,
  applyInvite,
  buildShareLink,
} = useConnect();

const inviteText = ref("");

let unlistenDeep: (() => void) | null = null;

onMounted(async () => {
  await mount();
  // 深链 `mclink://join/<code>`：Rust 已把 action/params 发到前端，这里直接预填。
  unlistenDeep = await listen<{ action: string; params: string[] }>("deep-link", (e) => {
    if (e.payload.action === "join" && e.payload.params[0]) {
      applyInvite(e.payload.params[0]);
    }
  });
});

onUnmounted(() => {
  unmount();
  unlistenDeep?.();
});

const TRUST: Record<string, { label: string; cls: string }> = {
  official: { label: t("connect.trust.official"), cls: "trust--official" },
  verified: { label: t("connect.trust.verified"), cls: "trust--verified" },
  unsigned: { label: t("connect.trust.unsigned"), cls: "trust--unsigned" },
  blocked: { label: t("connect.trust.blocked"), cls: "trust--blocked" },
};

function trustInfo(trust: string) {
  return TRUST[trust] ?? TRUST.unsigned;
}

function fieldLabel(f: JoinField): string {
  if (f.key === "room_code") return t("connect.field.roomCode");
  return f.label ?? f.key;
}

function pasteInvite() {
  if (inviteText.value.trim()) applyInvite(inviteText.value);
}

async function copyCode() {
  try {
    await navigator.clipboard.writeText(roomCode.value);
    showSuccess(t("connect.copied"));
  } catch {
    /* 剪贴板不可用时忽略 */
  }
}

async function share() {
  try {
    await navigator.clipboard.writeText(buildShareLink());
    showSuccess(t("connect.linkCopied"));
  } catch {
    /* 忽略 */
  }
}
</script>

<template>
  <div class="connect">
    <!-- 当前游戏：从已选画像自动带入，零输入 -->
    <div class="connect__game">
      <m3e-icon name="sports_esports" />
      <span>{{ t("connect.currentGame") }}：{{ currentGameName }}</span>
    </div>

    <!-- 无可用适配器 -->
    <div v-if="adapters.length === 0 && mode === 'idle'" class="card connect__empty">
      <i class="material-symbols-rounded">hub</i>
      <p>{{ t("connect.noAdapters") }}</p>
    </div>

    <!-- 空闲态：两个主操作 + 动态加入表单 -->
    <template v-else-if="mode === 'idle'">
      <div v-if="!showJoin" class="connect__actions">
        <m3e-button variant="filled" class="action action--primary" @click="startHost">
          <m3e-icon slot="icon" name="add" />
          {{ t("connect.createRoom") }}
        </m3e-button>
        <m3e-button variant="outlined" class="action" @click="showJoin = true">
          <m3e-icon slot="icon" name="login" />
          {{ t("connect.joinRoom") }}
        </m3e-button>
      </div>

      <!-- 加入表单：由适配器 join_fields 动态生成 -->
      <div v-else class="card connect__join">
        <h3 class="card__title">{{ t("connect.joinFormTitle") }}</h3>
        <m3e-form-field
          v-for="f in joinFields"
          :key="f.key"
          variant="outlined"
          class="field"
        >
          <label slot="label">{{ fieldLabel(f) }}</label>
          <input
            slot="input"
            v-model="form[f.key]"
            :type="f.type === 'password' ? 'password' : 'text'"
            :placeholder="f.placeholder"
          />
        </m3e-form-field>
        <div class="connect__join-actions">
          <m3e-button
            variant="filled"
            :disabled="!canJoin || busy"
            @click="join"
          >
            {{ t("connect.joinRoom") }}
          </m3e-button>
          <m3e-button variant="text" @click="showJoin = false">
            {{ t("common.cancel") }}
          </m3e-button>
        </div>
      </div>

      <!-- 粘贴邀请链接自动识别房间码 -->
      <div class="card connect__invite">
        <m3e-form-field variant="outlined" class="field">
          <label slot="label">{{ t("connect.pasteInvite") }}</label>
          <input
            slot="input"
            v-model="inviteText"
            type="text"
            :placeholder="t('connect.pasteInvitePlaceholder')"
            @keyup.enter="pasteInvite"
          />
        </m3e-form-field>
        <m3e-button variant="tonal" @click="pasteInvite">
          {{ t("connect.recognize") }}
        </m3e-button>
      </div>

      <!-- 可用适配器（可点选，默认首个 ready） -->
      <details class="card connect__adapters">
        <summary>{{ t("connect.adapters") }}</summary>
        <ul>
          <li
            v-for="a in adapters"
            :key="a.pluginId"
            :class="['adapter', { 'adapter--active': a.pluginId === selectedId }]"
            @click="selectAdapter(a.pluginId)"
          >
            <span class="adapter__dot" :class="a.ready ? 'on' : 'off'" />
            <span class="adapter__name">{{ a.name }}</span>
            <span class="adapter__trust" :class="trustInfo(a.trust).cls">
              {{ trustInfo(a.trust).label }}
            </span>
          </li>
        </ul>
      </details>
    </template>

    <!-- 连接中 -->
    <div v-else-if="mode === 'connecting'" class="card connect__connecting">
      <span class="spinner" />
      <p>{{ errorMsg || t("connect.connecting") }}</p>
      <m3e-button variant="text" @click="stop">{{ t("connect.disconnect") }}</m3e-button>
    </div>

    <!-- 已连接 -->
    <div v-else class="card connect__connected">
      <div class="connect__role">
        <span class="badge" :class="role === 'host' ? 'badge--host' : 'badge--guest'">
          {{ role === "host" ? t("connect.host") : t("connect.guest") }}
        </span>
      </div>

      <div class="connect__code">
        <span class="connect__code-label">{{ t("connect.field.roomCode") }}</span>
        <span class="connect__code-value">{{ roomCode }}</span>
      </div>

      <div class="connect__connected-actions">
        <m3e-button variant="tonal" @click="copyCode">
          <m3e-icon slot="icon" name="content_copy" />
          {{ t("connect.copyCode") }}
        </m3e-button>
        <m3e-button variant="tonal" @click="share">
          <m3e-icon slot="icon" name="share" />
          {{ t("connect.invite") }}
        </m3e-button>
        <m3e-button variant="outlined" @click="stop">
          <m3e-icon slot="icon" name="link_off" />
          {{ t("connect.disconnect") }}
        </m3e-button>
      </div>

      <p v-if="errorMsg" class="connect__error">{{ errorMsg }}</p>
    </div>
  </div>
</template>

<style scoped>
.connect {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-4);
  padding: var(--sp-6) var(--sp-4);
  overflow-y: auto;
}

.connect__game {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-1) var(--sp-3);
  border-radius: 999px;
  background: var(--secondary-container);
  color: var(--on-secondary-container);
  font-size: var(--fs-sm);
}

.card {
  width: 100%;
  max-width: 480px;
  background: var(--surface-container);
  border-radius: var(--radius-lg);
  padding: var(--sp-4);
  box-shadow: var(--shadow-1);
}

.card__title {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  margin-bottom: var(--sp-3);
}

.connect__empty {
  text-align: center;
  color: var(--text-muted);
}
.connect__empty i {
  font-size: 40px;
  opacity: 0.6;
}

.connect__actions {
  display: flex;
  gap: var(--sp-3);
  width: 100%;
  max-width: 480px;
}
.action {
  flex: 1;
}
.action--primary {
  flex: 1.4;
}

.field {
  width: 100%;
  margin-bottom: var(--sp-3);
}

.connect__join-actions,
.connect__connected-actions {
  display: flex;
  gap: var(--sp-2);
  flex-wrap: wrap;
  margin-top: var(--sp-2);
}

.connect__invite {
  display: flex;
  align-items: flex-end;
  gap: var(--sp-2);
}

.connect__adapters summary {
  cursor: pointer;
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}
.connect__adapters ul {
  list-style: none;
  margin: var(--sp-2) 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
}
.adapter {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-2);
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.adapter--active {
  background: var(--secondary-container);
}
.adapter__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.adapter__dot.on {
  background: var(--green, #2e7d32);
}
.adapter__dot.off {
  background: var(--text-muted);
}
.adapter__name {
  flex: 1;
  font-size: var(--fs-sm);
}
.adapter__trust {
  font-size: var(--fs-xs);
  padding: 2px 8px;
  border-radius: 999px;
}
.trust--official {
  background: var(--primary-container);
  color: var(--on-primary-container);
}
.trust--verified {
  background: var(--tertiary-container, #eaddff);
  color: var(--on-tertiary-container, #2a1a5e);
}
.trust--unsigned {
  background: var(--surface-variant);
  color: var(--text-muted);
}
.trust--blocked {
  background: var(--error-container, #f9dedc);
  color: var(--on-error-container, #410e0b);
}

.connect__connecting {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-3);
}
.spinner {
  width: 28px;
  height: 28px;
  border: 3px solid var(--outline);
  border-top-color: var(--primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.connect__role {
  display: flex;
  justify-content: center;
  margin-bottom: var(--sp-3);
}
.badge {
  padding: var(--sp-1) var(--sp-3);
  border-radius: 999px;
  font-size: var(--fs-sm);
  font-weight: var(--fw-semibold);
}
.badge--host {
  background: var(--primary-container);
  color: var(--on-primary-container);
}
.badge--guest {
  background: var(--tertiary-container, #eaddff);
  color: var(--on-tertiary-container, #2a1a5e);
}

.connect__code {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-1);
  margin-bottom: var(--sp-4);
}
.connect__code-label {
  font-size: var(--fs-xs);
  color: var(--text-muted);
}
.connect__code-value {
  font-size: 28px;
  font-weight: var(--fw-bold);
  letter-spacing: 2px;
  font-variant-numeric: tabular-nums;
}

.connect__connected-actions {
  justify-content: center;
}

.connect__error {
  margin-top: var(--sp-3);
  color: var(--error, #b3261e);
  font-size: var(--fs-sm);
  text-align: center;
}
</style>
