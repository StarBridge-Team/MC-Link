<script setup lang="ts">
import { computed, inject } from "vue";

defineProps<{
  showToast: (msg: string) => void;
  playerName: string;
}>();

defineEmits<{
  nameChange: [name: string];
}>();

const navigateTo = inject<(icon: string, text?: string) => void>('navigateTo')!;

const greeting = computed(() => {
  const h = new Date().getHours();
  if (h >= 6 && h < 12) return "早上好";
  if (h >= 12 && h < 14) return "中午好";
  if (h >= 14 && h < 18) return "下午好";
  return "晚上好";
});

function openAccount() {
  navigateTo('setting', 'personalization');
}

const hints = [
  { icon: "bi-hdd-stack", role: "房主", text: "请先在 Minecraft 中开启局域网联机，然后前往", em: "联机", tail: "页面创建房间" },
  { icon: "bi-person-plus", role: "成员", text: "前往", em: "联机", tail: "页面，输入房主分享的房间名和密码即可加入" },
  { icon: "bi-plug", role: "适配器", text: "在", em: "适配器", tail: "页面下载陶瓦联机以获得更多联机方式" },
];
</script>

<template>
  <div class="home-root">
    <div class="greeting-header">
      <el-avatar :size="48" class="greeting-avatar" @click="openAccount">
        <i class="bi bi-person-circle"></i>
      </el-avatar>
      <div class="greeting-text">
        <span>{{ greeting }}</span>，
        <span class="greeting-name">{{ playerName }}</span>，
        <span>我们要做点什么？</span>
      </div>
    </div>

    <div class="home-hint">
      <el-card
        v-for="(h, i) in hints"
        :key="i"
        class="hint-card"
        shadow="never"
        :body-style="{ padding: 'var(--sp-4) var(--sp-5)', display: 'flex', alignItems: 'flex-start', gap: 'var(--sp-4)' }"
      >
        <div class="hint-icon"><i :class="['bi', h.icon]"></i></div>
        <div class="hint-content">
          <strong>{{ h.role }}</strong>：{{ h.text }}<em>{{ h.em }}</em>{{ h.tail }}
        </div>
      </el-card>
    </div>
  </div>
</template>

<style scoped>
.home-root {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
}

.greeting-header {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  padding: 0 0 var(--sp-5);
}

.greeting-avatar {
  background: var(--accent-primary);
  cursor: pointer;
  flex-shrink: 0;
  font-size: 26px;
  color: #fff;
}

.greeting-avatar:hover {
  opacity: 0.85;
}

.greeting-text {
  font-size: var(--fs-xl);
  font-weight: var(--fw-medium);
  color: var(--text-primary);
  line-height: var(--lh-snug);
}

.greeting-name {
  color: var(--accent-primary);
  font-weight: var(--fw-semibold);
}

.home-hint {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.hint-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--r-lg);
}

.hint-icon {
  width: 36px;
  height: 36px;
  border-radius: var(--r-md);
  background: var(--accent-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--fs-xl);
  color: #fff;
  flex-shrink: 0;
  margin-top: 2px;
}

.hint-content {
  font-size: var(--fs-base);
  line-height: var(--lh-normal);
  color: var(--text-primary);
}

.hint-content strong {
  color: var(--text-primary);
  font-weight: var(--fw-semibold);
}

.hint-content em {
  font-style: normal;
  color: var(--accent-primary);
  font-weight: var(--fw-medium);
}
</style>
