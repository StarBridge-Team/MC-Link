<script setup lang="ts">
import FieldRow from "../../components/ui/FieldRow.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";

/**
 * 插件运行参数（B7 新增分区）。
 *
 * 握手超时与网关并发上限此前是 `plugin/session/handshake.rs` 与
 * `plugin/gateway.rs` 里的常量：机器慢或插件多时，用户只能看着失败。
 * 现在可调，改动对**新连接**生效（存量连接不受影响）。
 */
const settings = useSettings();
const state = settings.state;

function valueOf(e: Event): number {
  const v = (e.target as HTMLElement & { value?: number }).value;
  return typeof v === "number" ? v : 0;
}

function onHandshake(e: Event) {
  const value = Math.min(60, Math.max(5, Math.round(valueOf(e))));
  settings.patch({ plugin_handshake_timeout: value }, { silent: true });
}

function onInflight(e: Event) {
  const value = Math.min(128, Math.max(4, Math.round(valueOf(e))));
  settings.patch({ gateway_max_inflight: value }, { silent: true });
}

function commit() {
  void settings.saveNow();
}
</script>

<template>
  <div class="scroll-area">
    <SettingCard
      icon="extension"
      title="插件运行"
      desc="插件握手等待与网关并发上限。改动对新建立的连接生效"
      wide
    >
      <FieldRow label="握手超时">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="5" :max="60" :step="5">
            <m3e-slider-thumb
              :value="state.plugin_handshake_timeout"
              @input="onHandshake"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">
            {{ Math.round(state.plugin_handshake_timeout) }}s
          </span>
        </div>
      </FieldRow>
      <p class="hint">插件冷启动较慢（尤其机械硬盘）时可调大，避免误判为握手失败。</p>

      <FieldRow label="网关并发上限">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="4" :max="128" :step="4">
            <m3e-slider-thumb
              :value="state.gateway_max_inflight"
              @input="onInflight"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">
            {{ Math.round(state.gateway_max_inflight) }}
          </span>
        </div>
      </FieldRow>
      <p class="hint">同时处理的插件连接数；超过上限的连接会被直接拒绝，不排队。</p>
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
