<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import InfoBar from "../../ui/InfoBar.vue";
import type { LegalBundle } from "../../../lib/api/types";

/**
 * 引导第二步：同意 EULA。
 *
 * # 拉不到条款不是错误
 *
 * 条款托管在资源服务器上；拉不到时后端返回 `eula: null` + `fallback_url`，
 * 界面提示用户去官网阅读后同意——**既不报错也不允许跳过**。
 * 此路径下的同意记录会如实标注 `version/sha256 = unfetched`，不假装知道正文内容。
 *
 * # 同意的是哪一版由后端裁决
 *
 * 界面不传版本与哈希，只传语言，所以用户伪造不了"我同意的是上一版"。
 */
const props = defineProps<{
  bundle: LegalBundle | null;
  loading: boolean;
}>();

const emit = defineEmits<{ agree: [] }>();

const { t } = useI18n();

const eula = computed(() => props.bundle?.eula ?? null);
const alreadyAccepted = computed(() => props.bundle?.accepted ?? null);

async function openFallback() {
  const url = props.bundle?.fallback_url;
  if (!url) return;
  try {
    await openUrl(url);
  } catch (e) {
    console.warn("[oobe] 打开官网失败:", e);
  }
}
</script>

<template>
  <div class="step">
    <p v-if="loading" class="hint">{{ t("oobe.eulaFetching") }}</p>

    <template v-else-if="eula">
      <div class="doc">
        <header class="doc__head">
          <span class="doc__title">{{ eula.title || t("oobe.eulaTitle") }}</span>
          <span class="tag mono">{{ t("oobe.eulaVersion") }} {{ eula.version }}</span>
        </header>
        <pre class="doc__body selectable">{{ eula.text }}</pre>
      </div>

      <InfoBar
        v-if="alreadyAccepted"
        kind="success"
        :text="t('oobe.eulaAgreed', { version: alreadyAccepted.version })"
      />

      <m3e-button variant="filled" class="block-btn" @click="emit('agree')">
        {{ t("oobe.eulaAgree") }}
      </m3e-button>
    </template>

    <template v-else>
      <p class="step__title">{{ t("oobe.eulaFallbackTitle") }}</p>
      <p class="hint">{{ t("oobe.eulaFallbackHint") }}</p>
      <InfoBar
        v-if="bundle?.error"
        kind="warning"
        :text="`${t('oobe.eulaUnfetchedNotice')} ${bundle.error}`"
      />
      <div class="actions">
        <m3e-button @click="openFallback">{{ t("oobe.eulaOpenWebsite") }}</m3e-button>
        <m3e-button variant="filled" @click="emit('agree')">{{ t("oobe.eulaAgree") }}</m3e-button>
      </div>
    </template>

    <p v-if="bundle?.attachments?.length" class="hint">
      {{ t("oobe.eulaAttachments") }}:
      {{ bundle.attachments.map((a) => a.title || a.id).join(" / ") }}
    </p>
  </div>
</template>

<style scoped>
.step {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  min-height: 0;
}

.step__title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.doc {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-radius: var(--r-md);
  border: 1px solid var(--outline-variant);
  background: var(--surface-container);
  overflow: hidden;
}

.doc__head {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  border-bottom: 1px solid var(--outline-variant);
}

.doc__title {
  flex: 1;
  min-width: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--text-primary);
}

.doc__body {
  margin: 0;
  padding: var(--sp-4);
  height: 260px;
  overflow: auto;
  font-family: var(--font-sans);
  font-size: var(--fs-label);
  line-height: var(--lh-loose);
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-word;
}

.actions {
  display: flex;
  gap: var(--sp-2);
  justify-content: flex-end;
}

.tag {
  display: inline-flex;
  align-items: center;
  height: 20px;
  padding: 0 var(--sp-2);
  border-radius: var(--r-full);
  background: var(--surface-container-high);
  color: var(--text-secondary);
  font-size: var(--fs-xs);
  white-space: nowrap;
}
</style>
