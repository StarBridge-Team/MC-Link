<script setup lang="ts">
import { ref, watch, nextTick } from "vue"

const props = defineProps<{
  text: string;
}>()

const items = ref<string[]>([props.text])
const sliding = ref(false)
let timer: ReturnType<typeof setTimeout> | null = null

watch(() => props.text, (newVal, oldVal) => {
  if (newVal === oldVal) return
  if (timer) clearTimeout(timer)

  items.value = [oldVal || newVal, newVal]
  sliding.value = true

  nextTick(() => {
    timer = setTimeout(() => {
      sliding.value = false
      items.value = [newVal]
    }, 400)
  })
})
</script>

<template>
  <span class="elevator-mask">
    <span class="elevator-track" :class="{ 'elevator-slide': sliding }">
      <span class="elevator-cell" v-for="(item, i) in items" :key="i">{{ item }}</span>
    </span>
  </span>
</template>

<style scoped>
.elevator-mask {
  overflow: hidden;
  display: inline-block;
  height: 1.2em;
  vertical-align: baseline;
  line-height: 1.2;
  position: relative;
}

.elevator-track {
  display: flex;
  flex-direction: column;
  will-change: transform;
}

.elevator-track.elevator-slide {
  animation: elevator-slide-up 400ms cubic-bezier(0.4, 0, 0.2, 1) forwards;
}

@keyframes elevator-slide-up {
  from {
    transform: translateY(0);
  }
  to {
    transform: translateY(-50%);
  }
}

.elevator-cell {
  height: 1.2em;
  flex-shrink: 0;
  white-space: nowrap;
}
</style>
