<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { UnlistenFn } from "@tauri-apps/api/event";
import PluginCard from "../../components/plugin/PluginCard.vue";
import InfoBar from "../../components/ui/InfoBar.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { showError, showSuccess } from "../../composables/useToast";
import {
  getPluginGateway,
  listPlugins,
  onPluginEvent,
  reloadPlugins,
  setPluginBlocked,
  setPluginEnabled,
  setPluginGrants,
} from "../../lib/api/plugin";
import type { PluginFacets, PluginGateway, PluginInfo } from "../../lib/api/types";

/**
 * 个性化 → 插件。
 *
 * # 筛选走后端
 *
 * 搜索与筛选都传给 `plugin_list`，由后端统一口径并返回 `facets`（每个维度的
 * 计数以**其它**维度结果为分母）。前端不再自己过滤——否则计数与列表会分叉。
 *
 * # 未知取值不会让列表变砖
 *
 * 平台/方式/标签里的未知值由后端丢弃并在 `warnings` 里留痕，界面只负责显示这条告警。
 */
const { t } = useI18n();

const plugins = ref<PluginInfo[]>([]);
const facets = ref<PluginFacets | null>(null);
const warnings = ref<string[]>([]);
const total = ref(0);
const matched = ref(0);
const currentPlatform = ref("");
const loading = ref(false);
const error = ref<string | null>(null);
const gateway = ref<PluginGateway | null>(null);

const filter = reactive({
  query: "",
  kinds: [] as string[],
  methods: [] as string[],
  platforms: [] as string[],
  tags: [] as string[],
  enabledOnly: false,
});

let unlisten: UnlistenFn | null = null;
let searchTimer: ReturnType<typeof setTimeout> | null = null;

const FACET_KEYS = ["kinds", "methods", "platforms", "tags"] as const;

const facetRows = computed(() => {
  const f = facets.value;
  if (!f) return [];
  return FACET_KEYS.map((key) => ({
    key,
    label: t(`plugins.${key === "kinds" ? "kind" : key.slice(0, -1)}`),
    entries: Object.entries(f[key] ?? {}).sort((a, b) => b[1] - a[1]),
  })).filter((row) => row.entries.length > 0);
});

async function load() {
  loading.value = true;
  error.value = null;
  try {
    const result = await listPlugins({
      query: filter.query || undefined,
      kinds: filter.kinds.length ? filter.kinds : undefined,
      methods: filter.methods.length ? filter.methods : undefined,
      platforms: filter.platforms.length ? filter.platforms : undefined,
      tags: filter.tags.length ? filter.tags : undefined,
      enabledOnly: filter.enabledOnly || undefined,
    });
    plugins.value = result.plugins;
    facets.value = result.facets;
    warnings.value = result.warnings;
    total.value = result.total;
    matched.value = result.matched;
    currentPlatform.value = result.currentPlatform;
  } catch (e) {
    error.value = messageOf(e);
  } finally {
    loading.value = false;
  }
}

async function loadGateway() {
  try {
    gateway.value = await getPluginGateway();
  } catch {
    gateway.value = null;
  }
}

/** 搜索框输入：350ms 防抖，避免每敲一个字就跨一次 IPC。 */
function onSearchInput() {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void load(), 350);
}

function toggleFacet(key: (typeof FACET_KEYS)[number], value: string) {
  const list = filter[key] as string[];
  const index = list.indexOf(value);
  if (index >= 0) list.splice(index, 1);
  else list.push(value);
  void load();
}

function isFacetActive(key: (typeof FACET_KEYS)[number], value: string) {
  return (filter[key] as string[]).includes(value);
}

async function toggleEnabled(plugin: PluginInfo, enabled: boolean) {
  try {
    await setPluginEnabled(plugin.id, enabled);
    await load();
  } catch (e) {
    showError(`${t("common.saveFailed")}: ${messageOf(e)}`);
  }
}

async function toggleBlocked(plugin: PluginInfo, blocked: boolean) {
  try {
    await setPluginBlocked(plugin.id, blocked);
    await load();
  } catch (e) {
    showError(`${t("common.saveFailed")}: ${messageOf(e)}`);
  }
}

async function savePermissions(plugin: PluginInfo, granted: string[] | undefined) {
  try {
    await setPluginGrants(plugin.id, granted);
    showSuccess(granted === undefined ? t("plugins.permissionsReset") : t("plugins.permissionsSaved"));
    await load();
  } catch (e) {
    showError(`${t("common.saveFailed")}: ${messageOf(e)}`);
  }
}

async function reload() {
  try {
    warnings.value = await reloadPlugins();
    showSuccess(t("plugins.reloaded"));
    await load();
  } catch (e) {
    showError(`${t("common.saveFailed")}: ${messageOf(e)}`);
  }
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

onMounted(async () => {
  await Promise.all([load(), loadGateway()]);
  // 外部插件上下线时只刷新网关状态：列表更新由用户主动操作触发，
  // 避免插件频繁重连时界面不断重排。
  // 订阅失败（非 Tauri 环境）不影响列表本身，忽略即可。
  try {
    unlisten = await onPluginEvent(() => void loadGateway());
  } catch (e) {
    console.warn("[plugins] 订阅插件事件失败:", e);
  }
});

onUnmounted(() => {
  if (searchTimer) clearTimeout(searchTimer);
  unlisten?.();
  unlisten = null;
});
</script>

<template>
  <div class="scroll-area">
    <div class="stack">
      <SettingCard :icon="'bi bi-hdd-network'" :title="t('plugins.gateway')" wide>
        <div class="gateway">
          <span class="tag" :class="gateway?.running ? 'tag--ok' : 'tag--off'">
            {{ gateway?.running ? t("plugins.gatewayRunning") : t("plugins.gatewayStopped") }}
          </span>
          <span v-if="gateway?.port" class="hint">
            {{ t("plugins.gatewayPort") }}: <span class="mono">{{ gateway.port }}</span>
          </span>
          <span v-if="gateway?.protocolVersion" class="hint mono">
            v{{ gateway.protocolVersion }} · {{ gateway.subprotocol }}
          </span>
          <span class="grow" />
          <var-button size="small" text @click="reload">
            <i class="bi bi-arrow-repeat" />
            <span>{{ t("plugins.reload") }}</span>
          </var-button>
        </div>
      </SettingCard>

      <SettingCard :icon="'bi bi-puzzle'" :title="t('plugins.title')" :desc="t('plugins.desc')" wide>
        <div class="toolbar">
          <var-input
            v-model="filter.query"
            class="toolbar__search"
            :placeholder="t('plugins.searchPlaceholder')"
            clearable
            @update:model-value="onSearchInput"
          />
          <label class="toolbar__toggle">
            <var-switch
              :model-value="filter.enabledOnly"
              @update:model-value="(v: unknown) => { filter.enabledOnly = v === true; void load(); }"
            />
            <span class="hint">{{ t('plugins.enabledOnly') }}</span>
          </label>
        </div>

        <div v-for="row in facetRows" :key="row.key" class="facet">
          <span class="field-label">{{ row.label }}</span>
          <div class="facet__chips">
            <button
              v-for="[value, count] in row.entries"
              :key="value"
              class="chip"
              :class="{ 'is-active': isFacetActive(row.key, value) }"
              type="button"
              @click="toggleFacet(row.key, value)"
            >
              {{ value }}
              <span class="chip__count mono">{{ count }}</span>
            </button>
          </div>
        </div>

        <InfoBar v-if="error" kind="danger" :text="error">
          <var-button size="small" text @click="load">{{ t("common.retry") }}</var-button>
        </InfoBar>

        <p class="hint">
          {{ t("plugins.total", { count: total }) }} · {{ t("plugins.matched", { count: matched }) }}
          <span v-if="currentPlatform" class="mono"> · {{ currentPlatform }}</span>
        </p>

        <p v-if="loading" class="hint">{{ t("common.loading") }}</p>
        <p v-else-if="plugins.length === 0" class="hint">{{ t("plugins.empty") }}</p>
        <div v-else class="plugins">
          <PluginCard
            v-for="plugin in plugins"
            :key="plugin.id"
            :plugin="plugin"
            @toggle-enabled="(v) => toggleEnabled(plugin, v)"
            @toggle-blocked="(v) => toggleBlocked(plugin, v)"
            @save-permissions="(v) => savePermissions(plugin, v)"
          />
        </div>

        <InfoBar
          v-if="warnings.length > 0"
          kind="warning"
          :text="`${t('plugins.warnings')}: ${warnings.join(' / ')}`"
        />
      </SettingCard>
    </div>
  </div>
</template>

<style scoped>
.gateway {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  flex-wrap: wrap;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  flex-wrap: wrap;
}

.toolbar__search {
  flex: 1;
  min-width: 220px;
  max-width: 420px;
}

.toolbar__toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
}

.facet {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.facet__chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 var(--sp-3);
  border-radius: var(--r-full);
  border: 1px solid var(--outline);
  color: var(--text-secondary);
  font-size: var(--fs-label);
  transition: background-color var(--motion-short) var(--ease-standard),
    color var(--motion-short) var(--ease-standard);
}

.chip:hover {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
  color: var(--text-primary);
}

.chip.is-active {
  background: var(--secondary-container);
  border-color: transparent;
  color: var(--on-secondary-container);
  font-weight: var(--fw-medium);
}

.chip__count {
  color: var(--text-muted);
}

.chip.is-active .chip__count {
  color: inherit;
  opacity: 0.7;
}

.plugins {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

.tag {
  display: inline-flex;
  align-items: center;
  height: 22px;
  padding: 0 var(--sp-3);
  border-radius: var(--r-full);
  font-size: var(--fs-xs);
}

.tag--ok {
  background: var(--success-container);
  color: var(--on-success-container);
}

.tag--off {
  background: var(--surface-container-high);
  color: var(--text-muted);
}
</style>
