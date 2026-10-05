<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../ui/ChipSelect.vue";
import SettingCard from "../ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";
import { getBackgroundFiles } from "../../lib/api/datadir";
import type { BackgroundFile } from "../../lib/api/types";

/**
 * 个性化 · 背景音乐。
 *
 * 与背景来源同源（都读 `Background/` 目录），但选的是音频文件，且支持"用视频自带音轨"。
 * 单独一张卡：它与"背景长什么样"没有关系，用户找不到它时不该去翻背景样式。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

const files = ref<BackgroundFile[]>([]);
const musicUrlInput = ref("");

const AUDIO_EXT = ["mp3", "wav", "ogg", "flac", "aac", "m4a"];

const musicFiles = computed(() =>
  files.value.filter((f) => AUDIO_EXT.includes(extensionOf(f.name))),
);

function extensionOf(name: string): string {
  return name.split(".").pop()?.toLowerCase() ?? "";
}

const musicModeOptions = computed<ChipOption<string>[]>(() => {
  const options: ChipOption<string>[] = [
    { value: "none", label: t("background.musicNone") },
    { value: "file", label: t("background.musicFile") },
    { value: "url", label: t("background.musicUrl") },
  ];
  // "使用视频音轨"只在真的选了视频背景时才有意义。
  if (state.background_type === "video" && state.background_value) {
    options.push({ value: "video", label: t("background.musicVideo") });
  }
  return options;
});

function isSelectedMusic(name: string): boolean {
  return state.music_mode === "file" && state.music_value === name;
}

onMounted(async () => {
  try {
    files.value = await getBackgroundFiles();
  } catch (e) {
    console.warn("[background] 读取 Background 目录失败:", e);
    files.value = [];
  }
  // 回填已保存的 URL，否则界面会显示为空，用户以为设置丢了。
  if (state.music_mode === "url") musicUrlInput.value = state.music_value;
});

function setMusicMode(value: string) {
  settings.patch({ music_mode: value });
}

function selectMusic(file: BackgroundFile) {
  musicUrlInput.value = "";
  settings.patch({ music_mode: "file", music_value: file.name });
}

function applyMusicUrl() {
  const url = musicUrlInput.value.trim();
  if (!url) return;
  settings.patch({ music_mode: "url", music_value: url });
}
</script>

<template>
  <SettingCard :icon="'music_note'" :title="t('background.music')" :desc="t('background.musicDesc')">
    <ChipSelect
      small
      :model-value="state.music_mode"
      :options="musicModeOptions"
      @update:model-value="setMusicMode"
    />

    <template v-if="state.music_mode === 'file'">
      <p v-if="musicFiles.length === 0" class="hint">{{ t("background.musicEmpty") }}</p>
      <div v-else class="file-list">
        <button
          v-for="file in musicFiles"
          :key="file.name"
          class="file-chip"
          :class="{ 'is-active': isSelectedMusic(file.name) }"
          type="button"
          @click="selectMusic(file)"
        >
          <i class="material-symbols-rounded">audio_file</i>
          <span class="ellipsis">{{ file.name }}</span>
        </button>
      </div>
    </template>

    <div v-if="state.music_mode === 'url'" class="url-row" @keyup.enter="applyMusicUrl">
      <m3e-form-field variant="outlined" class="url-row__input">
        <input v-model="musicUrlInput" :placeholder="t('background.musicUrlPlaceholder')" />
      </m3e-form-field>
      <m3e-button variant="filled" @click="applyMusicUrl">{{ t("common.apply") }}</m3e-button>
    </div>
  </SettingCard>
</template>

<style scoped>
.file-list {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
  max-height: 168px;
  overflow-y: auto;
}

.file-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  max-width: 220px;
  height: 32px;
  padding: 0 var(--sp-3);
  border-radius: var(--r-sm);
  border: 1px solid var(--outline);
  color: var(--text-secondary);
  font-size: var(--fs-label);
  transition: background-color var(--motion-short) var(--ease-standard),
    color var(--motion-short) var(--ease-standard);
}

.file-chip:hover {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
  color: var(--text-primary);
}

.file-chip.is-active {
  background: var(--secondary-container);
  border-color: transparent;
  color: var(--on-secondary-container);
}

.url-row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  max-width: 560px;
}

.url-row__input {
  flex: 1;
  min-width: 0;
}
</style>
