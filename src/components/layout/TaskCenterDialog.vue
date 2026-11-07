<script setup lang="ts">
import { computed } from "vue";

import MediaControlWidget from "../widget/MediaControlWidget.vue";
import { useUpdater } from "../../composables/useUpdater";

/**
 * 任务中心（组件库 `m3e-dialog`）。
 *
 * 汇总的是**自动更新**这条链路（检查/下载/安装/失败）。它本来就有一整套长任务状态，
 * 却只有设置页里能看到；放到左下角入口后，进行中/失败的任务在主窗口就能直接看到。
 *
 * 遮罩、圆角、进出动画、Esc 关闭全部由 `m3e-dialog` 提供，不再自己画 `backdrop`/`panel`。
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
  <m3e-dialog :open="open" @cancel="emit('close')">
    <span slot="header">任务中心</span>

    <div class="body">
      <p v-if="tasks.length === 0 && !failure" class="empty">暂无进行中的任务</p>

      <ul v-if="tasks.length > 0" class="tasks">
        <li v-for="task in tasks" :key="task.id" class="task">
          <span class="task__title">{{ task.title }}</span>
          <span class="task__detail mono">{{ task.detail }}</span>
        </li>
      </ul>

      <p v-if="failure" class="error">{{ failure }}</p>

      <!-- 背景音视频控制：原先挂在窗口角落，现在并入任务中心。没有音源时它自己不渲染。 -->
      <MediaControlWidget class="media" />
    </div>

    <m3e-dialog-action slot="actions" @click="emit('close')">关闭</m3e-dialog-action>
  </m3e-dialog>
</template>

<style scoped>
.body {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
  min-width: 280px;
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

.media {
  align-self: flex-start;
}
</style>
