<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useConnect } from "../../composables/useConnect";
import type { JoinField } from "../../lib/api/types";

/**
 * 成员模式：直接选适配器 + 按 join_fields 填字段。
 * 若适配器声明了 password 相位的字段，首步提交后再问密码（"有密码还要问"）。
 */
const { t } = useI18n();
const {
  adapters,
  selectedId,
  joinFields,
  form,
  busy,
  canJoin,
  loadAdapters,
  selectAdapter,
  join,
} = useConnect();

const showPassword = ref(false);

const TRUST: Record<string, { label: string; cls: string }> = {
  official: { label: t("connect.trust.official"), cls: "trust--official" },
  verified: { label: t("connect.trust.verified"), cls: "trust--verified" },
  unsigned: { label: t("connect.trust.unsigned"), cls: "trust--unsigned" },
  blocked: { label: t("connect.trust.blocked"), cls: "trust--blocked" },
};

const primaryFields = computed<JoinField[]>(() =>
  joinFields.value.filter((f) => f.phase !== "password"),
);
const passwordFields = computed<JoinField[]>(() =>
  joinFields.value.filter((f) => f.phase === "password"),
);
const hasPasswordStep = computed(() => passwordFields.value.length > 0);

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

function onSubmit() {
  if (!selectedId.value || !canJoin.value || busy.value) return;
  // 有密码相位：先展示密码字段，再提交。
  if (hasPasswordStep.value && !showPassword.value) {
    showPassword.value = true;
    return;
  }
  void join(selectedId.value, { ...form.value });
}

onMounted(() => void loadAdapters());
</script>

<template>
  <div class="member scroll-area">
    <p class="hint">{{ t("connect.memberModeNote") }}</p>

    <div v-if="adapters.length === 0" class="card member__empty">
      <i class="material-symbols-rounded">hub</i>
      <p>{{ t("connect.noAdapters") }}</p>
    </div>

    <template v-else>
      <!-- 适配器选择 -->
      <div class="member__adapters">
        <m3e-card
          v-for="a in adapters"
          :key="a.pluginId"
          variant="outlined"
          class="adapter"
          :class="{ 'adapter--active': a.pluginId === selectedId }"
          @click="selectAdapter(a.pluginId)"
        >
          <div class="adapter__body">
            <span class="adapter__name">{{ a.name }}</span>
            <span class="adapter__trust" :class="TRUST[a.trust]?.cls">
              {{ TRUST[a.trust]?.label ?? a.trust }}
            </span>
          </div>
        </m3e-card>
      </div>

      <!-- 动态字段 -->
      <div v-if="selectedId" class="card member__form">
        <m3e-form-field
          v-for="f in primaryFields"
          :key="f.key"
          variant="outlined"
          class="field"
        >
          <label slot="label">{{ fieldLabel(f) }}</label>
          <input
            slot="input"
            v-model="form[f.key]"
            :type="f.type === 'password' ? 'password' : 'text'"
            :placeholder="f.placeholder"
          />
        </m3e-form-field>

        <template v-if="showPassword">
          <p class="member__pw-hint">
            <i class="material-symbols-rounded">lock</i>
            {{ t("connect.needPassword") }}
          </p>
          <m3e-form-field
            v-for="f in passwordFields"
            :key="f.key"
            variant="outlined"
            class="field"
          >
            <label slot="label">{{ fieldLabel(f) }}</label>
            <input
              slot="input"
              v-model="form[f.key]"
              :type="f.type === 'password' ? 'password' : 'text'"
              :placeholder="f.placeholder"
            />
          </m3e-form-field>
        </template>

        <m3e-button
          variant="filled"
          class="member__submit"
          :disabled="!canJoin || busy"
          @click="onSubmit"
        >
          <m3e-icon slot="icon" name="login" />
          {{ hasPasswordStep && !showPassword ? t("connect.dialog.continue") : t("connect.startConnect") }}
        </m3e-button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.member {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  max-width: 520px;
  margin: 0 auto;
  width: 100%;
}

.card {
  background: var(--surface-container);
  border-radius: var(--radius-lg);
  padding: var(--sp-4);
  box-shadow: var(--shadow-1);
}
.member__empty {
  text-align: center;
  color: var(--text-muted);
}
.member__empty i {
  font-size: 40px;
  opacity: 0.6;
}

.member__adapters {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
.adapter {
  cursor: pointer;
}
.adapter--active {
  outline: 2px solid var(--primary);
}
.adapter__body {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--sp-3);
}
.adapter__name {
  font-size: var(--fs-md);
  font-weight: var(--fw-semibold);
}
.adapter__trust {
  font-size: var(--fs-xs);
  padding: 2px 8px;
  border-radius: 999px;
}
.trust--official {
  background: var(--primary-container);
  color: var(--on-primary-container);
}
.trust--verified {
  background: var(--tertiary-container, #eaddff);
  color: var(--on-tertiary-container, #2a1a5e);
}
.trust--unsigned {
  background: var(--surface-variant);
  color: var(--text-muted);
}
.trust--blocked {
  background: var(--error-container, #f9dedc);
  color: var(--on-error-container, #410e0b);
}

.member__form {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
.field {
  width: 100%;
}
.member__pw-hint {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}
.member__pw-hint i {
  font-size: 16px;
}
.member__submit {
  align-self: flex-start;
}
</style>
