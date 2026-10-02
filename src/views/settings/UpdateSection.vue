<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import FieldRow from "../../components/ui/FieldRow.vue";
import InfoBar from "../../components/ui/InfoBar.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { showError, showSuccess } from "../../composables/useToast";
import { useUpdater, type UpdateBlockReason } from "../../composables/useUpdater";
import { getAppVersion } from "../../lib/api/app";
import type { RuntimeInfo } from "../../lib/api/types";

/**
 * 个性化 → 更新。
 *
 * # 两道闸门决定界面该说什么
 *
 * 1. **构建渠道**：`dev` 连检查都不发起，`self-built` 只给手动下载链接。
 *    因此"检查更新"之后显示的可能是"当前构建不参与自动更新"，而不是"已是最新"。
 * 2. **安装形态**：便携版替换自身 exe，安装版交给安装器；由后端按 `latest.asset`
 *    决定，界面不需要区分。
 *
 * 安装成功约 0.6 秒后应用会退出，所以**先提示再调用**。
 */
const { t } = useI18n();
const updater = useUpdater();

const runtime = ref<RuntimeInfo | null>(null);
const checked = ref(false);
/** 本地读到的版本号：`check_update_command` 未跑过时也能把"当前版本"显示出来。 */
const localVersion = ref("");

/** 优先用检查结果里的版本，其次用本地版本（两者同源，只是时机不同）。 */
const currentVersion = computed(
  () => updater.currentVersion.value || localVersion.value || "—",
);

const blockedText = computed(() => {
  const reason = updater.blockedReason.value;
  if (!reason) return null;
  return reason === "dev" ? t("update.blockedDev") : t("update.blockedSelfBuilt");
});

const channelText = computed(() => {
  switch (updater.buildChannel.value) {
    case "dev":
      return t("update.channelDev");
    case "official":
      return t("update.channelOfficial");
    case "self-built":
      return t("update.channelSelfBuilt");
    default:
      return updater.buildChannel.value || t("common.unknown");
  }
});

/** 优先用检查结果里的形态；还没检查过就用 `get_runtime_info_command` 的结果。 */
const installMode = computed(() => updater.installMode.value || runtime.value?.install_mode || "");

const installModeText = computed(() =>
  installMode.value === "portable"
    ? t("update.modePortable")
    : installMode.value === "installed"
      ? t("update.modeInstalled")
      : t("common.unknown"),
);

/**
 * 进度百分比文案。
 *
 * 只在真的在下载/安装时才显示：`progress` 的初值就是 0，不设这个门会一直
 * 挂着一行毫无意义的"0%"。长度未知时（后端 `total = 0`）返回空串，交给
 * 不确定进度条表达。
 */
const progressText = computed(() => {
  const active = updater.downloading.value || updater.installing.value;
  if (!active || updater.progress.value < 0) return "";
  return `${updater.progress.value}%`;
});

/** 把 composable 抛出的原因代号翻成文案；不是代号就原样展示后端信息。 */
function reasonText(value: unknown): string {
  const raw = value instanceof Error ? value.message : String(value);
  const map: Record<UpdateBlockReason, string> = {
    dev: t("update.blockedDev"),
    "self-built": t("update.blockedSelfBuilt"),
    unsupported: t("update.autoInstallUnsupported"),
    "no-asset": t("update.noAsset"),
  };
  return map[raw as UpdateBlockReason] ?? raw;
}

onMounted(async () => {
  // 两项互不依赖：任一失败都要让另一项照常显示。
  await Promise.all([
    updater
      .loadRuntimeInfo()
      .then((info) => (runtime.value = info))
      .catch((e) => console.warn("[update] 读取运行环境失败:", e)),
    getAppVersion()
      .then((v) => (localVersion.value = v))
      .catch(() => undefined),
  ]);
});

async function check() {
  checked.value = false;
  try {
    await updater.check();
    checked.value = true;
  } catch (e) {
    showError(reasonText(e));
  }
}

async function install() {
  try {
    // 先给提示：调用成功后应用很快退出，界面上要留住"正在重启"这句话。
    showSuccess(t("update.installing"));
    await updater.install();
  } catch (e) {
    showError(reasonText(e));
  }
}

async function clearCache() {
  try {
    await updater.clearCache();
    showSuccess(t("update.cacheCleared"));
  } catch (e) {
    showError(reasonText(e));
  }
}

async function openManual(url: string) {
  try {
    await openUrl(url);
  } catch (e) {
    showError(String(e));
  }
}
</script>

<template>
  <div class="scroll-area">
    <div class="grid stagger">
      <SettingCard
        :icon="'cloud_download'"
        :title="t('update.title')"
        :desc="t('update.desc')"
        wide
      >
        <div class="rows">
          <FieldRow :label="t('update.current')">
            <span class="mono">{{ currentVersion }}</span>
          </FieldRow>
          <FieldRow :label="t('update.installMode')">
            <span>{{ installModeText }}</span>
          </FieldRow>
          <FieldRow :label="t('update.channel')">
            <span>{{ channelText }}</span>
          </FieldRow>
        </div>

        <div class="actions">
          <m3e-button
            variant="filled"
            :disabled="updater.checking.value"
            @click="check"
          >
            <m3e-icon slot="icon" name="sync" />
            <span>{{ updater.checking.value ? t("update.checking") : t("update.check") }}</span>
          </m3e-button>
          <m3e-button :disabled="updater.downloading.value" @click="clearCache">
            {{ t("update.clearCache") }}
          </m3e-button>
        </div>

        <!-- 不允许自动更新时，先解释原因，而不是显示"已是最新" -->
        <InfoBar v-if="blockedText" kind="warning" :text="blockedText">
          <m3e-button
            v-if="updater.manualUrl.value"
            size="small"
            @click="openManual(updater.manualUrl.value!)"
          >
            {{ t("update.manualDownload") }}
          </m3e-button>
        </InfoBar>

        <InfoBar
          v-else-if="checked && !updater.hasUpdate.value"
          kind="success"
          :text="t('update.upToDate')"
        />

        <template v-if="updater.hasUpdate.value && updater.latest.value">
          <InfoBar
            kind="info"
            :text="t('update.hasUpdate', { version: updater.latest.value.version })"
          >
            <span v-if="updater.mandatory.value" class="badge">{{ t("update.mandatory") }}</span>
          </InfoBar>

          <div class="rows">
            <FieldRow v-if="updater.latest.value.release_date" :label="t('update.releaseDate')">
              <span class="mono">{{ updater.latest.value.release_date }}</span>
            </FieldRow>
          </div>

          <div v-if="updater.latest.value.release_notes" class="notes">
            <div class="field-label">{{ t("update.releaseNotes") }}</div>
            <pre class="notes__body selectable">{{ updater.latest.value.release_notes }}</pre>
          </div>

          <div class="actions">
            <m3e-button
              v-if="updater.canAutoInstall.value"
              variant="filled"
              :disabled="updater.installing.value"
              @click="install"
            >
              {{ t("update.autoInstall") }}
            </m3e-button>
            <m3e-button
              v-if="updater.manualUrl.value"
              @click="openManual(updater.manualUrl.value!)"
            >
              {{ t("update.manualDownload") }}
            </m3e-button>
          </div>

          <InfoBar v-if="updater.canAutoInstall.value" kind="info" :text="t('update.installHint')" />
          <InfoBar
            v-else-if="!blockedText"
            kind="warning"
            :text="t('update.autoInstallUnsupported')"
          />

          <!-- 长度未知时后端给 total = 0，此时用不确定进度条而不是假装有个百分比 -->
          <m3e-linear-progress-indicator
            v-if="updater.downloading.value || updater.installing.value"
            :value="updater.progress.value < 0 ? 0 : updater.progress.value"
            :mode="updater.progress.value < 0 ? 'indeterminate' : 'determinate'"
          />
          <p v-if="progressText" class="hint">
            {{ t("update.progress") }}: <span class="mono">{{ progressText }}</span>
          </p>
        </template>

        <p v-if="updater.isDevBuild.value" class="hint">{{ t("update.forceCheckHint") }}</p>
      </SettingCard>
    </div>
  </div>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-4);
  align-content: start;
}

.rows {
  display: flex;
  flex-direction: column;
  max-width: 560px;
}

.rows :deep(.field-row + .field-row) {
  border-top: 1px solid var(--outline-variant);
}

.actions {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-wrap: wrap;
}

.notes__body {
  margin: 0;
  padding: var(--sp-3);
  max-height: 240px;
  overflow: auto;
  border-radius: var(--r-sm);
  background: var(--surface-container);
  color: var(--text-secondary);
  font-size: var(--fs-label);
  white-space: pre-wrap;
  word-break: break-word;
}

.badge {
  display: inline-flex;
  align-items: center;
  height: 20px;
  padding: 0 var(--sp-2);
  border-radius: var(--r-full);
  background: var(--error-container);
  color: var(--on-error-container);
  font-size: var(--fs-xs);
}
</style>
