import { ref, onUnmounted, watch } from 'vue'
import { createClashWS } from '@/api/websocket'
import { useConfigStore } from './config'
import { useServiceStore } from './service'
import { appVisible } from './appVisible'
import type { TrafficData, MemoryData } from '@/types'
import type ReconnectingWebSocket from 'reconnecting-websocket'

// 网络信息刷新信号：每次自增触发 NetworkInfo 执行刷新
// 仅在 OverviewPage 可见时由 OverviewPage 或 Sidebar 写入，NetworkInfo 监听
export const networkRefreshSignal = ref(0)

export function triggerNetworkRefresh() {
  networkRefreshSignal.value++
}

// 网络信息"自动刷新"复选框状态，持久化到 localStorage
const AUTO_REFRESH_KEY = 'singboard-network-auto-refresh'
export const autoRefreshEnabled = ref(localStorage.getItem(AUTO_REFRESH_KEY) === 'true')
watch(autoRefreshEnabled, (val) => {
  localStorage.setItem(AUTO_REFRESH_KEY, String(val))
})

export interface HistoryPoint {
  name: number
  value: number
}

const MAX_POINTS = 60
const initHistory = (): HistoryPoint[] =>
  new Array(MAX_POINTS).fill(0).map((_, i) => ({ name: i, value: 0 }))

export const uploadSpeedHistory = ref<HistoryPoint[]>(initHistory())
export const downloadSpeedHistory = ref<HistoryPoint[]>(initHistory())
export const memoryHistory = ref<HistoryPoint[]>(initHistory())
export const connectionsHistory = ref<HistoryPoint[]>(initHistory())

const currentTraffic = ref<TrafficData>({ up: 0, down: 0 })
const memory = ref<MemoryData>({ inuse: 0, oslimit: 0 })

let trafficWs: ReconnectingWebSocket | null = null
let memoryWs: ReconnectingWebSocket | null = null
let refCount = 0

export function pushConnectionCount(count: number) {
  const ts = Date.now()
  connectionsHistory.value = [
    ...connectionsHistory.value,
    { name: ts, value: count },
  ].slice(-MAX_POINTS)
}

// 模块级：监听核心状态，停止时立即重置所有数据
// 放在模块顶层确保无论当前在哪个界面都生效，不会因组件卸载而失效
const { serviceStatus } = useServiceStore()
watch(
  () => serviceStatus.value.state,
  (state, prevState) => {
    // unknown 是软件刚启动时的初始占位状态，尚未完成首次轮询
    // 从 unknown 变为非 running 不应触发重置，否则首次启动核心时会多清一次数据
    if (state !== 'running' && prevState !== 'unknown') {
      currentTraffic.value = { up: 0, down: 0 }
      memory.value = { inuse: 0, oslimit: 0 }
      uploadSpeedHistory.value = initHistory()
      // NetworkInfo 组件在其他页面时已被卸载，其内部 watch 不会执行
      // 在此统一清除缓存，确保切回 OverviewPage 时不会读到旧数据
      sessionStorage.removeItem('singboard-network')
      downloadSpeedHistory.value = initHistory()
      memoryHistory.value = initHistory()
      connectionsHistory.value = initHistory()
    }
  },
)

export function useOverviewStore() {
  const { activeClashApiId } = useConfigStore()

  function start() {
    if (!trafficWs) {
      trafficWs = createClashWS('/traffic', (data: TrafficData) => {
        currentTraffic.value = data
        const ts = Date.now()

        uploadSpeedHistory.value = [
          ...uploadSpeedHistory.value,
          { name: ts, value: data.up },
        ].slice(-MAX_POINTS)

        downloadSpeedHistory.value = [
          ...downloadSpeedHistory.value,
          { name: ts, value: data.down },
        ].slice(-MAX_POINTS)
      })
    }
    if (!memoryWs) {
      memoryWs = createClashWS('/memory', (data: MemoryData) => {
        if (data.inuse === 0) return
        memory.value = data
        const ts = Date.now()

        memoryHistory.value = [
          ...memoryHistory.value,
          { name: ts, value: data.inuse },
        ].slice(-MAX_POINTS)
      })
    }
  }

  function stop() {
    trafficWs?.close()
    trafficWs = null
    memoryWs?.close()
    memoryWs = null
  }

  refCount++
  const unwatchApi = watch(
    () => activeClashApiId.value,
    () => {
      if (trafficWs || memoryWs) {
        stop()
        uploadSpeedHistory.value = initHistory()
        downloadSpeedHistory.value = initHistory()
        memoryHistory.value = initHistory()
        connectionsHistory.value = initHistory()
        start()
      }
    },
  )
  const unwatchVisible = watch(appVisible, (visible) => {
    if (visible) {
      if (!trafficWs && !memoryWs && refCount > 0) start()
    } else {
      if (trafficWs || memoryWs) stop()
    }
  })
  onUnmounted(() => {
    unwatchApi()
    unwatchVisible()
    refCount--
    if (refCount === 0) stop()
  })

  return {
    uploadSpeedHistory,
    downloadSpeedHistory,
    memoryHistory,
    connectionsHistory,
    currentTraffic,
    memory,
    start,
    stop,
  }
}