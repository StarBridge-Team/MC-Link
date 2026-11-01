<script setup lang="ts">
import { onMounted, ref } from "vue";

import InfoBar from "../../components/ui/InfoBar.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";
import { showError, showSuccess } from "../../composables/useToast";
import { savePersonalization } from "../../lib/api/config";
import { listHmclThemes, type HmclTheme } from "../../lib/api/hmcl";

/**
 * HMCL 主题导入（B7）。
 *
 * 「应用」= 把 HMCL 主题里的强调色写成**本程序的种子色**（`theme_color`），
 * 走既有的动态配色链路（`useColorScheme` 会用这个种子重算整套 M3 调色板）。
 * 因此不新增第二套主题引擎，只多一个种子来源的入口。
 */
const settings = useSettings();

const themes = ref<HmclTheme[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const applying = ref<string | null>(null);

async function load() {
  loading.value = true;
  error.value = null;
  try {
    themes.value = await listHmclThemes();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function apply(theme: HmclTheme) {
  if (!theme.seed) {
    showError(`主题「${theme.name}」里没有可用的颜色`);
    return;
  }
  applying.value = theme.path;
  try {
    await savePersonalization({
      ...settings.state,
      theme_color: theme.seed,
      // 关掉"跟随背景图"，否则种子仍由背景图决定，刚写入的颜色会被覆盖。
      theme_from_background: false,
    });
    await settings.load();
    await settings.applyAll();
    showSuccess(`已应用 HMCL 主题「${theme.name}」的配色`);
  } catch (e) {
    showError(`应用主题失败: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    applying.value = null;
  }
}

onMounted(load);
</script>

<template>
  <div class="scroll-area">
    <SettingCard
      icon="palette"
      title="HMCL 主题"
      desc="读取 HMCL 启动器 skins 目录下的主题，把其中的强调色作为本程序的配色种子"
      wide
    >
      <InfoBar v-if="error" kind="danger" :text="error">
        <m3e-button size="small" @click="load">重试</m3e-button>
      </InfoBar>

      <p v-else-if="loading" class="hint">正在扫描 HMCL 主题…</p>

      <p v-else-if="themes.length === 0" class="hint">
        没有找到 HMCL 主题。请确认 HMCL 已安装主题（位于其数据目录的
        <code>skins</code> 文件夹）。
      </p>

      <div v-else class="themes">
        <div v-for="theme in themes" :key="theme.path" class="theme">
          <div class="theme__info">
            <span class="theme__name">{{ theme.name }}</span>
            <span v-if="theme.seed" class="theme__seed mono">
              <span class="dot" :style="{ background: theme.seed }" />
              {{ theme.seed }}
            </span>
            <span v-else class="hint">未解析到颜色</span>
          </div>
          <m3e-button
            variant="tonal"
            size="small"
            :disabled="!theme.seed || applying === theme.path"
            @click="apply(theme)"
          >
            {{ applying === theme.path ? "应用中…" : "应用配色" }}
          </m3e-button>
        </div>
      </div>
    </SettingCard>
  </div>
</template>

<style scoped>
.themes {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.theme {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-3);
  padding: var(--sp-2) 0;
  border-bottom: 1px solid color-mix(in srgb, var(--on-surface) 10%, transparent);
}

.theme__info {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  min-width: 0;
}

.theme__name {
  font-weight: var(--fw-medium);
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.theme__seed {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  color: var(--text-secondary);
}

.dot {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  border: 1px solid color-mix(in srgb, var(--on-surface) 20%, transparent);
}

code {
  font-family: var(--font-mono);
}
</style>
