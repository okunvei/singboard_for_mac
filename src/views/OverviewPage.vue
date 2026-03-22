<script setup lang="ts">
import { onMounted, computed, ref } from 'vue'
import { useOverviewStore } from '@/stores/overview'
import { useConnectionsStore } from '@/stores/connections'
import NetworkInfo from '@/components/overview/NetworkInfo.vue'
import TopologyChart from '@/components/overview/TopologyChart.vue'
import { formatBytes, formatSpeed } from '@/utils/format'

const { currentTraffic, memory, start: startOverview } = useOverviewStore()
const { connections, start: startConnections } = useConnectionsStore()

const memoryUsage = computed(() => formatBytes(memory.value.inuse))

onMounted(async () => {
  startOverview()
  startConnections()
})
</script>

<template>
  <div class="space-y-4">
    <h1 class="text-xl font-bold">概览</h1>

    <div class="grid grid-cols-4 gap-3">
      <div class="stat bg-base-200 rounded-lg p-3">
        <div class="stat-title text-xs">上传速度</div>
        <div class="stat-value text-lg text-primary">{{ formatSpeed(currentTraffic.up) }}</div>
      </div>
      <div class="stat bg-base-200 rounded-lg p-3">
        <div class="stat-title text-xs">下载速度</div>
        <div class="stat-value text-lg text-secondary">{{ formatSpeed(currentTraffic.down) }}</div>
      </div>
      <div class="stat bg-base-200 rounded-lg p-3">
        <div class="stat-title text-xs">活跃连接</div>
        <div class="stat-value text-lg">{{ connections.length }}</div>
      </div>
      <div class="stat bg-base-200 rounded-lg p-3">
        <div class="stat-title text-xs">内存使用</div>
        <div class="stat-value text-lg">{{ memoryUsage }}</div>
      </div>
    </div>

    <NetworkInfo />

    <TopologyChart />
  </div>
</template>
