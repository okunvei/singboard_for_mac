<script setup lang="ts">
import { onMounted, computed } from 'vue'
import { useOverviewStore } from '@/stores/overview'
import { useConnectionsStore } from '@/stores/connections'
import { useConfigStore } from '@/stores/config' // ✨ 新加：用来读取设置里的 API 和 Secret
import { useServiceStore } from '@/stores/service' // ✨ 引入服务 Store
import { open } from '@tauri-apps/plugin-shell' // ✨ 新加：用来调用系统浏览器
import NetworkInfo from '@/components/overview/NetworkInfo.vue'
import TopologyChart from '@/components/overview/TopologyChart.vue'
import SparkLine from '@/components/overview/SparkLine.vue'
import { formatBytes, formatSpeed } from '@/utils/format'


const { clashApiUrl, clashApiSecret } = useConfigStore() // ✨ 新加：获取配置对象
const { serviceStatus } = useServiceStore() // ✨ 获取服务状态

// ✨ 创建一个计算属性，判断是否可以点击
const isRunning = computed(() => serviceStatus.value.state === 'running')

// ✨ 新加：这个函数负责拼接地址并打开浏览器
const openWebUI = async () => {
  // ✨ 逻辑拦截：如果没运行，直接返回
  if (!isRunning.value) return
  
  try {
    // 1. 直接使用 store 提供的当前激活地址和密钥
    const apiUrl = clashApiUrl.value || 'http://127.0.0.1:9090'
    const secret = clashApiSecret.value || ''
    
    // 2. 解析 URL（提取域名和端口）
    const url = new URL(apiUrl)
    const hostname = url.hostname
    const port = url.port || (url.protocol === 'https:' ? '443' : '80')

    // 3. 拼接地址：基础地址
    let targetUrl = `${apiUrl}/ui/?hostname=${hostname}&port=${port}`

    // 4. 加上 secret
    if (secret) {
      targetUrl += `&secret=${secret}`
    }

    // 5. 召唤系统默认浏览器
    await open(targetUrl)
  } catch (error) {
    console.error('无法解析地址，请检查配置是否正确:', error)
  }
}

const {
  currentTraffic, memory, uploadSpeedHistory, downloadSpeedHistory, connectionsHistory,
  start: startOverview,
} = useOverviewStore()
const { connections, downloadTotal, uploadTotal, start: startConnections } = useConnectionsStore()

const memoryUsage = computed(() => formatBytes(memory.value.inuse))

const speedLabelFormatter = (v: number) => (v === 0 ? '' : formatSpeed(v))
const connLabelFormatter = (v: number) => (v === 0 ? '' : String(Math.round(v)))

onMounted(async () => {
  startOverview()
  startConnections()
})
</script>

<template>
  <div class="flex flex-col h-full overflow-hidden">
    <div class="sticky top-0 z-10 bg-base-100 pb-4 flex items-center justify-between">
      <h1 class="text-xl font-bold">概览</h1>
      
      <button 
        @click="openWebUI" 
        :disabled="!isRunning"
        class="btn btn-sm transition-all"
        :class="isRunning ? 'btn-primary' : 'btn-ghost bg-base-300 text-base-content/30 cursor-not-allowed'"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="mr-1">
          <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
          <polyline points="15 3 21 3 21 9"></polyline>
          <line x1="10" y1="14" x2="21" y2="3"></line>
        </svg>
        访问 WebUI
      </button>
    </div>

    <div class="flex-1 overflow-auto space-y-4 pr-1">
      <div class="grid grid-cols-3 gap-3">
        <div class="bg-base-200 rounded-xl p-4 flex flex-col gap-1.5">
          <div class="text-xs text-base-content/60 font-semibold tracking-wider">上传</div>
          <div class="text-3xl font-extralight tabular-nums">
            {{ formatSpeed(currentTraffic.up) }}
          </div>
          <div class="h-14 mt-1">
            <SparkLine
              :data="uploadSpeedHistory"
              color="#67d4e2"
              :min="60000"
              :label-formatter="speedLabelFormatter"
            />
          </div>
          <div class="text-xs text-base-content/50">总计 {{ formatBytes(uploadTotal) }}</div>
        </div>
        <div class="bg-base-200 rounded-xl p-4 flex flex-col gap-1.5">
          <div class="text-xs text-base-content/60 font-semibold tracking-wider">下载</div>
          <div class="text-3xl font-extralight tabular-nums">
            {{ formatSpeed(currentTraffic.down) }}
          </div>
          <div class="h-14 mt-1">
            <SparkLine
              :data="downloadSpeedHistory"
              color="#8b7bf6"
              :min="60000"
              :label-formatter="speedLabelFormatter"
            />
          </div>
          <div class="text-xs text-base-content/50">总计 {{ formatBytes(downloadTotal) }}</div>
        </div>
        <div class="bg-base-200 rounded-xl p-4 flex flex-col gap-1.5">
          <div class="text-xs text-base-content/60 font-semibold tracking-wider">
            连接
          </div>
          <div class="text-3xl font-extralight tabular-nums">{{ connections.length }}</div>
          <div class="h-14 mt-1">
            <SparkLine
              :data="connectionsHistory"
              color="#10b981"
              :min="10"
              :label-formatter="connLabelFormatter"
              :right-margin="24"
            />
          </div>
          <div class="text-xs text-base-content/50">内存使用 {{ memoryUsage }}</div>
        </div>
      </div>
  
      <NetworkInfo />
  
      <TopologyChart />
    </div>
  </div>
</template>
