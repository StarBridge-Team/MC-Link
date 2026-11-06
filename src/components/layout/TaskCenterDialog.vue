<script setup lang="ts">
import { computed } from "vue";

import MediaControlWidget from "../widget/MediaControlWidget.vue";
import { useUpdater } from "../../composables/useUpdater";

/**
 * 任务中心浮层。
 *
 * 目前汇总的是**自动更新**这条链路（检查/下载/安装/失败）。它本来就有一整套
 * 长任务状态，却只有设置页里能看到；放到右下角入口后，进行中/失败的任务
 * 在主窗口就能直接看到。
 *
 * 之后要接下载/房间任务时，在 `tasks` 里追加即可，不用改布局。
 */
defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();

const updater = useUpdater();

const tasks = computed(() => {
  const list: { id: string; title: string; detail: string }[] = [];
  if (updater.checking.value) {
    list.push({ id: "check", title: "检查更新", detail: "正在检查…" });
  }
  if (updater.downloading.value) {
    list.push({
      id: "download",
      title: "下载更新",
      detail: `${Math.round(updater.progress.value * 100)}%`,
    });
  }
  if (updater.installing.value) {
    list.push({ id: "install", title: "安装更新", detail: "正在安装…" });
  }
  return list;
});

const failure = computed(() => updater.installError.value || updater.error.value || "");
</script>

<template>
  <div v-if="open" class="backdrop" @click="emit('close')">
    <div class="panel" @click.stop>
      <header class="panel__head">
        <span class="panel__title">任务中心</span>
        <button class="panel__close" type="button" @click="emit('close')">
          <i class="material-symbols-rounded">close</i>
        </button>
      </header>

      <p v-if="tasks.length === 0 && !failure" class="empty">暂无进行中的任务</p>

      <ul v-if="tasks.length > 0" class="tasks">
        <li v-for="task in tasks" :key="task.id" class="task">
          <span class="task__title">{{ task.title }}</span>
          <span class="task__detail mono">{{ task.detail }}</span>
        </li>
      </ul>

      <p v-if="failure" class="error">{{ failure }}</p>

      <!-- 背景音视频控制：原先挂在窗口右下角，现在并入任务中心，
           不再单独占一块悬浮区域。没有音源时组件自身不渲染。 -->
      <MediaControlWidget class="media" />
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: flex-end;
  justify-content: flex-end;
  padding: var(--sp-6);
  background: color-mix(in srgb, #000 24%, transparent);
}

.panel {
  width: 320px;
  max-width: 80vw;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
  padding: var(--sp-4);
  border-radius: var(--shape-corner-l, 16px);
  background: var(--surface-container-high);
  color: var(--on-surface);
  box-shadow: 0 8px 28px color-mix(in srgb, #000 28%, transparent);
}

.panel__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.panel__title {
  font-weight: var(--fw-semibold);
}

.panel__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: inherit;
  cursor: pointer;
}

.panel__close:hover {
  background: color-mix(in srgb, var(--on-surface) 10%, transparent);
}

.empty {
  color: var(--text-muted);
  font-size: var(--fs-label);
}

.tasks {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.task {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--sp-3);
  font-size: var(--fs-label);
}

.task__detail {
  color: var(--text-secondary);
}

.error {
  color: var(--error);
  font-size: var(--fs-label);
}

/* 媒体控制自带胶囊底色，这里只让它靠左、不拉伸。 */
.media {
  align-self: flex-start;
}
</style>
