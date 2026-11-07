<script setup lang="ts">
import FieldRow from "../../components/ui/FieldRow.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";

/**
 * 网络与下载（B7 新增分区）。
 *
 * 这两个值此前写死在下载器里（`downloader/verified.rs`），弱网或服务端限速时
 * 用户没有任何调节手段。现在落到 `tuning`（启动与保存时应用），对**下一次下载**生效。
 */
const settings = useSettings();
const state = settings.state;

function valueOf(e: Event): number {
  const v = (e.target as HTMLElement & { value?: number }).value;
  return typeof v === "number" ? v : 0;
}

function onParallel(e: Event) {
  const value = Math.min(8, Math.max(1, Math.round(valueOf(e))));
  settings.patch({ download_parallel: value }, { silent: true });
}

function onIdle(e: Event) {
  const value = Math.min(120, Math.max(10, Math.round(valueOf(e))));
  settings.patch({ download_idle_timeout: value }, { silent: true });
}

function commit() {
  void settings.saveNow();
}
</script>

<template>
  <div class="scroll-area">
    <SettingCard
      icon="download"
      title="网络与下载"
      desc="下载并发与超时。改动对下一次下载生效，不影响正在进行的下载"
      wide
    >
      <FieldRow label="并行分片数">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="1" :max="8" :step="1">
            <m3e-slider-thumb
              :value="state.download_parallel"
              @input="onParallel"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">
            {{ Math.round(state.download_parallel) }}
          </span>
        </div>
      </FieldRow>
      <p class="hint">分片越多通常越快；弱网或服务端限速时调低反而更稳。</p>

      <FieldRow label="空闲超时">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="10" :max="120" :step="5">
            <m3e-slider-thumb
              :value="state.download_idle_timeout"
              @input="onIdle"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">
            {{ Math.round(state.download_idle_timeout) }}s
          </span>
        </div>
      </FieldRow>
      <p class="hint">连续这么久没有收到任何数据才判超时（不是整个下载的总时长）。</p>
    </SettingCard>
  </div>
</template>

<style scoped>
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

.hint {
  font-size: var(--fs-label);
  color: var(--text-muted);
  line-height: var(--lh-normal);
}
</style>
