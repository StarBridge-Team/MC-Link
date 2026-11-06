<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import NavList, { type NavListItem } from "../components/layout/NavList.vue";
import { getSettingManifest, getSettingMeta } from "../lib/api/settingMeta";
import type { SettingSection } from "../lib/api/types";
import { filterEntries, mergeEntries, type SettingEntry } from "../lib/settings/registry";
import { KEYS, local } from "../lib/persist";
import { DEFAULT_SETTING_TAB } from "../router";
import SettingsManifestSection from "./settings/SettingsManifestSection.vue";

/**
 * 设置页容器：左侧导航（`m3e-nav-menu`）+ 右侧内容 + 顶部搜索。
 *
 * # 搜索能搜到什么
 *
 * 不只是分区名：
 * - 原生分区：`registry.ts` 的 `keywords` 覆盖该分区内**每个设置项**的名字；
 * - 服务端分区：拉取 `get_setting_meta` 的字段 label，一并进索引。
 *
 * 搜「模糊」「对比度」「玩家名」都能定位到对应分区，而不是只有搜「个性化」才有结果。
 */
const route = useRoute();
const router = useRouter();

const query = ref("");
const remote = ref<SettingSection[]>([]);
/** 远程分区的字段名索引（分区 id → 字段 label 列表）。 */
const fieldIndex = ref<Record<string, string[]>>({});

const entries = computed(() => mergeEntries(remote.value));
const flat = computed(() => entries.value);

/** 搜索过滤后喂给导航组件的项。 */
const navItems = computed<NavListItem[]>(() =>
  filterEntries(entries.value, query.value, (id) => fieldIndex.value[id] ?? []).map((e) => ({
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
    // 不限定在原生几个 tab：服务端分区与 HMCL 主题等也允许被记住。
    const saved = local.getString(KEYS.settingTab);
    if (saved && flat.value.some((e) => e.id === saved)) {
      activeId.value = saved;
    }
  }

  try {
    const manifest = await getSettingManifest();
    remote.value = manifest.sections;
    // 远程分区的字段名也纳入搜索。后端 `get_setting_meta` 带缓存，成本很低；
    // 单个分区失败不影响其它分区。
    await Promise.all(
      manifest.sections.map(async (section) => {
        try {
          const meta = await getSettingMeta(section.id);
          fieldIndex.value[section.id] = meta.fields.map((f) => f.label || f.key);
        } catch {
          /* 忽略：这个分区只是搜不到字段名 */
        }
      }),
    );
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
      <!-- 用组件库的搜索栏，而不是给 `form-field` 塞一个 input：
           后者的圆角/高度/清除按钮都是"输入框"语义，与搜索栏不一致。 -->
      <m3e-search-bar class="search" clearable @clear="query = ''">
        <m3e-icon slot="leading" name="search"></m3e-icon>
        <input
          slot="input"
          :value="query"
          type="search"
          placeholder="搜索设置项"
          @input="(e: Event) => (query = (e.target as HTMLInputElement).value)"
        />
      </m3e-search-bar>

      <NavList :items="navItems" :active="activeId" @select="select" />
    </aside>

    <main class="content">
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

.side {
  /* 宽度跟随导航栏规格，搜索栏与导航项左右对齐；不写死像素，缩放时一起变。 */
  flex: none;
  width: var(--nav-width);
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

/* 搜索栏占满侧栏宽度（组件默认是 inline 的，不给宽度会缩成内容宽）。 */
.search {
  width: 100%;
  flex: none;
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
