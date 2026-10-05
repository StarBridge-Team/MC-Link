<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useConnect } from "../../composables/useConnect";
import type { JoinField } from "../../lib/api/types";

/**
 * 成员模式：选适配器 + 按 `join_fields` 填字段。
 * 若适配器声明了 password 相位的字段，首步提交后再问密码（"有密码还要问"）。
 *
 * 交互元素一律用 `@m3e/web` 组件（selection-list / list-option / card / badge /
 * form-field / button / icon），不手写等价结构。
 */
const { t } = useI18n();
const {
  adapters,
  selectedId,
  formFields,
  form,
  busy,
  canJoin,
  loadAdapters,
  selectAdapter,
  join,
} = useConnect();

const showPassword = ref(false);

const TRUST_LABEL: Record<string, string> = {
  official: t("connect.trust.official"),
  verified: t("connect.trust.verified"),
  unsigned: t("connect.trust.unsigned"),
  blocked: t("connect.trust.blocked"),
};

/** 信任度 → 徽章配色类（配色写在 <style>，不改组件结构）。 */
function trustClass(trust: string): string {
  return `trust--${trust}`;
}

// `formFields` 已剔除邀请码字段——它由页面顶部的搜索框录入（见 useConnect）。
const primaryFields = computed<JoinField[]>(() =>
  formFields.value.filter((f) => f.phase !== "password"),
);
const passwordFields = computed<JoinField[]>(() =>
  formFields.value.filter((f) => f.phase === "password"),
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

    <m3e-card v-if="adapters.length === 0" variant="elevated">
      <div slot="content" class="member__empty">
        <m3e-icon name="hub" />
        <p>{{ t("connect.noAdapters") }}</p>
      </div>
    </m3e-card>

    <template v-else>
      <!-- 适配器选择 -->
      <m3e-selection-list class="member__adapters">
        <m3e-list-option
          v-for="a in adapters"
          :key="a.pluginId"
          :selected="a.pluginId === selectedId"
          @click="selectAdapter(a.pluginId)"
        >
          {{ a.name }}
          <m3e-badge slot="trailing" class="trust" :class="trustClass(a.trust)">
            {{ TRUST_LABEL[a.trust] ?? a.trust }}
          </m3e-badge>
        </m3e-list-option>
      </m3e-selection-list>

      <!-- 动态字段：控件放 m3e-form-field 的**默认** slot -->
      <m3e-card v-if="selectedId" variant="elevated">
        <div slot="content" class="member__form">
          <m3e-form-field
            v-for="f in primaryFields"
            :key="f.key"
            variant="outlined"
            class="field"
          >
            <label slot="label">{{ fieldLabel(f) }}</label>
            <input
              v-model="form[f.key]"
              :type="f.type === 'password' ? 'password' : 'text'"
              :placeholder="f.placeholder"
            />
          </m3e-form-field>

          <template v-if="showPassword">
            <p class="member__pw-hint">
              <m3e-icon name="lock" />
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
            {{
              hasPasswordStep && !showPassword
                ? t("connect.dialog.continue")
                : t("connect.startConnect")
            }}
          </m3e-button>
        </div>
      </m3e-card>
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

.member__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-4);
  padding: var(--sp-6) 0;
  text-align: center;
  color: var(--text-muted);
}
.member__empty m3e-icon {
  font-size: 40px;
  opacity: 0.6;
}

.member__adapters {
  width: 100%;
}

/* 信任度徽章：只覆盖配色，形状/尺寸交给组件。 */
.trust {
  --m3e-badge-medium-font-size: var(--fs-xs);
}
.trust--official {
  --m3e-badge-container-color: var(--primary-container);
  --m3e-badge-color: var(--on-primary-container);
}
.trust--verified {
  --m3e-badge-container-color: var(--tertiary-container, #eaddff);
  --m3e-badge-color: var(--on-tertiary-container, #2a1a5e);
}
.trust--unsigned {
  --m3e-badge-container-color: var(--surface-variant);
  --m3e-badge-color: var(--text-muted);
}
.trust--blocked {
  --m3e-badge-container-color: var(--error-container, #f9dedc);
  --m3e-badge-color: var(--on-error-container, #410e0b);
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
.member__pw-hint m3e-icon {
  font-size: 16px;
}
.member__submit {
  align-self: flex-start;
}
</style>
