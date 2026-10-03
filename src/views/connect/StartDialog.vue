<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useConnect } from "../../composables/useConnect";
import type { JoinField, LocalGame } from "../../lib/api/types";

/**
 * 房主弹窗：先选适配器（>1 个时），再按该适配器的 host_fields 出字段。
 * generated 字段（如房间码）由适配器生成、不收集输入，只显示说明；其余按类型出输入框。
 *
 * 弹窗用 `m3e-dialog`（自带遮罩、焦点陷阱、关闭按钮与 Esc 处理），不手写遮罩层。
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

const inputFields = computed<JoinField[]>(() => hostFields.value.filter((f) => !f.generated));

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
  <m3e-dialog
    v-if="open"
    open
    dismissible
    class="start-dialog"
    @closed="emit('close')"
  >
    <span slot="header">{{ t("connect.startConnect") }}</span>

    <!-- 步骤一：选适配器 -->
    <div v-if="step === 'adapter'" class="dialog__body">
      <p class="hint">{{ t("connect.selectAdapter") }}</p>
      <m3e-selection-list>
        <m3e-list-option
          v-for="a in adapters"
          :key="a.pluginId"
          :selected="a.pluginId === selectedId"
          @click="pickAdapter(a.pluginId)"
        >
          {{ a.name }}
        </m3e-list-option>
      </m3e-selection-list>
    </div>

    <!-- 步骤二：按 host_fields 出字段 -->
    <div v-else class="dialog__body">
      <p v-if="hostFields.length === 0" class="hint">{{ t("connect.hostModeNote") }}</p>

      <template v-for="f in hostFields" :key="f.key">
        <!-- 生成类字段：只显示说明，不收集输入 -->
        <p v-if="f.generated" class="dialog__generated">
          <m3e-icon name="auto_awesome" />
          {{ fieldLabel(f) }} {{ t("connect.field.generated") }}
          <span v-if="f.note" class="hint">· {{ f.note }}</span>
        </p>
        <!-- 其余：按类型出输入框；控件放默认 slot -->
        <m3e-form-field v-else variant="outlined" class="field">
          <label slot="label">{{ fieldLabel(f) }}</label>
          <input
            v-model="form[f.key]"
            :type="f.type === 'password' ? 'password' : 'text'"
            :placeholder="f.placeholder"
          />
        </m3e-form-field>
      </template>
    </div>

    <div slot="actions" end class="dialog__actions">
      <m3e-button variant="text" @click="emit('close')">
        {{ t("connect.dialog.cancel") }}
      </m3e-button>
      <m3e-button variant="filled" @click="confirm">
        {{ t("connect.startConnect") }}
      </m3e-button>
    </div>
  </m3e-dialog>
</template>

<style scoped>
.dialog__body {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

.dialog__generated {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}
.dialog__generated m3e-icon {
  font-size: 16px;
}

.field {
  width: 100%;
}

.dialog__actions {
  display: flex;
  gap: var(--sp-2);
}
</style>
