<script setup lang="ts">
defineProps<{
  ready: boolean;
  version: string;
  step: string;
}>();
</script>

<template>
  <Transition name="splash-fade">
    <div v-if="!ready" class="splash-overlay">
      <div class="splash-card">
        <div class="splash-main">
          <div class="splash-icon">
            <img src="/mc-link-icon.png" alt="MC Link">
          </div>
          <div class="splash-info">
            <div class="splash-title-row">
              <div class="splash-title">MC Link</div>
              <div class="splash-version">{{ version }}</div>
            </div>
            <div class="splash-subtitle">我的世界联机工具</div>
            <div class="splash-progress">
              <div class="splash-spinner"></div>
              <span class="splash-step">{{ step }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.splash-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--window-wrapper-bg, #ffffff);
  background-image: var(--window-wrapper-bg-image, none);
  background-size: var(--window-wrapper-bg-fit, cover);
  background-position: center;
  background-repeat: no-repeat;
}

.splash-card {
  max-width: min(88vw, 600px);
  box-sizing: border-box;
}

.splash-main {
  display: flex;
  align-items: flex-start;
  gap: 24px;
}

.splash-icon {
  flex-shrink: 0;
  margin-top: 2px;
}

.splash-icon img {
  width: 80px;
  height: 80px;
  display: block;
}

.splash-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.splash-title-row {
  display: flex;
  align-items: baseline;
  gap: 12px;
  flex-wrap: wrap;
}

.splash-title {
  font-size: 32px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: 0.5px;
  line-height: 1.15;
}

.splash-version {
  font-size: 15px;
  color: var(--text-secondary);
  font-weight: 500;
}

.splash-subtitle {
  font-size: 15px;
  color: var(--text-secondary);
  margin-top: 6px;
}

.splash-progress {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 18px;
}

.splash-spinner {
  width: 20px;
  height: 20px;
  border: 2.5px solid var(--border-color, #e0e0e0);
  border-top-color: var(--accent-primary, #0066cc);
  border-radius: 50%;
  flex-shrink: 0;
  animation: splash-spin 0.6s linear infinite;
}

@keyframes splash-spin {
  to { transform: rotate(360deg); }
}

.splash-step {
  font-size: 14px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.splash-fade-enter-active,
.splash-fade-leave-active {
  transition: opacity 0.35s ease;
}

.splash-fade-enter-from,
.splash-fade-leave-to {
  opacity: 0;
}
</style>