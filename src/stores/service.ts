import { ref, computed, onUnmounted, watch } from 'vue'
import { queryServiceStatus } from '@/bridge/service'
import { useConfigStore } from './config'
import type { ServiceStatus } from '@/types'

const serviceStatus = ref<ServiceStatus>({ state: 'unknown' })
let pollTimer: ReturnType<typeof setInterval> | null = null
let refCount = 0

// 新增：计时相关
const runStartTime = ref<number | null>(null)
const elapsedText = ref('0:00:00')
let tickTimer: ReturnType<typeof setInterval> | null = null

function startTick() {
  runStartTime.value = Date.now()
  tickTimer = setInterval(() => {
    const secs = Math.floor((Date.now() - runStartTime.value!) / 1000)
    const h = Math.floor(secs / 3600)
    const m = Math.floor((secs % 3600) / 60)
    const s = secs % 60
    elapsedText.value = `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
  }, 1000)
}

function stopTick() {
  if (tickTimer) {
    clearInterval(tickTimer)
    tickTimer = null
  }
  runStartTime.value = null
  elapsedText.value = '0:00:00'
}

// 监听状态变化，控制计时器
watch(
  () => serviceStatus.value.state,
  (newState, oldState) => {
    if (newState === 'running' && oldState !== 'running') {
      startTick()
    } else if (newState !== 'running') {
      stopTick()
    }
  },
  { immediate: true }
)

let firstPollResolve: (() => void) | null = null
const firstPollReady = new Promise<void>((resolve) => { firstPollResolve = resolve })

async function poll() {
  const { serviceName } = useConfigStore()
  try {
    serviceStatus.value = await queryServiceStatus(serviceName.value)
  } catch {
    serviceStatus.value = { state: 'unknown' }
  }
  if (firstPollResolve) {
    firstPollResolve()
    firstPollResolve = null
  }
}

export function useServiceStore() {
  if (refCount === 0) {
    poll()
    pollTimer = setInterval(poll, 2000)
  }
  refCount++

  onUnmounted(() => {
    refCount--
    if (refCount === 0 && pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  })

  const statusText = computed(() => {
    const map: Record<string, string> = {
      running: `已运行 ${elapsedText.value}`,  // ← 改这里
      stopped: '服务已安装 核心未运行',
      starting: '启动中',
      stopping: '停止中',
      not_installed: '请先在设置中安装服务',
      unknown: '未知',
    }
    return map[serviceStatus.value.state] || '未知'
  })

  return {
    serviceStatus,
    statusText,
    ready: firstPollReady,
    refresh: poll,
  }
}
