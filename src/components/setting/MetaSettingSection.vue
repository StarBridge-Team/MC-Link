<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { getSetting, saveSetting } from "../../lib/api/settings";
import { getSettingMeta } from "../../lib/api/settingMeta";
import { yamlGet, yamlSet } from "../../lib/yaml";
import type { SettingMeta, FieldMeta } from "../../lib/api/types";

const props = defineProps<{
  section: string;
  showToast: (msg: string) => void;
}>();

const loading = ref(false);
const saving = ref(false);
const meta = ref<SettingMeta | null>(null);
const rawYaml = ref("");
const errorMsg = ref("");

/** 当前字段值缓存（按 key 索引） */
const values = ref<Record<string, string>>({});

/** 按 group 分组的字段 */
const groupedFields = computed(() => {
  if (!meta.value) return [];
  const groups = new Map<string, FieldMeta[]>();
  for (const f of meta.value.fields) {
    const g = f.group || "";
    if (!groups.has(g)) groups.set(g, []);
    groups.get(g)!.push(f);
  }
  return Array.from(groups.entries()).map(([name, fields]) => ({ name, fields }));
});

async function load() {
  loading.value = true;
  errorMsg.value = "";
  try {
    // 并行拉取元配置与当前值
    const [m, y] = await Promise.all([
      getSettingMeta(props.section).catch((e) => {
        throw new Error(`元配置拉取失败: ${typeof e === "string" ? e : (e as Error).message}`);
      }),
      getSetting(props.section).catch(() => ""),
    ]);
    meta.value = m;
    rawYaml.value = y;
    refreshValues();
  } catch (e: any) {
    errorMsg.value = typeof e === "string" ? e : (e as Error).message;
  } finally {
    loading.value = false;
  }
}

function refreshValues() {
  const next: Record<string, string> = {};
  if (meta.value) {
    for (const f of meta.value.fields) {
      const v = yamlGet(rawYaml.value, f.key);
      next[f.key] = v === undefined ? f.default : v;
    }
  }
  values.value = next;
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => persist(), 400);
}

async function persist() {
  if (!meta.value) return;
  saving.value = true;
  try {
    let y = rawYaml.value;
    for (const f of meta.value.fields) {
      y = yamlSet(y, f.key, values.value[f.key] ?? f.default);
    }
    await saveSetting(props.section, y);
    rawYaml.value = y;
  } catch (e: any) {
    props.showToast(`保存失败: ${typeof e === "string" ? e : (e as Error).message}`);
  } finally {
    saving.value = false;
  }
}

function onFieldChange(field: FieldMeta, newVal: string) {
  values.value[field.key] = newVal;
  if (field.auto_save) {
    scheduleSave();
  }
}

function onSwitchChange(field: FieldMeta, checked: boolean) {
  onFieldChange(field, checked ? "true" : "false");
}

function onNumberChange(field: FieldMeta, val: string | number) {
  onFieldChange(field, String(val));
}

function onChipsChange(field: FieldMeta, val: string) {
  onFieldChange(field, val);
}

function onSliderChange(field: FieldMeta, val: number) {
  onFieldChange(field, String(val));
}

function saveNow() {
  if (saveTimer) clearTimeout(saveTimer);
  return persist();
}

watch(() => props.section, () => load());

onMounted(() => load());

defineExpose({ reload: load });
</script>

<template>
  <div v-if="loading" class="loading-text">
    <el-icon class="is-loading"><i class="bi bi-arrow-repeat" /></el-icon>
    <span>加载中...</span>
  </div>

  <el-alert
    v-else-if="errorMsg"
    type="error"
    :closable="false"
    class="error-bar"
  >
    <template #title>
      <span class="error-text">
        <i class="bi bi-exclamation-triangle-fill"></i>
        {{ errorMsg }}
      </span>
    </template>
  </el-alert>

  <div v-else-if="meta" class="meta-section">
    <el-card v-if="meta.description" class="meta-card meta-header-card" shadow="never">
      <div class="meta-header">
        <i v-if="meta.icon" :class="['bi', meta.icon, 'meta-header__icon']"></i>
        <div class="meta-header__body">
          <div class="meta-header__title">{{ meta.title }}</div>
          <div class="meta-header__desc">{{ meta.description }}</div>
        </div>
      </div>
    </el-card>

    <el-card v-for="group in groupedFields" :key="group.name" class="meta-card" shadow="never">
      <div v-if="group.name" class="section-title">{{ group.name }}</div>
      <div
        v-for="field in group.fields"
        :key="field.key"
        class="field-row"
      >
        <div class="field-row__label">
          <span class="field-row__label-title">{{ field.label }}</span>
          <span v-if="field.description" class="field-row__label-desc">{{ field.description }}</span>
        </div>

        <div class="field-row__control">
          <!-- switch -->
          <el-switch
            v-if="field.type === 'switch'"
            :model-value="values[field.key] === 'true'"
            @change="(v: boolean) => onSwitchChange(field, v)"
          />

          <!-- number -->
          <div v-else-if="field.type === 'number'" class="input-with-unit">
            <el-input-number
              :model-value="Number(values[field.key])"
              :min="field.min"
              :max="field.max"
              :placeholder="field.placeholder"
              controls-position="right"
              class="number-input"
              @update:model-value="(v: number) => onNumberChange(field, v ?? 0)"
            />
            <span v-if="field.unit" class="unit-label">{{ field.unit }}</span>
          </div>

          <!-- slider -->
          <div v-else-if="field.type === 'slider'" class="slider-wrap">
            <el-slider
              :model-value="Number(values[field.key])"
              :min="field.min"
              :max="field.max"
              :step="field.step"
              show-input
              class="meta-slider"
              @update:model-value="(v: number) => onSliderChange(field, v)"
            />
            <span v-if="field.unit" class="unit-label">{{ field.unit }}</span>
          </div>

          <!-- select -->
          <el-select
            v-else-if="field.type === 'select'"
            :model-value="values[field.key]"
            class="u-select"
            @update:model-value="(v: string) => onFieldChange(field, v)"
          >
            <el-option
              v-for="opt in field.options"
              :key="opt.value"
              :value="opt.value"
              :label="opt.label"
            />
          </el-select>

          <!-- chips -->
          <el-radio-group
            v-else-if="field.type === 'chips'"
            :model-value="values[field.key]"
            class="chip-group"
            @update:model-value="(v: string) => onChipsChange(field, v)"
          >
            <el-radio-button
              v-for="opt in field.options"
              :key="opt.value"
              :value="opt.value"
              :title="opt.description"
            >
              {{ opt.label }}
            </el-radio-button>
          </el-radio-group>

          <!-- password -->
          <el-input
            v-else-if="field.type === 'password'"
            :model-value="values[field.key]"
            type="password"
            show-password
            :placeholder="field.placeholder"
            @update:model-value="(v: string) => onFieldChange(field, String(v))"
          />

          <!-- textarea -->
          <el-input
            v-else-if="field.type === 'textarea'"
            type="textarea"
            :model-value="values[field.key]"
            :placeholder="field.placeholder"
            :spellcheck="false"
            class="u-textarea"
            @update:model-value="(v: string) => onFieldChange(field, String(v))"
          />

          <!-- text 默认 -->
          <el-input
            v-else
            :model-value="values[field.key]"
            type="text"
            :placeholder="field.placeholder"
            @update:model-value="(v: string) => onFieldChange(field, String(v))"
          />
        </div>
      </div>
    </el-card>

    <div class="meta-actions">
      <el-button type="primary" :loading="saving" @click="saveNow">
        <i class="bi bi-check-lg"></i>
        <span>{{ saving ? '保存中...' : '保存' }}</span>
      </el-button>
    </div>
  </div>
</template>

<style scoped>
.meta-section {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
  overflow-y: auto;
  min-height: 0;
}

.meta-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--r-lg);
}

.meta-card :deep(.el-card__body) {
  padding: var(--sp-5);
}

.meta-header-card {
  background: var(--bg-soft);
}

.meta-header {
  display: flex;
  gap: var(--sp-4);
  align-items: flex-start;
}

.meta-header__icon {
  font-size: var(--fs-2xl);
  color: var(--accent-primary);
  flex-shrink: 0;
  margin-top: 2px;
}

.meta-header__title {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.meta-header__desc {
  font-size: var(--fs-sm);
  color: var(--text-muted);
  margin-top: 2px;
  line-height: var(--lh-snug);
}

.field-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-4);
  padding: var(--sp-3) 0;
}

.field-row + .field-row {
  border-top: 1px solid var(--border-color);
}

.field-row__label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.field-row__label-title {
  font-size: var(--fs-base);
  font-weight: var(--fw-medium);
  color: var(--text-primary);
}

.field-row__label-desc {
  font-size: var(--fs-sm);
  color: var(--text-muted);
  line-height: var(--lh-snug);
}

.field-row__control {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}

.input-with-unit {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}

.number-input {
  width: 140px;
}

.slider-wrap {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  min-width: 260px;
}

.meta-slider {
  flex: 1;
  min-width: 180px;
}

.unit-label {
  font-size: var(--fs-base);
  color: var(--text-secondary);
  font-weight: var(--fw-medium);
  min-width: 44px;
}

.u-select {
  min-width: 160px;
}

.u-textarea {
  width: 280px;
}

.u-textarea :deep(.el-textarea__inner) {
  min-height: 80px;
  font-family: var(--font-mono);
  background: var(--bg-soft);
}

.error-bar {
  background: var(--status-danger-bg);
  border-color: var(--status-danger);
}

.error-text {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  color: var(--status-danger);
  font-size: var(--fs-base);
}

.meta-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--sp-3);
}

.loading-text {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  color: var(--text-muted);
}

.section-title {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  margin-bottom: var(--sp-3);
}
</style>
