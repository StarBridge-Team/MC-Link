<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  mergeEntries,
  filterEntries,
  groupEntries,
  type SettingEntry,
} from "../lib/settings/registry";
import { getSettingManifest } from "../lib/api/settingMeta";
import type { SettingSection } from "../lib/api/types";
import SettingsManifestSection from "./settings/SettingsManifestSection.vue";
import { DEFAULT_SETTING_TAB, isSettingTab } from "../router";
import { KEYS, local } from "../lib/persist";

/**
 * 设置页容器：左树（按 group 分组）+ 右侧内容 + 顶部搜索。
 *
 * 分区不再硬编码映射（旧版的 `SECTIONS`），全部来自 `settings/registry.ts`——
 * 原生分区带组件，服务端下发分区带元配置 id。左树点击只切换 `activeId`，
 * 并同步进路由（保持可分享的 URL）与本地记忆（从别的页回来落到上次看的分区）。
 */
const route = useRoute();
const router = useRouter();

const query = ref("");
const remote = ref<SettingSection[]>([]);

const entries = computed(() => mergeEntries(remote.value));
const flat = computed(() => entries.value);
const groups = computed(() =>
  groupEntries(filterEntries(entries.value, query.value)),
);

const activeId = ref<string>(DEFAULT_SETTING_TAB);
const active = computed<SettingEntry | undefined>(
  () => flat.value.find((e) => e.id === activeId.value) ?? flat.value[0],
);

function select(id: string) {
  activeId.value = id;
  void router.replace({ name: "setting", params: { tab: id } }).catch(() => {});
  local.setString(KEYS.settingTab, id);
}

onMounted(async () => {
  const fromRoute = typeof route.params.tab === "string" ? route.params.tab : "";
  if (isSettingTab(fromRoute) && flat.value.some((e) => e.id === fromRoute)) {
    activeId.value = fromRoute;
  } else {
    const saved = local.getString(KEYS.settingTab);
    if (isSettingTab(saved) && flat.value.some((e) => e.id === saved)) {
      activeId.value = saved;
    }
  }
  // 服务端分区：拉清单后合并进左树，原生分区不依赖它先到位。
  try {
    remote.value = (await getSettingManifest()).sections;
  } catch {
    remote.value = [];
  }
});

watch(
  () => route.params.tab,
  (t) => {
    if (typeof t === "string" && flat.value.some((e) => e.id === t)) activeId.value = t;
  },
);
</script>

<template>
  <div class="setting">
    <aside class="tree">
      <div class="search">
        <m3e-form-field variant="outlined">
          <input
            :value="query"
            type="search"
            placeholder="搜索设置"
            @input="(e: Event) => (query = (e.target as HTMLInputElement).value)"
          />
        </m3e-form-field>
      </div>
      <nav class="groups">
        <div v-for="[group, items] in groups" :key="group" class="group">
          <div class="group__title">{{ group }}</div>
          <button
            v-for="item in items"
            :key="item.id"
            class="item"
            :class="{ active: item.id === activeId }"
            @click="select(item.id)"
          >
            <span class="material-symbols-rounded item__icon">{{ item.icon }}</span>
            <span class="item__label">{{ item.label }}</span>
          </button>
        </div>
      </nav>
    </aside>

    <main class="content">
      <!-- 原生分区用组件渲染；服务端分区交给 SettingsManifestSection 按 id 拉元配置。 -->
      <component
        v-if="active && active.kind === 'native' && active.component"
        :is="active.component"
        :key="active.id"
      />
      <SettingsManifestSection v-else-if="active && active.kind === 'remote'" :section-id="active.id" />
      <div v-else class="empty">{{ query ? "没有匹配的设置" : "暂无设置" }}</div>
    </main>
  </div>
</template>

<style scoped>
.setting {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 220px 1fr;
  gap: var(--sp-4);
}

.tree {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
  min-height: 0;
  padding-right: var(--sp-2);
  border-right: 1px solid color-mix(in srgb, var(--on-surface) 12%, transparent);
}

.search {
  flex: none;
}

.groups {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.group__title {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--text-secondary);
  padding: 0 var(--sp-2) var(--sp-1);
}

.item {
  width: 100%;
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  padding: var(--sp-2) var(--sp-3);
  border: none;
  border-radius: var(--shape-corner-m);
  background: transparent;
  color: var(--text-primary);
  cursor: pointer;
  font: inherit;
  text-align: left;
}

.item:hover {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
}

.item.active {
  background: var(--secondary-container);
  color: var(--on-secondary-container);
}

.item__icon {
  font-size: 20px;
}

.content {
  min-height: 0;
  overflow-y: auto;
  padding-left: var(--sp-2);
}

.empty {
  padding: var(--sp-8);
  text-align: center;
  color: var(--text-secondary);
}
</style>
