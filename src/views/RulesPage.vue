<script setup lang="ts">
import { computed, onMounted, ref, watch, nextTick } from 'vue'

import { useRulesStore } from '@/stores/rules'
import { fetchRuleProviders, updateRuleProvider } from '@/api'
import type { RuleProvider } from '@/types'
import { getRequestErrorReason } from '@/utils/requestError'
import { useToastStore } from '@/stores/toast'
import { useConfigStore } from '@/stores/config'
import { useServiceStore } from '@/stores/service'
import { srsMatchProvider, srsListProvider, getRunningConfigPath } from '@/bridge/config'

const { filteredRules, loading, filterText, loadRules } = useRulesStore()
const { serviceStatus } = useServiceStore()
const isRunning = computed(() => serviceStatus.value.state === 'running')

const activeTab = ref<'rules' | 'providers'>('rules')

const ruleProviders = ref<RuleProvider[]>([])
const providersAvailable = ref(false)
const updatingProvider = ref<string | null>(null)
const updatingAll = ref(false)
const { pushToast } = useToastStore()
const { config } = useConfigStore()

const providerSearchText = ref('')
const providerSearching = ref(false)
// undefined = not searched, -1 = error/not found, false = no match, true = matched
const providerMatchCounts = ref<Record<string, boolean | -1>>({})
const providerSearchDone = ref(false)
let searchTimer: ReturnType<typeof setTimeout> | null = null

const displayedProviders = computed(() => {
  const q = providerSearchText.value.trim()
  if (!q) return ruleProviders.value
  if (!providerSearchDone.value) return ruleProviders.value
  return ruleProviders.value.filter((p) => providerMatchCounts.value[p.name] === true)
})

async function searchInProviders() {
  const q = providerSearchText.value.trim()
  if (!q) {
    providerMatchCounts.value = {}
    providerSearchDone.value = false
    return
  }
  providerSearching.value = true
  providerSearchDone.value = false

  let configPath = ''
  try {
    configPath = await getRunningConfigPath()
  } catch { }

  const results: Record<string, boolean | -1> = {}
  await Promise.allSettled(
    ruleProviders.value.map(async (p) => {
      try {
        results[p.name] = await srsMatchProvider(
          config.value.workingDir ?? '',
          configPath,
          config.value.singboxPath ?? '',
          p.name,
          q,
        )
      } catch (e) {
        console.error(`[srs-search] ${p.name}:`, e)
        results[p.name] = -1
      }
    }),
  )
  providerMatchCounts.value = results
  providerSearchDone.value = true
  providerSearching.value = false
}

watch(providerSearchText, (val) => {
  if (searchTimer) clearTimeout(searchTimer)
  if (!val.trim()) {
    providerMatchCounts.value = {}
    providerSearchDone.value = false
    return
  }
  searchTimer = setTimeout(searchInProviders, 500)
})

async function loadProviders() {
  try {
    const { data } = await fetchRuleProviders()
    ruleProviders.value = Object.values(data.providers)
    providersAvailable.value = true
  } catch {
    providersAvailable.value = false
  }
}

async function handleUpdateProvider(name: string) {
  updatingProvider.value = name
  try {
    await updateRuleProvider(name)
    await loadProviders()
    await loadRules()
  } catch (error) {
    pushToast({
      type: 'error',
      message: `更新规则提供商失败\n${name}\n原因: ${getRequestErrorReason(error)}`,
    })
  }
  updatingProvider.value = null
}

async function handleUpdateAll() {
  updatingAll.value = true
  const updatable = ruleProviders.value.filter((p) => p.vehicleType !== 'Inline')
  const results = await Promise.allSettled(updatable.map((p) => updateRuleProvider(p.name)))
  const failed = results.filter((result): result is PromiseRejectedResult => result.status === 'rejected')
  if (failed.length > 0) {
    const reasons = Array.from(new Set(failed.map((result) => getRequestErrorReason(result.reason))))
    pushToast({
      type: 'error',
      message: `部分规则提供商更新失败 (${failed.length}/${updatable.length})\n原因: ${reasons.join('；')}`,
    })
  }
  await loadProviders()
  await loadRules()
  updatingAll.value = false
}

// ---- 规则详情弹窗 ----
const detailProvider = ref<RuleProvider | null>(null)
const detailLoading = ref(false)
const detailError = ref('')
const detailRules = ref<Array<{ type: string; value: string }>>([])
const detailFilterText = ref('')

// 弹窗搜索：调用 Rust 端 srsMatchProvider 做精确匹配
const detailMatchResult = ref<boolean | null>(null) // null=未搜索, true=匹配, false=未匹配
const detailMatchSearching = ref(false)
let detailSearchTimer: ReturnType<typeof setTimeout> | null = null

const filteredDetailRules = ref<Array<{ type: string; value: string }>>([])
let filterTimer: ReturnType<typeof setTimeout> | null = null

function parseIPv4(ip: string): number | null {
  const parts = ip.split('.')
  if (parts.length !== 4) return null
  let n = 0
  for (const p of parts) {
    const v = Number(p)
    if (!Number.isInteger(v) || v < 0 || v > 255) return null
    n = (n << 8) | v
  }
  return n >>> 0
}

function runDetailFilter() {
  const q = detailFilterText.value.trim().toLowerCase()
  if (!q) {
    filteredDetailRules.value = detailRules.value
    return
  }
  const qIPv4 = parseIPv4(q)
  filteredDetailRules.value = detailRules.value.filter((r) => {
    // 文本包含
    if (r.value.toLowerCase().includes(q) || r.type.toLowerCase().includes(q)) return true
    // IP CIDR 语义匹配
    if (qIPv4 !== null && (r.type === 'ip_cidr' || r.type === 'source_ip_cidr')) {
      const [network, prefixStr] = r.value.split('/')
      if (!prefixStr) return false
      const prefix = Number(prefixStr)
      const netNum = parseIPv4(network)
      if (netNum === null || prefix < 0 || prefix > 32) return false
      const mask = prefix === 0 ? 0 : (~0 << (32 - prefix)) >>> 0
      return (qIPv4 & mask) === (netNum & mask)
    }
    // 域名语义匹配
    if (qIPv4 === null) {
      const val = r.value.toLowerCase()
      if (r.type === 'domain') return q === val
      if (r.type === 'domain_suffix') return q.endsWith(val) || q.endsWith('.' + val.replace(/^\./, ''))
      if (r.type === 'domain_keyword') return q.includes(val)
    }
    return false
  })
}

watch(detailFilterText, () => {
  if (filterTimer) clearTimeout(filterTimer)
  filterTimer = setTimeout(runDetailFilter, 300)
})

watch(detailRules, () => {
  filteredDetailRules.value = detailRules.value
})

async function searchInDetail() {
  const q = detailFilterText.value.trim()
  const provider = detailProvider.value
  if (!q || !provider) {
    detailMatchResult.value = null
    return
  }
  detailMatchSearching.value = true
  detailMatchResult.value = null

  let configPath = ''
  try { configPath = await getRunningConfigPath() } catch {}

  try {
    detailMatchResult.value = await srsMatchProvider(
      config.value.workingDir ?? '',
      configPath,
      config.value.singboxPath ?? '',
      provider.name,
      q,
    )
  } catch {
    detailMatchResult.value = null
  } finally {
    detailMatchSearching.value = false
  }
}

watch(detailFilterText, (val) => {
  if (detailSearchTimer) clearTimeout(detailSearchTimer)
  if (!val.trim()) {
    detailMatchResult.value = null
    detailMatchSearching.value = false
    return
  }
  detailSearchTimer = setTimeout(searchInDetail, 500)
})

// 虚拟滚动
const ROW_HEIGHT = 28
const OVERSCAN = 10
const detailScrollTop = ref(0)
const detailContainerHeight = ref(400)
const detailScrollRef = ref<HTMLElement | null>(null)

const virtualSlice = computed(() => {
  const items = filteredDetailRules.value
  const total = items.length
  const startIdx = Math.max(0, Math.floor(detailScrollTop.value / ROW_HEIGHT) - OVERSCAN)
  const visibleCount = Math.ceil(detailContainerHeight.value / ROW_HEIGHT) + OVERSCAN * 2
  const endIdx = Math.min(total, startIdx + visibleCount)
  return {
    items: items.slice(startIdx, endIdx),
    startIdx,
    totalHeight: total * ROW_HEIGHT,
    offsetY: startIdx * ROW_HEIGHT,
  }
})

function onDetailScroll(e: Event) {
  const el = e.target as HTMLElement
  detailScrollTop.value = el.scrollTop
}

async function openProviderDetail(provider: RuleProvider) {
  detailProvider.value = provider
  detailLoading.value = true
  detailError.value = ''
  detailRules.value = []
  detailFilterText.value = ''

  let configPath = ''
  try {
    configPath = await getRunningConfigPath()
  } catch {}

  try {
    const rules = await srsListProvider(
      config.value.workingDir ?? '',
      configPath,
      config.value.singboxPath ?? '',
      provider.name,
    )
    detailRules.value = rules
  } catch (e: any) {
    detailError.value = e?.message || String(e)
  } finally {
    detailLoading.value = false
  }
}

function closeProviderDetail() {
  detailProvider.value = null
  detailRules.value = []
  detailFilterText.value = ''
  detailError.value = ''
  detailScrollTop.value = 0
  detailMatchResult.value = null
  detailMatchSearching.value = false
  if (detailSearchTimer) { clearTimeout(detailSearchTimer); detailSearchTimer = null }
  if (filterTimer) { clearTimeout(filterTimer); filterTimer = null }
}

function formatDate(dateStr: string): string {
  if (!dateStr) return ''
  const diff = Date.now() - new Date(dateStr).getTime()
  if (diff < 60000) return '刚刚'
  if (diff < 3600000) return Math.floor(diff / 60000) + ' 分钟前'
  if (diff < 86400000) return Math.floor(diff / 3600000) + ' 小时前'
  return Math.floor(diff / 86400000) + ' 天前'
}

onMounted(() => {
  if (isRunning.value) {
    loadRules()
    loadProviders()
  }
})

watch(isRunning, (running) => {
  if (running) {
    loadRules()
    loadProviders()
  }
})
</script>

<template>
  <div class="flex flex-col h-full gap-3">
    <template v-if="!isRunning">
      <div class="flex flex-col items-center justify-center flex-1 gap-4 text-base-content/40">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1" stroke="currentColor" class="w-16 h-16">
          <path stroke-linecap="round" stroke-linejoin="round" d="M5.636 5.636a9 9 0 1012.728 0M12 3v9" />
        </svg>
        <div class="text-center space-y-1">
          <p class="text-lg font-medium">服务未启动</p>
          <p class="text-sm">请先启动 sing-box 服务以查看规则信息</p>
        </div>
      </div>
    </template>

    <template v-else>
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <h1 class="text-xl font-bold shrink-0">规则</h1>
        <div class="tabs tabs-boxed tabs-sm">
          <a class="tab" :class="{ 'tab-active': activeTab === 'rules' }" @click="activeTab = 'rules'">
            规则 ({{ filteredRules.length }})
          </a>
          <a
            v-if="providersAvailable"
            class="tab"
            :class="{ 'tab-active': activeTab === 'providers' }"
            @click="activeTab = 'providers'"
          >
            规则提供商 ({{ ruleProviders.length }})
          </a>
        </div>
      </div>
      <button class="btn btn-sm btn-ghost" :class="{ 'loading': loading }" @click="loadRules">
        刷新
      </button>
    </div>

    <template v-if="activeTab === 'rules'">
      <input
        v-model="filterText"
        type="text"
        placeholder="搜索规则..."
        class="input input-sm input-bordered w-full"
      />

      <div class="flex-1 overflow-auto">
        <table class="table table-xs table-pin-rows">
          <thead>
            <tr class="bg-base-200">
              <th class="w-8">#</th>
              <th>类型</th>
              <th>匹配内容</th>
              <th>代理</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(rule, i) in filteredRules" :key="i" class="hover:bg-base-200/50">
              <td class="text-base-content/30">{{ i + 1 }}</td>
              <td>
                <span class="text-xs leading-none px-1.5 py-0.5 rounded bg-base-content/10 text-base-content/60">{{ rule.type }}</span>
              </td>
              <td class="max-w-[300px] truncate text-xs" :title="rule.payload">
                {{ rule.payload }}
              </td>
              <td class="text-xs text-primary">{{ rule.proxy }}</td>
            </tr>
          </tbody>
        </table>

        <div v-if="loading" class="flex justify-center py-10">
          <span class="loading loading-spinner loading-md"></span>
        </div>
      </div>
    </template>

    <div v-if="activeTab === 'providers'" class="flex-1 overflow-auto space-y-1.5">
      <div
        v-if="ruleProviders.length === 0"
        class="flex items-center justify-center py-10 text-base-content/40"
      >
        暂无规则提供商
      </div>

      <div v-if="ruleProviders.length > 0" class="flex items-center gap-2">
        <div class="relative flex-1">
          <input
            v-model="providerSearchText"
            type="text"
            placeholder="搜索规则内容..."
            class="input input-sm input-bordered w-full"
          />
          <span
            v-if="providerSearching"
            class="loading loading-spinner loading-xs absolute right-2 top-1/2 -translate-y-1/2 text-base-content/40"
          ></span>
        </div>
        <button
          class="btn btn-sm btn-ghost shrink-0"
          :class="{ 'loading': updatingAll }"
          @click="handleUpdateAll"
          :disabled="updatingAll"
        >
          <template v-if="!updatingAll">全部更新</template>
        </button>
      </div>

      <div
        v-if="ruleProviders.length > 0 && providerSearchText.trim() && providerSearchDone && displayedProviders.length === 0"
        class="flex items-center justify-center py-10 text-base-content/40"
      >
        未找到匹配规则
      </div>

      <div
        v-for="(provider, i) in displayedProviders"
        :key="provider.name"
        class="bg-base-200 rounded-lg px-3 py-2 flex items-center justify-between transition-colors"
        :class="provider.vehicleType !== 'Inline' && provider.behavior !== 'SOURCE' ? 'cursor-pointer hover:bg-base-300/30' : ''"
        @click="provider.vehicleType !== 'Inline' && provider.behavior !== 'SOURCE' && openProviderDetail(provider)"
      >
        <div class="flex items-center gap-2 min-w-0">
          <span class="text-xs text-base-content/30 w-5 shrink-0">{{ i + 1 }}</span>
          <span class="text-sm font-medium truncate">{{ provider.name }}</span>
          <span class="text-xs text-base-content/50">({{ provider.ruleCount }})</span>
          <template v-if="providerSearchDone">
            <span
              v-if="providerMatchCounts[provider.name] === -1"
              class="text-xs leading-none px-1.5 py-0.5 rounded shrink-0 bg-base-content/10 text-base-content/30"
              title="未找到规则文件路径"
            >—</span>
            <span
              v-else
              class="text-xs leading-none px-1.5 py-0.5 rounded shrink-0"
              :class="providerMatchCounts[provider.name] === true ? 'bg-primary/20 text-primary' : 'bg-base-content/10 text-base-content/30'"
            >{{ providerMatchCounts[provider.name] === true ? '匹配' : '未匹配' }}</span>
          </template>
          <span v-if="provider.behavior" class="text-xs leading-none px-1.5 py-0.5 rounded bg-base-content/10 text-base-content/60 shrink-0">{{ provider.behavior }}</span>
          <span v-if="provider.vehicleType" class="text-xs leading-none px-1.5 py-0.5 rounded border border-base-content/20 text-base-content/60 shrink-0">{{ provider.vehicleType }}</span>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <span class="text-xs text-base-content/40">{{ formatDate(provider.updatedAt) }}</span>
          <button
            v-if="provider.vehicleType !== 'Inline'"
            class="btn btn-ghost btn-xs btn-circle"
            :class="{ 'loading': updatingProvider === provider.name }"
            @click.stop="handleUpdateProvider(provider.name)"
            title="更新"
          >
            <svg v-if="updatingProvider !== provider.name" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-3.5 h-3.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.992 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182" />
            </svg>
          </button>
        </div>
      </div>
    </div>
    </template>
  </div>

  <!-- 规则详情弹窗 -->
  <div
    v-if="detailProvider"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
    @click.self="closeProviderDetail"
  >
    <div class="w-full max-w-2xl max-h-[80vh] flex flex-col rounded-lg bg-base-100 shadow-xl">
      <div class="flex items-start justify-between px-5 pt-4 pb-3 shrink-0">
        <div class="flex flex-col gap-1.5">
          <div class="flex items-baseline gap-2">
            <span class="font-semibold text-base">{{ detailProvider.name }}</span>
            <span class="text-xs text-base-content/50">{{ detailProvider.ruleCount }} 条规则</span>
          </div>
          <div class="flex items-center gap-1.5">
            <span v-if="detailProvider.behavior" class="text-xs leading-none px-1.5 py-0.5 rounded bg-base-content/10 text-base-content/60">{{ detailProvider.behavior }}</span>
            <span v-if="detailProvider.vehicleType" class="text-xs leading-none px-1.5 py-0.5 rounded border border-base-content/20 text-base-content/60">{{ detailProvider.vehicleType }}</span>
            <span class="text-xs text-base-content/40">{{ formatDate(detailProvider.updatedAt) }}</span>
          </div>
        </div>
        <button class="btn btn-sm btn-circle btn-ghost" @click="closeProviderDetail">✕</button>
      </div>

      <div class="px-5 pb-2 shrink-0 flex items-center gap-2">
        <div class="relative flex-1">
          <input
            v-model="detailFilterText"
            type="text"
            placeholder="搜索规则内容..."
            class="input input-sm input-bordered w-full"
          />
          <span
            v-if="detailMatchSearching"
            class="loading loading-spinner loading-xs absolute right-2 top-1/2 -translate-y-1/2 text-base-content/40"
          ></span>
        </div>
        <span
          v-if="detailFilterText.trim() && !detailMatchSearching && detailMatchResult !== null"
          class="text-xs leading-none px-2 py-1 rounded shrink-0"
          :class="detailMatchResult ? 'bg-success/15 text-success' : 'bg-base-content/10 text-base-content/40'"
        >{{ detailMatchResult ? '匹配' : '未匹配' }}</span>
      </div>

      <div class="flex-1 flex flex-col px-5 pb-4 min-h-0">
        <div v-if="detailLoading" class="flex justify-center py-10">
          <span class="loading loading-spinner loading-md"></span>
        </div>
        <div v-else-if="detailError" class="text-sm text-error py-4">{{ detailError }}</div>
        <template v-else>
          <div class="flex text-xs font-semibold text-base-content/60 bg-base-200 rounded-t px-2 shrink-0" :style="{ height: ROW_HEIGHT + 'px', lineHeight: ROW_HEIGHT + 'px' }">
            <span class="w-12 shrink-0">#</span>
            <span class="w-28 shrink-0">类型</span>
            <span class="flex-1">内容</span>
          </div>
          <div
            ref="detailScrollRef"
            class="flex-1 overflow-auto min-h-0"
            @scroll="onDetailScroll"
          >
            <div :style="{ height: virtualSlice.totalHeight + 'px', position: 'relative' }">
              <div :style="{ transform: `translateY(${virtualSlice.offsetY}px)` }">
                <div
                  v-for="(rule, j) in virtualSlice.items"
                  :key="virtualSlice.startIdx + j"
                  class="flex items-center px-2 text-xs hover:bg-base-200/50"
                  :style="{ height: ROW_HEIGHT + 'px' }"
                >
                  <span class="w-12 shrink-0 text-base-content/30">{{ virtualSlice.startIdx + j + 1 }}</span>
                  <span class="w-28 shrink-0">
                    <span class="leading-none px-1.5 py-0.5 rounded bg-base-content/10 text-base-content/60 whitespace-nowrap">{{ rule.type }}</span>
                  </span>
                  <span class="flex-1 truncate" :title="rule.value">{{ rule.value }}</span>
                </div>
              </div>
            </div>
          </div>
          <div v-if="detailRules.length > 0" class="text-xs text-base-content/40 pt-2 shrink-0">
            <template v-if="detailFilterText.trim()">
              显示 {{ filteredDetailRules.length }} / 共 {{ detailRules.length }} 条
            </template>
            <template v-else>
              共 {{ detailRules.length }} 条
            </template>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>
