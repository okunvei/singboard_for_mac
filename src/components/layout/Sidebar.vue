<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useServiceStore } from '@/stores/service'
import { useConfigStore } from '@/stores/config'
import { normalizeVersionText } from '@/utils/format'
import { useToastStore } from '@/stores/toast'
import { getSingboxVersion, validateSingboxConfig, getRunningConfigPath, getRemoteConfigPath, copyToRunningConfig } from '@/bridge/config'
import { startService, stopService } from '@/bridge/service'

const route = useRoute()
const router = useRouter()
const { serviceStatus, statusText, refresh } = useServiceStore()
const { config, configProfiles } = useConfigStore()
const { pushToast } = useToastStore()
const singboxVersion = ref('')
const actionLoading = ref('') 
const versionWrapEl = ref<HTMLElement | null>(null)
const versionTrackEl = ref<HTMLElement | null>(null)
const shouldScrollVersion = ref(false)
const versionOverflowPx = ref(0)
let resizeOb: ResizeObserver | null = null

const navItems = [
  { path: '/overview', label: '概览', icon: 'chart' },
  { path: '/proxies', label: '代理', icon: 'proxy' },
  { path: '/connections', label: '连接', icon: 'connection' },
  { path: '/logs', label: '日志', icon: 'log' },
  { path: '/rules', label: '规则', icon: 'rule' },
  { path: '/config', label: '配置', icon: 'config' },
  { path: '/settings', label: '设置', icon: 'settings' },
]

const currentPath = computed(() => route.path)

function navigate(path: string) { router.push(path) }

async function refreshVersion() {
  // 注意：删除了之前的 "if (state !== 'running') return" 判断
  const singboxPath = config.value.singboxPath?.trim()
  if (!singboxPath) {
    singboxVersion.value = '未配置内核路径'
    return
  }
  try {
    const raw = await getSingboxVersion(singboxPath)
    singboxVersion.value = normalizeVersionText(raw)
  } catch (e: any) {
    const errMsg = String(e?.message || e)
    // 捕获“找不到文件”的错误
    if (errMsg.includes('系统找不到指定的文件') || errMsg.includes('not found') || errMsg.includes('2')) {
      singboxVersion.value = '❌ 未安装 exe 核心'
    } else {
      singboxVersion.value = '版本获取失败'
    }
  }
}

// 按钮控制逻辑
async function validateBeforeStart(): Promise<boolean> {
  const { singboxPath, workingDir } = config.value
  if (!singboxPath) {
    pushToast({ message: '请先在设置中配置内核路径', type: 'error' })
    return false
  }
  const activeId = config.value.activeConfigProfileId
  const profile = configProfiles.value.find((p) => p.id === activeId)

  try {
    let configPath = ''
    if (profile) {
      configPath = profile.type === 'local' ? profile.source : await getRemoteConfigPath(profile.id)
      await validateSingboxConfig(singboxPath, configPath, workingDir)
      await copyToRunningConfig(configPath)
    } else {
      configPath = await getRunningConfigPath()
      await validateSingboxConfig(singboxPath, configPath, workingDir)
    }
    return true
  } catch (e: any) {
    pushToast({ message: '校验失败: ' + (e?.message || e), type: 'error' })
    return false
  }
}

async function handleServiceAction(action: 'start' | 'stop' | 'restart') {
  actionLoading.value = action
  try {
    const name = config.value.serviceName
    if (action === 'start' || action === 'restart') {
      if (!(await validateBeforeStart())) return
      if (action === 'restart') await stopService(name)
      await startService(name)
    } else {
      await stopService(name)
    }
    setTimeout(refresh, 1000)
  } catch (e: any) {
    pushToast({ message: '操作失败: ' + e, type: 'error' })
  } finally {
    actionLoading.value = ''
  }
}

// 状态颜色
const statusColor = computed(() => {
  switch (serviceStatus.value.state) {
    case 'running': return 'bg-success shadow-[0_0_5px_rgba(34,197,94,0.4)]'
    case 'stopped': return 'bg-error shadow-[0_0_5px_rgba(239,68,68,0.4)]'
    case 'starting':
    case 'stopping': return 'bg-warning animate-pulse' // 停止时黄色警告颜色
    case 'not_installed': return 'bg-error opacity-50' // 未安装时红颜色突出表示重要提示
    default: return 'bg-base-content/30'
  }
})

// 监听与尺寸测量

function measureOverflow() {
  const wrap = versionWrapEl.value
  const track = versionTrackEl.value
  if (!wrap || !track || !singboxVersion.value) {
    shouldScrollVersion.value = false; versionOverflowPx.value = 0; return
  }
  const overflow = track.offsetWidth - wrap.clientWidth
  shouldScrollVersion.value = overflow > 2
  versionOverflowPx.value = Math.max(0, Math.ceil(overflow))
}

watch(() => [serviceStatus.value.state, config.value.singboxPath], () => { void refreshVersion() }, { immediate: true })
watch(versionWrapEl, (el) => {
  resizeOb?.disconnect()
  if (el) {
    resizeOb = new ResizeObserver(() => measureOverflow())
    resizeOb.observe(el)
  }
})

watch(singboxVersion, () => { requestAnimationFrame(() => measureOverflow()) })
onBeforeUnmount(() => { resizeOb?.disconnect(); resizeOb = null })
</script>

<template>
  <div class="flex flex-col w-48 bg-base-200 border-r border-base-300 h-full">
    <nav class="flex-1 py-2 overflow-y-auto">
      <button
        v-for="item in navItems"
        :key="item.path"
        class="w-full flex items-center gap-3 px-4 py-2.5 text-sm transition-colors"
        :class="currentPath === item.path ? 'bg-primary/10 text-primary font-medium border-r-2 border-primary' : 'hover:bg-base-300 text-base-content/70'"
        @click="navigate(item.path)"
      >
        <span class="w-5 text-center emoji-font">
          <template v-if="item.icon === 'chart'">📊</template>
          <template v-else-if="item.icon === 'proxy'">🔀</template>
          <template v-else-if="item.icon === 'connection'">🔗</template>
          <template v-else-if="item.icon === 'log'">📝</template>
          <template v-else-if="item.icon === 'rule'">📋</template>
          <template v-else-if="item.icon === 'config'">📄</template>
          <template v-else-if="item.icon === 'settings'">⚙️</template>
        </span>
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <div class="px-4 py-6 border-t border-base-300 space-y-4">
      <div class="flex flex-col gap-1.5 px-2">
        <span class="text-sm font-bold opacity-70">核心控制</span>
        <div class="flex items-center gap-2 text-xs text-base-content/80">
          <span class="w-2 h-2 rounded-full shrink-0" :class="statusColor"></span>
          <span>{{ statusText }}</span>
        </div>
      </div>
      
      <div class="flex flex-col gap-1">
        <button 
          class="btn btn-sm btn-ghost w-full justify-start font-normal hover:bg-success/10 hover:text-success text-base-content/80" 
          :disabled="serviceStatus.state === 'running' || serviceStatus.state === 'not_installed' || !!actionLoading"
          @click="handleServiceAction('start')"
        >
          <span v-if="actionLoading === 'start'" class="loading loading-spinner loading-xs"></span>
          🚀 启动核心
        </button>

        <button 
          class="btn btn-sm btn-ghost w-full justify-start font-normal hover:bg-warning/10 hover:text-warning text-base-content/80"
          :disabled="serviceStatus.state === 'not_installed' || !!actionLoading"
          @click="handleServiceAction('restart')"
        >
          <span v-if="actionLoading === 'restart'" class="loading loading-spinner loading-xs"></span>
          🔄 重启核心
        </button>

        <button 
          class="btn btn-sm btn-ghost w-full justify-start font-normal hover:bg-error/10 hover:text-error text-base-content/80"
          :disabled="serviceStatus.state === 'stopped' || serviceStatus.state === 'not_installed' || !!actionLoading"
          @click="handleServiceAction('stop')"
        >
          <span v-if="actionLoading === 'stop'" class="loading loading-spinner loading-xs"></span>
          🛑 停止核心
        </button>
      </div>
    </div>

    <div class="px-4 py-3 border-t border-base-300 bg-base-300/30">
      <div class="flex items-center gap-2 text-xs text-base-content/60 whitespace-nowrap overflow-hidden">
        <span
          ref="versionWrapEl"
          class="version-wrap text-base-content/60"
          :class="{ scrolling: shouldScrollVersion }"
          :style="{ '--overflow-distance': versionOverflowPx }"
          :title="singboxVersion"
        >
          <span ref="versionTrackEl" class="version-track">
            <span class="version-item">{{ singboxVersion || '正在检测核心...' }}</span>
          </span>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.version-wrap { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; }
.version-track { display: inline-flex; align-items: center; }
.version-wrap.scrolling .version-track { animation: version-pingpong 4.5s ease-in-out infinite alternate; will-change: transform; }
.version-item { flex: 0 0 auto; }
@keyframes version-pingpong { 0% { transform: translateX(0); } 100% { transform: translateX(calc(-1px * var(--overflow-distance, 0))); } }
</style>
