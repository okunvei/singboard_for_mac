<script setup lang="ts">
import { useToastStore } from '@/stores/toast'

const { toasts, removeToast, pauseToast, resumeToast } = useToastStore()

function toastClass(type?: 'error' | 'info') {
  if (type === 'info') {
    return 'bg-sky-500/95 text-white'
  }
  return 'bg-rose-500/95 text-white'
}
</script>

<template>
  <div class="fixed right-4 z-[60] pointer-events-none space-y-2" style="top: 3rem">
    <div
      v-for="toast in toasts"
      :key="toast.id"
      class="pointer-events-auto min-w-[260px] max-w-[480px] rounded-xl shadow-lg border border-white/20 backdrop-blur px-3 py-2"
      :class="toastClass(toast.type)"
      @mouseenter="pauseToast(toast.id)"
      @mouseleave="resumeToast(toast.id)"
    >
      <div class="flex items-start gap-2">
        <span class="text-sm leading-none mt-0.5">◔</span>
        <p class="text-sm leading-snug whitespace-pre-line flex-1">{{ toast.message }}</p>
        <button class="text-sm leading-none opacity-80 hover:opacity-100" @click="removeToast(toast.id)">×</button>
      </div>
      <div class="mt-2 h-1 rounded-full bg-white/30 overflow-hidden">
        <div
          class="h-full bg-white/90 transition-[width] duration-75 ease-linear"
          :style="{ width: `${Math.max(0, (toast.remainingMs / toast.duration) * 100)}%` }"
        />
      </div>
    </div>
  </div>
</template>
