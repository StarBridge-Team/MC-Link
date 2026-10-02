<script setup lang="ts">
import { useI18n } from "vue-i18n";
import EmptyState from "../components/ui/EmptyState.vue";

/**
 * 联机页占位。
 *
 * 按用户要求**暂时留空**：联机核心（WGP P2P）正在按插件化方案重接，
 * 旧的 P2P 命令已从后端移除（`src/lib/api/connect.ts` 保留作重接素材）。
 * 路由 `/connect` 保持存在，接入后把这里换成真正的联机界面即可。
 */
const { t } = useI18n();
</script>

<template>
  <EmptyState
    icon="bi bi-broadcast"
    :title="t('connect.placeholderTitle')"
    :desc="t('connect.placeholderDesc')"
  />
</template>
