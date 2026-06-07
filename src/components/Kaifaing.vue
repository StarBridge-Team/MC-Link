<script setup lang="ts">
defineProps<{
  title?: string;
  description?: string;
}>();
</script>

<template>
  <div class="kaifaing">
    <div class="kaifaing-icon">
      <i class="bi bi-code-slash" style="font-size: 48px;"></i>
    </div>
    <h2 class="kaifaing-title">{{ title || '开发中' }}</h2>
    <p class="kaifaing-desc">{{ description || '此功能正在开发中，敬请期待' }}</p>
    <div class="kaifaing-indicators">
      <span class="dot"></span>
      <span class="dot"></span>
      <span class="dot"></span>
    </div>
  </div>
</template>

<style scoped>
.kaifaing {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 80px 20px;
  text-align: center;
  animation: fade-up-in 500ms cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
}

.kaifaing-icon {
  color: var(--accent-primary);
  opacity: 0.6;
  margin-bottom: 24px;
  animation: pulse-icon 2.5s ease-in-out infinite;
}

@keyframes pulse-icon {
  0%, 100% { opacity: 0.6; transform: scale(1); }
  50% { opacity: 0.9; transform: scale(1.08); }
}

.kaifaing-title {
  font-size: 28px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0 0 12px;
  letter-spacing: 2px;
}

.kaifaing-desc {
  font-size: 15px;
  color: var(--text-muted);
  margin: 0 0 32px;
  line-height: 1.6;
  max-width: 300px;
}

.kaifaing-indicators {
  display: flex;
  gap: 10px;
}

.dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--accent-primary);
  opacity: 0.3;
  animation: dot-bounce 1.4s ease-in-out infinite both;
}

.dot:nth-child(1) { animation-delay: 0s; }
.dot:nth-child(2) { animation-delay: 0.2s; }
.dot:nth-child(3) { animation-delay: 0.4s; }

@keyframes dot-bounce {
  0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
  40% { opacity: 0.9; transform: scale(1.2); }
}
</style>
