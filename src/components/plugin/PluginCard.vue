<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { PluginInfo } from "../../lib/api/types";

/**
 * 单个插件条目。
 *
 * 只负责展示与收集用户意图，所有写操作都通过事件交给父组件去做——
 * 这样"改完之后列表怎么刷新"只有一个地方决定。
 *
 * 权限编辑器在展开区里：权限是本界面最"重"的信息，默认折叠，
 * 避免列表被十几行复选框撑开。
 */
const props = defineProps<{ plugin: PluginInfo }>();

const emit = defineEmits<{
  "toggle-enabled": [enabled: boolean];
  "toggle-blocked": [blocked: boolean];
  "save-permissions": [granted: string[] | undefined];
}>();

const { t } = useI18n();

const expanded = ref(false);

/** 本地编辑态的权限集合，仅在展开并保存时提交。 */
const draft = ref<string[]>([]);

watch(
  () => props.plugin,
  (plugin) => {
    draft.value = [...plugin.permissions];
  },
  { immediate: true, deep: true },
);

const KIND_ICON: Record<string, string> = {
  adapter: "cable",
  detector: "search",
  coupler: "link",
};

const TRUST_KEY: Record<string, string> = {
  official: "plugins.trustOfficial",
  verified: "plugins.trustVerified",
  unsigned: "plugins.trustUnsigned",
  blocked: "plugins.trustBlocked",
};

const kindIcon = computed(() => KIND_ICON[props.plugin.kind] ?? "extension");
const kindLabel = computed(() => props.plugin.kind);
const trustLabel = computed(() => t(TRUST_KEY[props.plugin.trust] ?? "plugins.trustUnsigned"));
const sourceLabel = computed(() =>
  props.plugin.source === "builtin" ? t("plugins.sourceBuiltin") : t("plugins.sourceExternal"),
);

/** 权限项是否已授予。 */
function granted(name: string): boolean {
  return draft.value.includes(name);
}

function setGranted(name: string, value: unknown) {
  const on = value !== false;
  const next = new Set(draft.value);
  if (on) next.add(name);
  else next.delete(name);
  draft.value = [...next];
}

const dirty = computed(() => {
  const a = [...draft.value].sort().join(",");
  const b = [...props.plugin.permissions].sort().join(",");
  return a !== b;
});

/** 展示用：声明过的权限（含被信任等级拒绝的），附说明与风险标记。 */
const permissionRows = computed(() => {
  const described = new Map(
    props.plugin.permissionDescriptions.map((p) => [p.name, p] as const),
  );
  const names = new Set([...props.plugin.declaredPermissions, ...props.plugin.permissions]);
  return [...names].map((name) => ({
    name,
    description: described.get(name)?.description ?? "",
    highRisk: described.get(name)?.highRisk ?? false,
    denied: props.plugin.deniedByCeiling.includes(name),
  }));
});

function toggleEnabled(e: Event) {
  emit("toggle-enabled", (e.target as HTMLInputElement).checked);
}

function toggleBlocked(e: Event) {
  emit("toggle-blocked", (e.target as HTMLInputElement).checked);
}
</script>

<template>
  <m3e-card
    variant="outlined"
    class="plugin"
    :class="{ 'is-disabled': !plugin.enabled, 'is-blocked': plugin.trust === 'blocked' }"
  >
    <div class="plugin-inner">
      <header class="plugin__head">
        <span class="icon-badge icon-badge--small"><i class="material-symbols-rounded">{{ kindIcon }}</i></span>

        <div class="grow">
          <div class="plugin__title-row">
            <span class="plugin__name ellipsis">{{ plugin.name }}</span>
            <span class="tag mono">{{ plugin.version }}</span>
            <span class="tag">{{ kindLabel }}</span>
            <span class="tag">{{ sourceLabel }}</span>
            <span class="tag" :class="{ 'tag--danger': plugin.trust === 'blocked' }">{{ trustLabel }}</span>
            <span v-if="!plugin.runnable" class="tag tag--warning">{{ t("common.notSupported") }}</span>
          </div>
          <p v-if="plugin.description" class="plugin__desc">{{ plugin.description }}</p>
          <p v-else class="plugin__desc plugin__desc--muted mono">{{ plugin.id }}</p>
        </div>

        <div class="plugin__actions">
          <m3e-switch
            :checked="plugin.enabled"
            :disabled="plugin.trust === 'blocked'"
            @change="toggleEnabled"
          />
          <m3e-button size="small" @click="expanded = !expanded">
            <m3e-icon slot="icon" :name="expanded ? 'expand_less' : 'expand_more'" />
            <span>{{ t("plugins.details") }}</span>
          </m3e-button>
        </div>
      </header>

      <div v-if="expanded" class="plugin__body">
      <dl class="meta">
        <div v-if="plugin.platforms.length" class="meta__row">
          <dt>{{ t("plugins.platform") }}</dt>
          <dd>{{ plugin.platforms.join(" · ") }}</dd>
        </div>
        <div v-if="plugin.methods.length" class="meta__row">
          <dt>{{ t("plugins.method") }}</dt>
          <dd>{{ plugin.methods.join(" · ") }}</dd>
        </div>
        <div v-if="plugin.games.length" class="meta__row">
          <dt>{{ t("plugins.games") }}</dt>
          <dd>{{ plugin.games.join(" · ") }}</dd>
        </div>
        <div v-if="plugin.tags.length" class="meta__row">
          <dt>{{ t("plugins.tag") }}</dt>
          <dd>{{ plugin.tags.join(" · ") }}</dd>
        </div>
        <div class="meta__row">
          <dt>{{ t("plugins.priority") }}</dt>
          <dd class="mono">{{ plugin.priority }}</dd>
        </div>
        <div v-if="plugin.fallback.length" class="meta__row">
          <dt>{{ t("plugins.fallback") }}</dt>
          <dd class="mono ellipsis">{{ plugin.fallback.join(" → ") }}</dd>
        </div>
      </dl>

      <div class="permissions">
        <div class="permissions__head">
          <span class="field-label">{{ t("plugins.permissions") }}</span>
          <span class="hint">{{ t("plugins.permissionsDesc") }}</span>
        </div>

        <p v-if="permissionRows.length === 0" class="hint">{{ t("plugins.noPermissions") }}</p>
        <div v-else class="permissions__list">
          <label v-for="row in permissionRows" :key="row.name" class="permission" :class="{ 'is-denied': row.denied }">
            <m3e-checkbox
              :checked="granted(row.name)"
              :disabled="row.denied"
              @change="(e: Event) => setGranted(row.name, (e.target as HTMLInputElement).checked)"
            />
            <span class="permission__text">
              <span class="permission__name mono">{{ row.name }}</span>
              <span v-if="row.highRisk" class="tag tag--danger">{{ t("plugins.highRisk") }}</span>
              <span v-if="row.denied" class="tag tag--warning">{{ t("plugins.deniedByCeiling") }}</span>
              <span v-if="row.description" class="hint">{{ row.description }}</span>
            </span>
          </label>
        </div>

        <div class="permissions__actions">
          <m3e-button size="small" variant="filled" :disabled="!dirty" @click="emit('save-permissions', [...draft])">
            {{ t("common.save") }}
          </m3e-button>
          <m3e-button size="small" @click="emit('save-permissions', undefined)">
            {{ t("common.reset") }}
          </m3e-button>
        </div>
      </div>

      <div class="block-row">
        <span class="hint">{{ t("plugins.blocked") }}</span>
        <m3e-switch
          :checked="plugin.trust === 'blocked'"
          @change="toggleBlocked"
        />
      </div>
    </div>
  </div>
  </m3e-card>
</template>

<style scoped>
/* var-card 已提供填充背景 / 圆角 / 外边距；这里只管内部纵向间距与状态边框。 */
.plugin-inner {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

.plugin {
  transition: border-color var(--motion-short) var(--ease-standard);
}

.plugin.is-disabled {
  opacity: 0.62;
}

.plugin.is-blocked {
  border: 1px solid color-mix(in srgb, var(--error) 50%, var(--outline-variant));
}

.plugin__head {
  display: flex;
  align-items: flex-start;
  gap: var(--sp-3);
}

.plugin__title-row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-wrap: wrap;
}

.plugin__name {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.plugin__desc {
  margin-top: 2px;
  font-size: var(--fs-label);
  color: var(--text-secondary);
  line-height: var(--lh-normal);
}

.plugin__desc--muted {
  color: var(--text-muted);
}

.plugin__actions {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-shrink: 0;
}

.plugin__body {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  padding-top: var(--sp-3);
  border-top: 1px solid var(--outline-variant);
}

.meta {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-2) var(--sp-5);
  margin: 0;
}

.meta__row {
  display: flex;
  gap: var(--sp-2);
  min-width: 0;
}

.meta dt {
  flex-shrink: 0;
  font-size: var(--fs-label);
  color: var(--text-muted);
}

.meta dd {
  margin: 0;
  min-width: 0;
  font-size: var(--fs-label);
  color: var(--text-secondary);
}

.permissions {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.permissions__head {
  display: flex;
  align-items: baseline;
  gap: var(--sp-3);
}

.permissions__list {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-1) var(--sp-4);
}

@media (max-width: 900px) {
  .permissions__list,
  .meta {
    grid-template-columns: 1fr;
  }
}

.permission {
  display: flex;
  align-items: flex-start;
  gap: var(--sp-2);
  min-width: 0;
}

.permission.is-denied {
  opacity: 0.6;
}

.permission__text {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-wrap: wrap;
  min-width: 0;
}

.permission__name {
  color: var(--text-secondary);
}

.permissions__actions {
  display: flex;
  gap: var(--sp-2);
}

.block-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-3);
}

.tag {
  display: inline-flex;
  align-items: center;
  height: 20px;
  padding: 0 var(--sp-2);
  border-radius: var(--r-full);
  background: var(--surface-container-high);
  color: var(--text-secondary);
  font-size: var(--fs-xs);
  white-space: nowrap;
}

.tag--warning {
  background: var(--warning-container);
  color: var(--on-warning-container);
}

.tag--danger {
  background: var(--error-container);
  color: var(--on-error-container);
}
</style>
