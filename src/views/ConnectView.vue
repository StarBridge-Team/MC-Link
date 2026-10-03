<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { useConnect } from "../composables/useConnect";
import { showSuccess } from "../composables/useToast";
import HostPanel from "./connect/HostPanel.vue";
import MemberPanel from "./connect/MemberPanel.vue";

/**
 * 联机页外壳：顶部 NavBar 在「房主 / 成员」之间切换（照搬 GameView 的 m3e-nav-bar 写法）。
 * 房主模式展示扫到的本机游戏卡片；成员模式直接选适配器 + 填字段。
 * 一旦进入 connecting/connected，内容区切到「连接状态卡」，与两个模式互斥。
 */
const { t } = useI18n();
const {
  mode,
  role,
  roomCode,
  errorMsg,
  mount,
  unmount,
  stop,
  applyInvite,
  buildShareLink,
} = useConnect();

const tab = ref<"host" | "member">("host");

const tabs = computed(() => [
  { id: "host" as const, label: t("connect.tab.host") },
  { id: "member" as const, label: t("connect.tab.member") },
]);

const connected = computed(() => mode.value !== "idle");

let unlistenDeep: (() => void) | null = null;

onMounted(async () => {
  await mount();
  unlistenDeep = await listen<{ action: string; params: string[] }>("deep-link", (e) => {
    if (e.payload.action === "join" && e.payload.params[0]) {
      tab.value = "member";
      applyInvite(e.payload.params[0]);
    }
  });
});

onUnmounted(() => {
  unmount();
  unlistenDeep?.();
});

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
    <div class="connect__toolbar">
      <m3e-nav-bar mode="expanded" class="connect__tabs">
        <m3e-nav-item
          v-for="item in tabs"
          :key="item.id"
          :selected="item.id === tab"
          @click="tab = item.id"
        >
          {{ item.label }}
        </m3e-nav-item>
      </m3e-nav-bar>
    </div>

    <div class="connect__content">
      <!-- 连接中 / 已连接：与两个模式互斥 -->
      <div v-if="connected" class="card connect__connected">
        <div v-if="mode === 'connecting'" class="connect__connecting">
          <span class="spinner" />
          <p>{{ errorMsg || t("connect.connecting") }}</p>
        </div>

        <template v-else>
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
        </template>
      </div>

      <!-- 空闲：房主 / 成员 两个面板 -->
      <Transition v-else name="connect-fwd" appear mode="out-in">
        <HostPanel v-if="tab === 'host'" key="host" />
        <MemberPanel v-else key="member" />
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.connect {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.connect__toolbar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  padding: var(--sp-2) var(--sp-4);
}

.connect__tabs {
  margin-left: auto;
  --m3e-nav-bar-container-color: transparent;
  --m3e-nav-bar-height: auto;
  --m3e-nav-item-active-container-color: var(--secondary-container);
  --m3e-nav-item-active-label-text-color: var(--on-secondary-container);
  --m3e-nav-item-active-icon-color: var(--on-secondary-container);
}

.connect__content {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.card {
  width: 100%;
  max-width: 520px;
  margin: 0 auto;
  background: var(--surface-container);
  border-radius: var(--radius-lg);
  padding: var(--sp-4);
  box-shadow: var(--shadow-1);
}

.connect__connecting {
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
  display: flex;
  gap: var(--sp-2);
  flex-wrap: wrap;
  justify-content: center;
}

.connect__error {
  margin-top: var(--sp-3);
  color: var(--error, #b3261e);
  font-size: var(--fs-sm);
  text-align: center;
}

/* 面板切换：左右滑动翻页 + 淡入（复用 GameView 思路，仅动 transform/opacity） */
.connect-fwd-enter-active,
.connect-fwd-leave-active {
  transition: transform var(--motion-medium) var(--ease-standard),
    opacity var(--motion-medium) var(--ease-standard);
}
.connect-fwd-enter-from {
  opacity: 0;
  transform: translateX(40px);
}
.connect-fwd-leave-to {
  opacity: 0;
  transform: translateX(-40px);
}
</style>
