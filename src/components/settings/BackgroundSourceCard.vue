<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../ui/ChipSelect.vue";
import ColorField from "../ui/ColorField.vue";
import SettingCard from "../ui/SettingCard.vue";
import { forceReloadRemote, remoteBgLoading, useSettings } from "../../composables/useSettings";
import { getBackgroundFiles } from "../../lib/api/datadir";
import type { BackgroundFile } from "../../lib/api/types";
import { isRemoteUrl } from "../../lib/appearance/background";

/**
 * 个性化 · 背景来源：纯色 / 图片 / 视频的选择与适配。
 *
 * 只负责"用什么当背景"。"看得见多少材质""糊到什么程度"分别在同级的
 * 透明度卡片与模糊卡片里 —— 拆开是因为它们与来源类型无关，
 * 挤在一起会让四个互不相干的设置看起来像一组。
 *
 * 文件列表来自数据目录的 `Background/` 文件夹：用户把图片/视频放进去，
 * 这里只做"挑选"，不做上传与删除（那是文件管理器的事）。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

const files = ref<BackgroundFile[]>([]);
const filesLoading = ref(false);
const bgUrlInput = ref("");

/** 只有图片/视频才需要"挑文件"；纯色与默认不显示这一段。 */
const isMedia = computed(
  () => state.background_type === "image" || state.background_type === "video",
);

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

/** 已选背景是否为本地文件（用于列表高亮；URL 与文件名可能重名）。 */
function isSelectedLocal(name: string): boolean {
  return !isRemoteUrl(state.background_value) && state.background_value === name;
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
});

function setType(value: string) {
  settings.patch({ background_type: value });
}

function setFit(value: string) {
  settings.patch({ background_fit: value });
}

/**
 * 选纯色。
 *
 * 存的是**不带 alpha 的十六进制**：透明度由「背景不透明度」单独管，
 * 让颜色字段只表达颜色，避免同一个效果有两个来源（见 `background_opacity` 的说明）。
 */
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

/**
 * 应用 URL 背景。
 *
 * 只写入设置就返回，**不等下载**：网络图/视频的下载在 `useSettings.refreshBackground`
 * 里异步进行，界面用 `remoteBgLoading` 在按钮上转圈表示"正在生效"。
 * 阻塞在这里会让"填了个大视频"变成界面卡死好几分钟。
 *
 * 这里是**唯一**会触发重新下载的操作：设置落盘后由 watch 触发，
 * 且因为 URL 变化（随机图片 API 常带不同参数）或强制刷新标记而重新拉取。
 */
function applyBackgroundUrl() {
  const url = bgUrlInput.value.trim();
  if (!url) return;
  const isVideo = /\.(mp4|webm|ogg|avi|mov|mkv|flv)(\?|#|$)/i.test(url);

  // 同一个 URL 也允许重新拉取（随机图片 API 的典型用法：点一次换一张）。
  // 通过 `forceReloadRemote()` 让 `useSettings` 丢弃已解析结果，而不是往设置里
  // 塞一个后端不认识的字段——那样会被序列化时丢掉，且污染配置契约。
  forceReloadRemote();

  settings.patch({
    background_value: url,
    background_type: isVideo ? "video" : "image",
  });
}
</script>

<template>
  <SettingCard :icon="'image'" :title="t('background.title')" :desc="t('background.desc')" wide>
    <div class="field-label">{{ t("background.type") }}</div>
    <ChipSelect
      :model-value="state.background_type"
      :options="typeOptions"
      @update:model-value="setType"
    />

    <div v-if="state.background_type === 'solid'" class="solid">
      <span class="field-label">{{ t("background.solidColor") }}</span>
      <ColorField
        :model-value="state.background_value || '#000000'"
        size="large"
        @update:model-value="setSolidColor"
      />
    </div>

    <template v-if="isMedia">
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
          <m3e-icon :name="file.is_video ? 'movie' : 'photo'" />
          <span class="ellipsis">{{ file.name }}</span>
        </button>
      </div>

      <div class="field-label">{{ t("background.url") }}</div>
      <!-- 回车监听挂在外层容器上：DOM 事件会冒泡，比依赖组件转发原生事件更可靠 -->
      <div class="url-row" @keyup.enter="applyBackgroundUrl">
        <m3e-form-field variant="outlined" class="url-row__input">
          <input v-model="bgUrlInput" :placeholder="t('background.urlPlaceholder')" />
        </m3e-form-field>
        <!--
          下载在后台进行（不等它做完才返回），所以按钮用转圈表达"正在生效"。
          `disabled` 同时防连点：连点会让同一张图被下多次。
        -->
        <m3e-button
          variant="filled"
          :disabled="remoteBgLoading"
          @click="applyBackgroundUrl"
        >
          <m3e-icon v-if="remoteBgLoading" slot="icon" name="progress_activity" class="spin" />
          {{ remoteBgLoading ? t("background.fetching") : t("common.apply") }}
        </m3e-button>
      </div>

      <p v-if="state.background_value" class="hint">
        {{ t("background.current") }}:
        <span class="mono">{{ state.background_value }}</span>
      </p>
    </template>
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

/* 图标槽里的转圈：只动 transform，不写 @keyframes 之外的属性。
   字形本身是 `progress_activity`（一圈缺口），转起来就是加载指示器。 */
.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* 尊重系统的"减少动画"偏好：转圈是纯装饰，静态显示也够表达"进行中"。 */
@media (prefers-reduced-motion: reduce) {
  .spin {
    animation: none;
  }
}
</style>
