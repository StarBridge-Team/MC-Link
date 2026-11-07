<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

/**
 * 背景音视频控制面板（B7）。
 *
 * 这是**第一个内置挂件**：它走与未来插件挂件相同的容器（`ui/WidgetLayer.vue`），
 * 只是内容由本程序提供。控制对象就是 `App.vue` 里的 `#bg-music` / `#bg-video`
 * （由 `lib/appearance/background.ts` 设置 src），因此这里直接操作 DOM，
 * 不必再向下传一套播放状态。
 *
 * 只在有背景音乐时显示：没有音源时挂一个播放键只会让人以为"点了没反应"。
 */
const audio = ref<HTMLAudioElement | null>(null);
const playing = ref(false);
const volume = ref(0.6);
const hasSource = ref(false);

let timer: ReturnType<typeof setInterval> | null = null;

function sync() {
  const el = document.getElementById("bg-music") as HTMLAudioElement | null;
  audio.value = el;
  // `src` 是属性（`audio.src = ...` 会同步到它）；用属性而非 attribute，
  // 因为后者在 `removeAttribute` 之后可能仍留着旧值。
  hasSource.value = Boolean(el?.src);
  playing.value = el ? !el.paused : false;
  if (el && Number.isFinite(el.volume)) volume.value = el.volume;
}

function toggle() {
  const el = audio.value;
  if (!el) return;
  if (el.paused) void el.play().catch(() => undefined);
  else el.pause();
  playing.value = !el.paused;
}

function setVolume(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  if (audio.value) audio.value.volume = value;
  volume.value = value;
}

onMounted(() => {
  sync();
  // 音源由 `applyBackground` 异步写入，低频轮询即可同步初始状态与外部变化。
  timer = setInterval(sync, 1000);
});

onBeforeUnmount(() => {
  if (timer) clearInterval(timer);
  timer = null;
});
</script>

<template>
  <div v-if="hasSource" class="media">
    <button class="media__btn" type="button" @click="toggle">
      <i class="material-symbols-rounded">{{ playing ? "pause" : "play_arrow" }}</i>
    </button>
    <input
      class="media__vol"
      type="range"
      min="0"
      max="1"
      step="0.01"
      :value="volume"
      @input="setVolume"
    />
  </div>
</template>

<style scoped>
.media {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-1) var(--sp-2);
  border-radius: 999px;
  background: color-mix(in srgb, var(--surface-container-high) 88%, transparent);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  box-shadow: 0 2px 8px color-mix(in srgb, #000 18%, transparent);
}

.media__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--on-surface);
  cursor: pointer;
}

.media__btn:hover {
  background: color-mix(in srgb, var(--on-surface) 10%, transparent);
}

.media__vol {
  width: 96px;
  accent-color: var(--primary);
}
</style>
