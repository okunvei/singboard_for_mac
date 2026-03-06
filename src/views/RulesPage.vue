<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRulesStore } from '@/stores/rules'
import { fetchRuleProviders, updateRuleProvider } from '@/api'
import type { RuleProvider } from '@/types'
import { getRequestErrorReason } from '@/utils/requestError'
import { useToastStore } from '@/stores/toast'
import { useConfigStore } from '@/stores/config'
import { srsMatchProvider } from '@/bridge/config'

const { filteredRules, loading, filterText, loadRules } = useRulesStore()

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
  const results: Record<string, boolean | -1> = {}
  await Promise.allSettled(
    ruleProviders.value.map(async (p) => {
      try {
        results[p.name] = await srsMatchProvider(
          config.value.workingDir ?? '',
          config.value.configPath ?? '',
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

function formatDate(dateStr: string): string {
  if (!dateStr) return ''
  const diff = Date.now() - new Date(dateStr).getTime()
  if (diff < 60000) return '刚刚'
  if (diff < 3600000) return Math.floor(diff / 60000) + ' 分钟前'
  if (diff < 86400000) return Math.floor(diff / 3600000) + ' 小时前'
  return Math.floor(diff / 86400000) + ' 天前'
}

onMounted(() => {
  loadRules()
  loadProviders()
})
</script>

<template>
  <div class="flex flex-col h-full gap-3">
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
        class="bg-base-200 rounded-lg px-3 py-2 flex items-center justify-between"
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
            @click="handleUpdateProvider(provider.name)"
            title="更新"
          >
            <svg v-if="updatingProvider !== provider.name" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-3.5 h-3.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.992 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
