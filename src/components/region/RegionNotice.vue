<script setup lang="ts">
/**
 * 地区限制提示页：**整块替换**联机界面（不是弹窗，也不是禁用按钮）。
 *
 * 什么时候渲染由调用方决定（`useRegionStore().blocked`）；判定规则与范围见
 * `src-tauri/src/commands/region.rs`——只有中国大陆会命中，港澳台与其他地区不受影响，
 * 判定失败也按不受限处理。
 */
import { useI18n } from "vue-i18n";

import EmptyState from "../ui/EmptyState.vue";

const { t } = useI18n();
</script>

<template>
  <div class="region-notice">
    <EmptyState
      icon="public_off"
      :title="t('regionGate.blockedTitle')"
      :desc="t('regionGate.blockedBody')"
    />
  </div>
</template>

<style scoped>
/* `.shell__main` 是纵向 flex：这里要 flex:1 才能把提示垂直居中到整页 */
.region-notice {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
</style>
