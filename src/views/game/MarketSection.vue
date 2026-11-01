<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import InfoBar from "../../components/ui/InfoBar.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { showError, showSuccess } from "../../composables/useToast";
import { marketInstall, marketList } from "../../lib/api/plugin";
import type { MarketEntry } from "../../lib/api/types";

/**
 * 插件市场（B7）。
 *
 * 清单来自资源服务器 `plugins/market.json`，安装走 `market_install`
 * （下载 → SHA256 校验 → 解压 → 重扫注册表）。
 *
 * **三态必须分开**：加载中 / 拉取失败 / 空清单。后端 fail-closed，拉不到清单是
 * 报错而不是空结果——如果把失败渲染成"市场为空"，用户会以为没有插件可装。
 */
const { t } = useI18n();

const entries = ref<MarketEntry[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const installing = ref<string | null>(null);

const hasEntries = computed(() => entries.value.length > 0);

async function load() {
  loading.value = true;
  error.value = null;
  try {
    entries.value = (await marketList()).plugins;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function install(entry: MarketEntry) {
  installing.value = entry.id;
  try {
    await marketInstall(entry.id);
    showSuccess(t("game.marketInstalled", { name: entry.name }));
  } catch (e) {
    showError(
      `${t("game.marketInstallFailed")}: ${e instanceof Error ? e.message : String(e)}`,
    );
  } finally {
    installing.value = null;
  }
}

onMounted(load);
</script>

<template>
  <div class="scroll-area">
    <SettingCard
      v-if="hasEntries"
      icon="storefront"
      :title="t('game.marketTitle')"
      :desc="t('game.marketDesc')"
      wide
    >
      <div class="stagger">
        <div
          v-for="(entry, index) in entries"
          :key="entry.id"
          class="card"
          :style="{ '--stagger-index': index }"
        >
          <div class="card__head">
            <span class="card__name">{{ entry.name }}</span>
            <span class="chip mono">{{ entry.kind }}</span>
            <span v-if="entry.version" class="hint mono">v{{ entry.version }}</span>
          </div>
          <p v-if="entry.description" class="hint">{{ entry.description }}</p>
          <p v-if="entry.author" class="hint">
            {{ t("game.marketAuthor", { author: entry.author }) }}
          </p>
          <div class="card__actions">
            <m3e-button
              variant="filled"
              :disabled="installing === entry.id"
              @click="install(entry)"
            >
              {{
                installing === entry.id
                  ? t("game.marketInstalling")
                  : t("game.marketInstall")
              }}
            </m3e-button>
          </div>
        </div>
      </div>
    </SettingCard>

    <div v-else class="market">
      <span class="icon-badge"><i class="material-symbols-rounded">storefront</i></span>
      <h2 class="market__title">
        {{
          error
            ? t("game.marketErrorTitle")
            : loading
              ? t("game.marketLoading")
              : t("game.marketEmptyTitle")
        }}
      </h2>
      <InfoBar v-if="error" kind="danger" :text="error">
        <m3e-button size="small" @click="load">{{ t("game.marketRetry") }}</m3e-button>
      </InfoBar>
      <p v-else-if="!loading" class="market__desc">{{ t("game.marketEmptyDesc") }}</p>
      <p v-if="!error && !loading" class="hint">{{ t("game.marketEmptyHint") }}</p>
    </div>
  </div>
</template>

<style scoped>
.market {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--sp-3);
  min-height: 320px;
  text-align: center;
}

.market__title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  line-height: var(--lh-tight);
}

.market__desc {
  max-width: 46em;
  font-size: var(--fs-base);
  color: var(--text-muted);
  line-height: var(--lh-normal);
}

.card {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
  padding: var(--sp-4);
  border-radius: var(--shape-corner-m);
  background: var(--card-bg);
}

.card__head {
  display: flex;
  align-items: baseline;
  gap: var(--sp-3);
}

.card__name {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.card__actions {
  display: flex;
  justify-content: flex-end;
}

.chip {
  padding: 0 var(--sp-2);
  border-radius: var(--shape-corner-s);
  background: var(--secondary-container);
  color: var(--on-secondary-container);
  font-size: var(--fs-caption);
}
</style>
