<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useConnect } from "../../composables/useConnect";
import type { JoinField, LocalGame } from "../../lib/api/types";

/**
 * 房主弹窗：先选适配器（>1 个时），再按该适配器的 host_fields 出字段。
 * generated 字段（如房间码）由适配器生成、不收集输入，只显示说明；其余按类型出输入框。
 */
const props = defineProps<{
  mode: "host";
  game: LocalGame | null;
  open: boolean;
}>();

const emit = defineEmits<{ (e: "close"): void }>();

const { t } = useI18n();
const { adapters, selectedId, hostFields, loadAdapters, startHost } = useConnect();

const step = ref<"adapter" | "fields">("adapter");
const form = ref<Record<string, string>>({});

const inputFields = computed<JoinField[]>(() =>
  hostFields.value.filter((f) => !f.generated),
);

function fieldLabel(f: JoinField): string {
  const known: Record<string, string> = {
    room_code: t("connect.field.roomCode"),
    network_name: t("connect.field.networkName"),
    key: t("connect.field.key"),
    invitation_code: t("connect.field.invitationCode"),
    password: t("connect.field.password"),
  };
  return known[f.key] ?? f.label ?? f.key;
}

function pickAdapter(id: string) {
  selectedId.value = id;
  step.value = "fields";
}

function reset() {
  step.value = adapters.value.length > 1 ? "adapter" : "fields";
  if (adapters.value.length === 1) selectedId.value = adapters.value[0].pluginId;
  form.value = {};
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      if (adapters.value.length === 0) void loadAdapters();
      reset();
    }
  },
  { immediate: true },
);

function confirm() {
  if (!selectedId.value || !props.game) return;
  const fields: Record<string, unknown> = { game: { port: props.game.port } };
  for (const f of inputFields.value) {
    if (f.key) fields[f.key] = form.value[f.key] ?? "";
  }
  void startHost(selectedId.value, fields);
  emit("close");
}
</script>

<template>
  <div v-if="open" class="dialog-backdrop" @click.self="emit('close')">
    <div class="dialog">
      <header class="dialog__head">
        <h3>{{ t("connect.startConnect") }}</h3>
        <button class="dialog__x" @click="emit('close')">
          <i class="material-symbols-rounded">close</i>
        </button>
      </header>

      <!-- 步骤一：选适配器 -->
      <div v-if="step === 'adapter'" class="dialog__body">
        <p class="hint">{{ t("connect.selectAdapter") }}</p>
        <ul class="adapter-list">
          <li
            v-for="a in adapters"
            :key="a.pluginId"
            class="adapter"
            :class="{ 'adapter--active': a.pluginId === selectedId }"
            @click="pickAdapter(a.pluginId)"
          >
            <span class="adapter__name">{{ a.name }}</span>
          </li>
        </ul>
      </div>

      <!-- 步骤二：按 host_fields 出字段 -->
      <div v-else class="dialog__body">
        <p v-if="hostFields.length === 0" class="hint">{{ t("connect.hostModeNote") }}</p>

        <template v-for="f in hostFields" :key="f.key">
          <!-- 生成类字段：只显示说明，不收集输入 -->
          <p v-if="f.generated" class="dialog__generated">
            <i class="material-symbols-rounded">auto_awesome</i>
            {{ fieldLabel(f) }} {{ t("connect.field.generated") }}
            <span v-if="f.note" class="hint">· {{ f.note }}</span>
          </p>
          <!-- 其余：按类型出输入框 -->
          <m3e-form-field v-else variant="outlined" class="field">
            <label slot="label">{{ fieldLabel(f) }}</label>
            <input
              slot="input"
              v-model="form[f.key]"
              :type="f.type === 'password' ? 'password' : 'text'"
              :placeholder="f.placeholder"
            />
          </m3e-form-field>
        </template>

        <div class="dialog__actions">
          <m3e-button variant="text" @click="emit('close')">
            {{ t("connect.dialog.cancel") }}
          </m3e-button>
          <m3e-button variant="filled" @click="confirm">
            {{ t("connect.startConnect") }}
          </m3e-button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dialog-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
}
.dialog {
  width: min(440px, 92vw);
  max-height: 86vh;
  overflow-y: auto;
  background: var(--surface-container-high, var(--surface-container));
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-3);
  padding: var(--sp-4);
}
.dialog__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--sp-3);
}
.dialog__head h3 {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
}
.dialog__x {
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  display: inline-flex;
}
.dialog__body {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
.adapter-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
.adapter {
  padding: var(--sp-3);
  border-radius: var(--radius-sm);
  background: var(--surface-container);
  cursor: pointer;
}
.adapter--active {
  outline: 2px solid var(--primary);
}
.adapter__name {
  font-size: var(--fs-md);
  font-weight: var(--fw-semibold);
}
.dialog__generated {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}
.dialog__generated i {
  font-size: 16px;
}
.field {
  width: 100%;
}
.dialog__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--sp-2);
  margin-top: var(--sp-2);
}
</style>
