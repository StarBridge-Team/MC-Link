<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import NavList, { type NavListItem } from "../components/layout/NavList.vue";
import { getSettingManifest } from "../lib/api/settingMeta";
import type { SettingSection } from "../lib/api/types";
import { filterEntries, mergeEntries, type SettingEntry } from "../lib/settings/registry";
import { KEYS, local } from "../lib/persist";
import { DEFAULT_SETTING_TAB } from "../router";
import SettingsManifestSection from "./settings/SettingsManifestSection.vue";

/**
 * 设置页容器：左侧导航（复用 `NavList` 的 `m3e-nav-menu`）+ 右侧内容 + 顶部搜索。
 *
 * # 为什么不用手写导航
 *
 * 左侧曾是一组手写 `<button>` + 自定义 CSS，观感与主窗口的 M3 Navigation Drawer
 * 不一致，选中指示器/键盘导航也要自己维护。现在统一走 `@m3e/web` 的导航组件，
 * 只把「分区列表」喂给它。
 *
 * # 分区来源
 *
 * 全部来自 `lib/settings/registry.ts`：原生分区带组件，服务端下发分区带元配置 id。
 * 去重与优先级也在那里（原生优先于远程）。
 */
const route = useRoute();
const router = useRouter();

const query = ref("");
const remote = ref<SettingSection[]>([]);

const entries = computed(() => mergeEntries(remote.value));
const flat = computed(() => entries.value);

/** 搜索过滤后喂给导航组件的项。 */
const navItems = computed<NavListItem[]>(() =>
  filterEntries(entries.value, query.value).map((e) => ({
    id: e.id,
    icon: e.icon,
    label: e.label,
  })),
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
  if (fromRoute && flat.value.some((e) => e.id === fromRoute)) {
    activeId.value = fromRoute;
  } else {
    // 不限定在原生 5 个 tab：服务端分区与 HMCL 主题等也允许被记住/从导航栏带入。
    const saved = local.getString(KEYS.settingTab);
    if (saved && flat.value.some((e) => e.id === saved)) {
      activeId.value = saved;
    }
  }
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
    <aside class="side">
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

      <NavList :items="navItems" :active="activeId" @select="select" />
    </aside>

    <main class="content">
      <!-- 原生分区用组件渲染；服务端分区交给 SettingsManifestSection 按 id 拉元配置。 -->
      <component
        v-if="active && active.kind === 'native' && active.component"
        :is="active.component"
        :key="active.id"
      />
      <SettingsManifestSection
        v-else-if="active && active.kind === 'remote'"
        :section-id="active.id"
      />
      <div v-else class="empty">{{ query ? "没有匹配的设置" : "暂无设置" }}</div>
    </main>
  </div>
</template>

<style scoped>
.setting {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: var(--sp-6);
}

/* 不画分割线：靠留白把导航与内容分开（用户明确不要竖线）。 */
.side {
  flex: none;
  width: var(--nav-width);
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

.search {
  flex: none;
  padding: 0 var(--sp-1);
}

.content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
  padding-right: var(--sp-2);
}

.empty {
  padding: var(--sp-8);
  text-align: center;
  color: var(--text-secondary);
}
</style>
