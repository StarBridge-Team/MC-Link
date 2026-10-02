<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import SettingCard from "../../components/ui/SettingCard.vue";
import { showError, showSuccess } from "../../composables/useToast";
import { getSetting, saveSetting } from "../../lib/api/config";
import { getSettingManifest, getSettingMeta } from "../../lib/api/settingMeta";
import type { FieldMeta, SettingMeta } from "../../lib/api/types";
import { parseFlatYaml, writeFlatYaml } from "../../lib/yamlLite";

/**
 * 资源服务器下发的「更多设置」分区。
 *
 * # 为什么按元配置渲染
 *
 * 分区与字段来自 `settings/manifest.json` 与 `settings/meta/<section>`，
 * 因此服务器加一个字段、客户端不必改代码。这里**不硬编码任何分区名与字段名**。
 *
 * # 为什么读写走"整段 YAML 文本"
 *
 * 后端对这些分区没有结构定义，只提供 `get_setting` / `save_setting` 的原文通道。
 * 这里用 `yamlLite` 做行级改写，保证用户手写的注释与未识别的键不被覆盖。
 * 没有可用分区时整张卡片不渲染，而不是显示一个空壳。
 */
const { t } = useI18n();

interface LoadedSection {
  meta: SettingMeta;
  values: Record<string, string>;
}

const sections = ref<LoadedSection[]>([]);
const loading = ref(true);
const saving = ref<string | null>(null);

/**
 * 已由客户端原生界面渲染的分区 id。服务器若也下发这些分区，会与原生界面重复
 * （表现为同一页冒出第二张「个性化设置」卡片），因此这里跳过。
 */
const NATIVE_SECTION_IDS = new Set(["personalization"]);

onMounted(async () => {
  try {
    const manifest = await getSettingManifest();
    const loaded: LoadedSection[] = [];
    for (const section of manifest.sections) {
      if (NATIVE_SECTION_IDS.has(section.id)) continue;
      // 单个分区失败不影响其它分区：服务器可能只配了一半。
      try {
        const [meta, content] = await Promise.all([
          getSettingMeta(section.id),
          getSetting(section.id),
        ]);
        loaded.push({ meta, values: parseFlatYaml(content) });
      } catch (e) {
        console.warn(`[settings] 跳过分区 ${section.id}:`, e);
      }
    }
    sections.value = loaded;
  } catch (e) {
    console.warn("[settings] 拉取设置清单失败:", e);
    sections.value = [];
  } finally {
    loading.value = false;
  }
});

/** 字段在界面上的当前值（缺失时落到元配置里的 default）。 */
function valueOf(field: FieldMeta): string {
  const section = sections.value.find((s) => s.meta.fields.includes(field));
  return section?.values[field.key] ?? field.default ?? "";
}

function setValue(field: FieldMeta, value: string) {
  const section = sections.value.find((s) => s.meta.fields.includes(field));
  if (section) section.values[field.key] = value;
}

async function save(section: LoadedSection) {
  saving.value = section.meta.section;
  try {
    const current = await getSetting(section.meta.section);
    await saveSetting(section.meta.section, writeFlatYaml(current, section.values));
    showSuccess(t("common.saved"));
  } catch (e) {
    showError(`${t("common.saveFailed")}: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    saving.value = null;
  }
}

function optionsOf(field: FieldMeta) {
  return field.options.map((o) => ({ label: o.label || o.value, value: o.value }));
}

/** m3e-select 的选中值经 change 事件回传。 */
function selectValue(e: Event): string {
  const el = e.target as HTMLElement & { value?: string };
  return el.value ?? "";
}

function isSwitchOn(field: FieldMeta): boolean {
  const raw = valueOf(field).trim().toLowerCase();
  return raw === "true" || raw === "1" || raw === "yes" || raw === "on";
}
</script>

<template>
  <div v-if="!loading && sections.length > 0" class="stagger">
    <SettingCard
      v-for="(section, index) in sections"
      :key="section.meta.section"
      :style="{ '--stagger-index': index }"
      :icon="section.meta.icon || 'tune'"
      :title="section.meta.title || section.meta.section"
      :desc="section.meta.description"
      wide
    >
      <div class="fields">
        <div v-for="field in section.meta.fields" :key="field.key" class="field">
          <div class="field__head">
            <span class="field__label">{{ field.label || field.key }}</span>
            <span v-if="field.unit" class="hint">{{ field.unit }}</span>
          </div>
          <p v-if="field.description" class="hint">{{ field.description }}</p>

          <m3e-switch
            v-if="field.type === 'switch'"
            :checked="isSwitchOn(field)"
            @change="(e: Event) => setValue(field, (e.target as HTMLInputElement).checked ? 'true' : 'false')"
          />
          <m3e-select
            v-else-if="field.type === 'select' || field.type === 'chips'"
            :value="valueOf(field)"
            @change="(e: Event) => setValue(field, selectValue(e))"
          >
            <m3e-option v-for="opt in optionsOf(field)" :key="opt.value" :value="opt.value">
              {{ opt.label }}
            </m3e-option>
          </m3e-select>
          <m3e-form-field v-else-if="field.type === 'textarea'" variant="outlined">
            <textarea
              rows="3"
              :placeholder="field.placeholder"
              :value="valueOf(field)"
              @input="(e: Event) => setValue(field, (e.target as HTMLTextAreaElement).value)"
            />
          </m3e-form-field>
          <m3e-form-field v-else variant="outlined">
            <input
              :type="field.type === 'number' || field.type === 'slider' ? 'number' : field.type === 'password' ? 'password' : 'text'"
              :placeholder="field.placeholder"
              :value="valueOf(field)"
              @input="(e: Event) => setValue(field, (e.target as HTMLInputElement).value)"
            />
          </m3e-form-field>
        </div>
      </div>

      <div class="actions">
        <m3e-button variant="filled" :disabled="saving === section.meta.section" @click="save(section)">
          {{ t("common.save") }}
        </m3e-button>
      </div>
    </SettingCard>
  </div>
</template>

<style scoped>
.fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-4);
}

@media (max-width: 980px) {
  .fields {
    grid-template-columns: 1fr;
  }
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
  min-width: 0;
}

.field__head {
  display: flex;
  align-items: baseline;
  gap: var(--sp-2);
}

.field__label {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--text-secondary);
}

.actions {
  display: flex;
  justify-content: flex-end;
}
</style>
