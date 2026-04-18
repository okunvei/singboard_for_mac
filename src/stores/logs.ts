import { ref, computed, onUnmounted, watch } from 'vue'
import { createClashWS } from '@/api/websocket'
import { fetchConfig } from '@/api'
import { useConfigStore } from './config'
import { appVisible } from './appVisible'
import type { LogEntry } from '@/types'
import type ReconnectingWebSocket from 'reconnecting-websocket'

const MAX_LOGS = 5000

const logs = ref<LogEntry[]>([])
const logLevel = ref('')
const paused = ref(false)
const filterText = ref('')

let ws: ReconnectingWebSocket | null = null
let refCount = 0

const filteredLogs = computed(() => {
  if (!filterText.value) return logs.value
  const q = filterText.value.toLowerCase()
  return logs.value.filter((l) => l.payload.toLowerCase().includes(q))
})

export function useLogsStore() {
  const { activeClashApiId } = useConfigStore()

  async function start() {
    if (ws) ws.close()
    logs.value = []
    if (!logLevel.value) {
      try {
        const { data } = await fetchConfig()
        logLevel.value = data['log-level'] || 'info'
      } catch {
        logLevel.value = 'info'
      }
    }
    ws = createClashWS('/logs', (data: LogEntry) => {
      if (paused.value) return
      data.time = new Date().toLocaleTimeString()
      logs.value.push(data)
      if (logs.value.length > MAX_LOGS) {
        logs.value = logs.value.slice(-MAX_LOGS)
      }
    }, { level: logLevel.value })
  }

  function stop() {
    ws?.close()
    ws = null
  }

  function clear() {
    logs.value = []
  }

  function changeLevel(level: string) {
    logLevel.value = level
    start()
  }

  refCount++
  const unwatchApi = watch(
    () => activeClashApiId.value,
    () => {
      if (ws) {
        logLevel.value = ''
        start()
      }
    },
  )
  const unwatchVisible = watch(appVisible, (visible) => {
    if (visible) {
      if (!ws && refCount > 0) start()
    } else {
      if (ws) stop()
    }
  })
  onUnmounted(() => {
    unwatchApi()
    unwatchVisible()
    refCount--
    if (refCount === 0) stop()
  })

  return {
    logs,
    filteredLogs,
    logLevel,
    paused,
    filterText,
    start,
    stop,
    clear,
    changeLevel,
  }
}
