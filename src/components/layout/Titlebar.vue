<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { onMounted, onUnmounted } from 'vue'

const appWindow = getCurrentWindow()

async function minimize() {
  await appWindow.minimize()
}
async function toggleMaximize() {
  await appWindow.toggleMaximize()
}
async function close() {
  await appWindow.close()
}

// Cmd+W 关闭窗口（macOS 标准快捷键）
function handleKeydown(e: KeyboardEvent) {
  if (e.metaKey && e.key === 'w') {
    e.preventDefault()
    close()
  }
}

onMounted(() => window.addEventListener('keydown', handleKeydown))
onUnmounted(() => window.removeEventListener('keydown', handleKeydown))
</script>

<template>
  <div
    data-tauri-drag-region
    class="flex items-center h-10 bg-base-200 border-b border-base-300 select-none px-3"
  >
    <!-- macOS 风格：红绿灯在左侧 -->
    <div class="flex items-center gap-1.5 mr-3">
      <!-- 关闭（红） -->
      <button
        class="w-3 h-3 rounded-full bg-[#ff5f57] hover:brightness-90 flex items-center justify-center group transition-all"
        @click="close"
        title="关闭"
      >
        <svg class="w-1.5 h-1.5 opacity-0 group-hover:opacity-100 text-[#820005]" viewBox="0 0 8 8" fill="none">
          <line stroke="currentColor" stroke-width="1.2" x1="1.5" y1="1.5" x2="6.5" y2="6.5"/>
          <line stroke="currentColor" stroke-width="1.2" x1="6.5" y1="1.5" x2="1.5" y2="6.5"/>
        </svg>
      </button>
      <!-- 最小化（黄） -->
      <button
        class="w-3 h-3 rounded-full bg-[#ffbd2e] hover:brightness-90 flex items-center justify-center group transition-all"
        @click="minimize"
        title="最小化"
      >
        <svg class="w-1.5 h-1.5 opacity-0 group-hover:opacity-100 text-[#7e4900]" viewBox="0 0 8 8" fill="none">
          <line stroke="currentColor" stroke-width="1.2" x1="1" y1="4" x2="7" y2="4"/>
        </svg>
      </button>
      <!-- 最大化（绿） -->
      <button
        class="w-3 h-3 rounded-full bg-[#28c840] hover:brightness-90 flex items-center justify-center group transition-all"
        @click="toggleMaximize"
        title="最大化"
      >
        <svg class="w-1.5 h-1.5 opacity-0 group-hover:opacity-100 text-[#006500]" viewBox="0 0 8 8" fill="none">
          <path stroke="currentColor" stroke-width="1" d="M1.5 6.5 L6.5 1.5 M4.5 1.5 L6.5 1.5 L6.5 3.5"/>
          <path stroke="currentColor" stroke-width="1" d="M3.5 6.5 L1.5 6.5 L1.5 4.5"/>
        </svg>
      </button>
    </div>

    <!-- 标题居中 -->
    <div class="flex-1 flex items-center justify-center" data-tauri-drag-region>
      <span class="text-sm font-semibold text-base-content/70">Singboard</span>
    </div>

    <!-- 右侧占位，保持标题视觉居中 -->
    <div class="w-[54px]"></div>
  </div>
</template>
