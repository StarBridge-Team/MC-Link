<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../ui/ChipSelect.vue";
import ColorField from "../ui/ColorField.vue";
import FieldRow from "../ui/FieldRow.vue";
import SettingCard from "../ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";
import { getBackgroundFiles } from "../../lib/api/datadir";
import type { BackgroundFile } from "../../lib/api/types";
import { isRemoteUrl } from "../../lib/appearance/background";

/**
 * 个性化 · 背景（背景 + 遮罩 + 背景音乐）。
 *
 * 文件列表来自数据目录的 `Background/` 文件夹：用户把图片/视频/音频放进去，
 * 这里只做"挑选"，不做上传与删除（那是文件管理器的事）。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

const files = ref<BackgroundFile[]>([]);
const filesLoading = ref(false);
const bgUrlInput = ref("");
const musicUrlInput = ref("");

const AUDIO_EXT = ["mp3", "wav", "ogg", "flac", "aac", "m4a"];

const musicFiles = computed(() =>
  files.value.filter((f) => AUDIO_EXT.includes(extensionOf(f.name))),
);

function extensionOf(name: string): string {
  return name.split(".").pop()?.toLowerCase() ?? "";
}

const typeOptions = computed<ChipOption<string>[]>(() => [
  { value: "default", label: t("background.typeDefault"), icon: "apps" },
  { value: "solid", label: t("background.typeSolid"), icon: "square" },
  { value: "image", label: t("background.typeImage"), icon: "image" },
  { value: "video", label: t("background.typeVideo"), icon: "movie" },
]);

const fitOptions = computed<ChipOption<string>[]>(() => [
  { value: "scale-to-fill", label: t("background.fitScaleToFill") },
  { value: "aspect-fit", label: t("background.fitAspectFit") },
  { value: "aspect-fill", label: t("background.fitAspectFill") },
  { value: "width-fix", label: t("background.fitWidthFix") },
  { value: "height-fix", label: t("background.fitHeightFix") },
]);

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

/** 已选背景是否为本地文件（用于列表高亮；URL 与文件名可能重名）。 */
function isSelectedLocal(name: string): boolean {
  return !isRemoteUrl(state.background_value) && state.background_value === name;
}

function isSelectedMusic(name: string): boolean {
  return state.music_mode === "file" && state.music_value === name;
}

onMounted(async () => {
  filesLoading.value = true;
  try {
    files.value = await getBackgroundFiles();
  } catch (e) {
    console.warn("[background] 读取 Background 目录失败:", e);
    files.value = [];
  } finally {
    filesLoading.value = false;
  }
  // 回填已保存的 URL，否则界面会显示为空，用户以为设置丢了。
  if (isRemoteUrl(state.background_value)) bgUrlInput.value = state.background_value;
  if (state.music_mode === "url") musicUrlInput.value = state.music_value;
});

function setType(value: string) {
  settings.patch({ background_type: value });
}

function setFit(value: string) {
  settings.patch({ background_fit: value });
}

function setSolidColor(value: string) {
  settings.patch({ background_value: value });
}

function selectFile(file: BackgroundFile) {
  bgUrlInput.value = "";
  settings.patch({
    background_value: file.name,
    background_type: file.is_video ? "video" : "image",
  });
}

function applyBackgroundUrl() {
  const url = bgUrlInput.value.trim();
  if (!url) return;
  settings.patch({
    background_value: url,
    background_type: /\.(mp4|webm|ogg|avi|mov|mkv|flv)(\?|#|$)/i.test(url)
      ? "video"
      : "image",
  });
}

function onOverlayToggle(e: Event) {
  settings.patch({ background_overlay: (e.target as HTMLInputElement).checked });
}

function onOverlayOpacity(e: Event) {
  const value = (e.target as HTMLElement & { value?: number }).value;
  settings.patch({ background_overlay_opacity: typeof value === "number" ? value : 0 });
}

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
  <SettingCard :icon="'image'" :title="t('background.title')" :desc="t('background.desc')" wide>
    <div class="field-label">{{ t("background.type") }}</div>
    <ChipSelect :model-value="state.background_type" :options="typeOptions" @update:model-value="setType" />

    <div v-if="state.background_type === 'solid'" class="solid">
      <ColorField :model-value="state.background_value || '#000000'" size="large" @update:model-value="setSolidColor" />
    </div>

    <template v-if="state.background_type === 'image' || state.background_type === 'video'">
      <div class="field-label">{{ t("background.fit") }}</div>
      <ChipSelect
        small
        :model-value="state.background_fit"
        :options="fitOptions"
        @update:model-value="setFit"
      />

      <div class="field-label">{{ t("background.localFiles") }}</div>
      <p class="hint">{{ t("background.localFilesHint") }}</p>
      <p v-if="filesLoading" class="hint">{{ t("background.scanning") }}</p>
      <p v-else-if="files.length === 0" class="hint">{{ t("background.localEmpty") }}</p>
      <div v-else class="file-list">
        <button
          v-for="file in files"
          :key="file.name"
          class="file-chip"
          :class="{ 'is-active': isSelectedLocal(file.name) }"
          type="button"
          @click="selectFile(file)"
        >
          <i class="material-symbols-rounded">{{ file.is_video ? 'movie' : 'photo' }}</i>
          <span class="ellipsis">{{ file.name }}</span>
        </button>
      </div>

      <div class="field-label">{{ t("background.url") }}</div>
      <!-- 回车监听挂在外层容器上：DOM 事件会冒泡，比依赖组件转发原生事件更可靠 -->
      <div class="url-row" @keyup.enter="applyBackgroundUrl">
        <m3e-form-field variant="outlined" class="url-row__input">
          <input v-model="bgUrlInput" :placeholder="t('background.urlPlaceholder')" />
        </m3e-form-field>
        <m3e-button variant="filled" @click="applyBackgroundUrl">{{ t("common.apply") }}</m3e-button>
      </div>

      <p v-if="state.background_value" class="hint">
        {{ t("background.current") }}:
        <span class="mono">{{ state.background_value }}</span>
      </p>
    </template>
  </SettingCard>

  <SettingCard :icon="'layers'" :title="t('background.overlay')" :desc="t('background.overlayDesc')">
    <FieldRow :label="t('background.overlay')">
      <m3e-switch :checked="state.background_overlay" @change="onOverlayToggle" />
    </FieldRow>
    <FieldRow v-if="state.background_overlay" :label="t('background.overlayOpacity')">
      <div class="slider-row">
        <m3e-slider
          class="slider-row__slider"
          :min="0"
          :max="100"
          :step="5"
          @change="onOverlayOpacity"
        >
          <m3e-slider-thumb :value="state.background_overlay_opacity" />
        </m3e-slider>
        <span class="slider-row__value mono">{{ Math.round(state.background_overlay_opacity) }}%</span>
      </div>
    </FieldRow>
  </SettingCard>

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
.solid {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}

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

.slider-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  width: 220px;
  max-width: 40vw;
}

.slider-row__slider {
  flex: 1;
  min-width: 0;
}

.slider-row__value {
  width: 48px;
  text-align: right;
  color: var(--text-secondary);
}
</style>
