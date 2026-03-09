<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useConfigStore } from '@/stores/config'
import { useServiceStore } from '@/stores/service'
import { useToastStore } from '@/stores/toast'
import { useProxiesStore } from '@/stores/proxies'
import {
  startService,
  stopService,
  restartService,
  installService,
  uninstallService,
  readServiceErrorLog,
} from '@/bridge/service'
import { getSingboxVersion, validateSingboxConfig } from '@/bridge/config'
import { patchConfig, fetchConfig } from '@/api'

const {
  config,
  updateConfig,
  clashApis,
  activeClashApi,
  activeClashApiId,
  setActiveClashApi,
  addClashApi,
  updateActiveClashApi,
  removeClashApi,
} = useConfigStore()
const { serviceStatus, refresh } = useServiceStore()
const { pushToast } = useToastStore()

const { proxyGroups, loadProxies } = useProxiesStore()

const groupTestUrlsExpanded = ref(false)
const newGroupTestUrl = ref({ group: '', url: '' })

const groupTestUrlEntries = computed(() =>
  Object.entries(config.value.groupTestUrls)
)

const availableGroups = computed(() =>
  proxyGroups.value
    .filter((g) => g.name !== 'GLOBAL' && !config.value.groupTestUrls[g.name])
    .map((g) => g.name)
)

function addGroupTestUrl() {
  const group = newGroupTestUrl.value.group.trim()
  const url = newGroupTestUrl.value.url.trim()
  if (!group || !url) return
  config.value.groupTestUrls = { ...config.value.groupTestUrls, [group]: url }
  newGroupTestUrl.value = { group: '', url: '' }
}

function removeGroupTestUrl(group: string) {
  const { [group]: _, ...rest } = config.value.groupTestUrls
  config.value.groupTestUrls = rest
}

const clashMode = ref('Rule')
const clashModeOptions = ref<string[]>(['Rule'])
const singboxVersion = ref('')
const actionLoading = ref('')
const activeApiForm = ref({
  name: '',
  url: '',
  secret: '',
})
const newApiForm = ref({
  name: '',
  url: '',
  secret: '',
})
const showEditApiForm = ref(false)
const showAddApiForm = ref(false)

function syncActiveApiForm() {
  const current = activeClashApi.value
  activeApiForm.value = {
    name: current?.name ?? '',
    url: current?.url ?? '',
    secret: current?.secret ?? '',
  }
}

function toggleEditApiForm() {
  showEditApiForm.value = !showEditApiForm.value
  if (showEditApiForm.value) {
    syncActiveApiForm()
    showAddApiForm.value = false
  }
}

function toggleAddApiForm() {
  showAddApiForm.value = !showAddApiForm.value
  if (showAddApiForm.value) {
    newApiForm.value = { name: '', url: '', secret: '' }
    showEditApiForm.value = false
  }
}

function handleSwitchApi(id: string) {
  setActiveClashApi(id)
  syncActiveApiForm()
  refresh()
  loadClashConfig()
}

function handleSaveActiveApi() {
  const url = activeApiForm.value.url.trim()
  if (!url) {
    alert('请填写当前 API 地址。')
    return
  }
  const name = activeApiForm.value.name.trim() || 'API'
  updateActiveClashApi({
    name,
    url,
    secret: activeApiForm.value.secret,
  })
  showEditApiForm.value = false
  refresh()
  loadClashConfig()
}

function handleAddApi() {
  const url = newApiForm.value.url.trim()
  if (!url) {
    alert('请填写新增 API 地址。')
    return
  }
  const name = newApiForm.value.name.trim() || `API ${clashApis.value.length + 1}`
  const id = addClashApi(name, url, newApiForm.value.secret)
  setActiveClashApi(id)
  syncActiveApiForm()
  newApiForm.value = { name: '', url: '', secret: '' }
  showAddApiForm.value = false
  refresh()
  loadClashConfig()
}

function handleRemoveActiveApi() {
  const current = activeClashApi.value
  if (!current) return
  if (clashApis.value.length <= 1) {
    alert('至少保留一个 Clash API。')
    return
  }
  if (!confirm(`确定删除 API：${current.name} ?`)) return
  removeClashApi(current.id)
  syncActiveApiForm()
  showEditApiForm.value = false
  refresh()
  loadClashConfig()
}

function parseModeOptions(data: any): string[] {
  const modeList = Array.isArray(data?.['mode-list'])
    ? data['mode-list']
    : Array.isArray(data?.modes)
      ? data.modes
      : []
  const options = modeList.filter((mode: unknown): mode is string => typeof mode === 'string' && mode.length > 0)
  return options.length > 0 ? options : ['Rule']
}

async function loadClashConfig() {
  try {
    const { data } = await fetchConfig()
    const currentMode = typeof data.mode === 'string' && data.mode ? data.mode : 'Rule'
    const modeOptions = parseModeOptions(data)
    const matchedCurrent = modeOptions.find((mode) => mode.toLowerCase() === currentMode.toLowerCase())
    clashModeOptions.value = matchedCurrent ? modeOptions : [currentMode, ...modeOptions]
    clashMode.value = matchedCurrent ?? currentMode
  } catch {}
}

async function setMode(mode: string) {
  try {
    await patchConfig({ mode } as any)
    await loadClashConfig()
  } catch {}
}

async function validateBeforeStart(): Promise<boolean> {
  const { singboxPath, configPath } = config.value
  if (!singboxPath || !configPath) {
    pushToast({ message: '请先配置 sing-box 路径和配置文件路径', type: 'error' })
    return false
  }
  try {
    await validateSingboxConfig(singboxPath, configPath, config.value.workingDir)
    return true
  } catch (e: any) {
    pushToast({ message: '配置文件校验失败:\n' + (e?.message || e), type: 'error' }, 8000)
    return false
  }
}

async function checkServiceAfterStart() {
  await new Promise((r) => setTimeout(r, 3000))
  await refresh()
  if (serviceStatus.value.state !== 'running') {
    // 服务未运行，尝试读取错误日志
    let detail = ''
    try {
      detail = await readServiceErrorLog(config.value.serviceName)
    } catch {}
    const msg = detail
      ? '服务启动失败:\n' + detail
      : '服务启动失败或异常退出，请检查配置文件'
    pushToast({ message: msg, type: 'error' }, 10000)
    return
  }
  // 服务显示 running，再验证 Clash API 是否可达
  try {
    await fetchConfig()
  } catch {
    pushToast({
      message: '服务进程已启动但无法连接 Clash API，核心可能未正常运行，请检查配置文件',
      type: 'error',
    }, 8000)
  }
}

async function handleServiceAction(action: string) {
  actionLoading.value = action
  try {
    const name = config.value.serviceName
    switch (action) {
      case 'start':
        if (!(await validateBeforeStart())) return
        await startService(name)
        checkServiceAfterStart()
        break
      case 'restart':
        if (!(await validateBeforeStart())) return
        await stopService(name)
        await new Promise((r) => setTimeout(r, 500))
        await startService(name)
        checkServiceAfterStart()
        break
      case 'stop': await stopService(name); break
      case 'install':
        await installService(name, config.value.singboxPath, config.value.configPath, config.value.workingDir)
        break
      case 'uninstall': await uninstallService(name); break
    }
    setTimeout(refresh, 1000)
  } catch (e: any) {
    pushToast({ message: '操作失败: ' + (e?.message || e), type: 'error' }, 6000)
  } finally {
    actionLoading.value = ''
  }
}

async function checkVersion() {
  if (!config.value.singboxPath) return
  try {
    singboxVersion.value = await getSingboxVersion(config.value.singboxPath)
  } catch {
    singboxVersion.value = '获取失败'
  }
}

const statusColor = computed(() => {
  switch (serviceStatus.value.state) {
    case 'running': return 'badge-success'
    case 'stopped': return 'badge-error'
    default: return 'badge-warning'
  }
})

const statusText = computed(() => {
  const map: Record<string, string> = {
    running: '运行中',
    stopped: '已停止',
    starting: '启动中',
    stopping: '停止中',
    not_installed: '未安装',
    unknown: '未知',
  }
  return map[serviceStatus.value.state] || '未知'
})

loadClashConfig()
syncActiveApiForm()
if (serviceStatus.value.state === 'running') {
  loadProxies()
}

watch(
  () => serviceStatus.value.state,
  (newState, oldState) => {
    if (newState === 'running' && oldState !== 'running') {
      loadClashConfig()
      loadProxies()
    }
  }
)

watch(
  () => activeClashApiId.value,
  () => {
    syncActiveApiForm()
  },
)
</script>

<template>
  <div class="space-y-6 max-w-2xl">
    <h1 class="text-xl font-bold">设置</h1>

    <div class="bg-base-200 rounded-lg p-4 space-y-3">
      <h2 class="font-semibold text-sm">服务控制</h2>
      <div class="flex items-center gap-2">
        <span class="text-sm">状态:</span>
        <span class="badge" :class="statusColor">{{ statusText }}</span>
      </div>
      <div class="flex gap-2">
        <button
          class="btn btn-sm btn-success"
          :class="{ loading: actionLoading === 'start' }"
          :disabled="serviceStatus.state === 'running'"
          @click="handleServiceAction('start')"
        >
          启动
        </button>
        <button
          class="btn btn-sm btn-warning"
          :class="{ loading: actionLoading === 'restart' }"
          @click="handleServiceAction('restart')"
        >
          重启
        </button>
        <button
          class="btn btn-sm btn-error"
          :class="{ loading: actionLoading === 'stop' }"
          :disabled="serviceStatus.state === 'stopped'"
          @click="handleServiceAction('stop')"
        >
          停止
        </button>
        <div class="divider divider-horizontal"></div>
        <button
          class="btn btn-sm btn-outline"
          :class="{ loading: actionLoading === 'install' }"
          @click="handleServiceAction('install')"
        >
          安装服务
        </button>
        <button
          class="btn btn-sm btn-outline btn-error"
          :class="{ loading: actionLoading === 'uninstall' }"
          @click="handleServiceAction('uninstall')"
        >
          卸载服务
        </button>
      </div>
    </div>

    <div class="bg-base-200 rounded-lg p-4 space-y-3">
      <h2 class="font-semibold text-sm">代理模式</h2>
      <div class="form-control max-w-xs">
        <select
          class="select select-sm select-bordered"
          :value="clashMode"
          @change="setMode(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="mode in clashModeOptions" :key="mode" :value="mode">{{ mode }}</option>
        </select>
      </div>
    </div>

    <div class="bg-base-200 rounded-lg p-4 space-y-3">
      <h2 class="font-semibold text-sm">Clash API</h2>
      <div class="space-y-2">
        <label class="label py-0"><span class="label-text text-xs">当前 API</span></label>
        <div class="flex items-center gap-2">
          <select
            class="select select-sm select-bordered flex-1"
            :value="activeClashApiId"
            @change="handleSwitchApi(($event.target as HTMLSelectElement).value)"
          >
            <option v-for="api in clashApis" :key="api.id" :value="api.id">
              {{ api.url }}
            </option>
          </select>
          <button
            class="btn btn-sm btn-square btn-outline"
            :class="{ 'btn-primary': showEditApiForm }"
            title="编辑当前 API"
            @click="toggleEditApiForm"
          >
            ✎
          </button>
          <button
            class="btn btn-sm btn-square btn-outline"
            :class="{ 'btn-primary': showAddApiForm }"
            title="新增 API"
            @click="toggleAddApiForm"
          >
            +
          </button>
        </div>
      </div>

      <div v-if="showEditApiForm" class="rounded-md bg-base-100 p-3 space-y-2 border border-base-300">
        <div class="text-xs font-medium text-base-content/70">编辑当前 API</div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">名称</span></label>
          <input v-model="activeApiForm.name" type="text" class="input input-sm input-bordered" placeholder="默认 API" />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">API 地址</span></label>
          <input
            v-model="activeApiForm.url"
            type="text"
            class="input input-sm input-bordered"
            placeholder="http://127.0.0.1:9090"
          />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">密钥 (Secret)</span></label>
          <input
            v-model="activeApiForm.secret"
            type="password"
            class="input input-sm input-bordered"
            placeholder="留空表示无密钥"
          />
        </div>
        <div class="flex justify-between gap-2">
          <button
            class="btn btn-sm btn-outline btn-error"
            :disabled="clashApis.length <= 1"
            @click="handleRemoveActiveApi"
          >
            删除当前
          </button>
          <button class="btn btn-sm btn-primary" @click="handleSaveActiveApi">保存当前</button>
        </div>
      </div>

      <div v-if="showAddApiForm" class="rounded-md bg-base-100 p-3 space-y-2 border border-base-300">
        <div class="text-xs font-medium text-base-content/70">新增 API</div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">名称</span></label>
          <input v-model="newApiForm.name" type="text" class="input input-sm input-bordered" placeholder="API 2" />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">API 地址</span></label>
          <input
            v-model="newApiForm.url"
            type="text"
            class="input input-sm input-bordered"
            placeholder="http://127.0.0.1:9090"
          />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">密钥 (Secret)</span></label>
          <input
            v-model="newApiForm.secret"
            type="password"
            class="input input-sm input-bordered"
            placeholder="留空表示无密钥"
          />
        </div>
        <div class="flex justify-end">
          <button class="btn btn-sm btn-outline" @click="handleAddApi">新增并切换</button>
        </div>
      </div>
    </div>

    <div class="bg-base-200 rounded-lg p-4 space-y-3">
      <h2 class="font-semibold text-sm">测速设置</h2>
      <div class="form-control">
        <label class="label"><span class="label-text text-xs">测速地址 (URL)</span></label>
        <input
          v-model="config.latencyTestUrl"
          type="text"
          class="input input-sm input-bordered"
          placeholder="https://www.gstatic.com/generate_204"
        />
      </div>
      <div class="space-y-2">
        <div
          class="flex items-center gap-1 cursor-pointer select-none"
          @click="groupTestUrlsExpanded = !groupTestUrlsExpanded"
        >
          <svg
            class="w-3 h-3 transition-transform"
            :class="{ 'rotate-90': groupTestUrlsExpanded }"
            viewBox="0 0 12 12"
          >
            <path fill="currentColor" d="M4 2l5 4-5 4z" />
          </svg>
          <span class="label-text text-xs">组测试链接 ({{ groupTestUrlEntries.length }})</span>
        </div>
        <div v-show="groupTestUrlsExpanded" class="space-y-2 pl-4">
          <div
            v-for="[group, url] in groupTestUrlEntries"
            :key="group"
            class="flex items-center gap-1.5"
          >
            <span class="badge badge-sm badge-outline gap-1 shrink-0">
              {{ group }}
              <button class="text-base-content/40 hover:text-error" @click="removeGroupTestUrl(group)">×</button>
            </span>
            <svg class="w-3 h-3 shrink-0 text-base-content/30" viewBox="0 0 12 12">
              <path d="M2 6h6M6 3l3 3-3 3" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            <span class="text-xs truncate flex-1" :title="url">{{ url }}</span>
            <button class="btn btn-ghost btn-xs btn-square min-h-0 h-5 w-5 text-error/60 hover:text-error" @click="removeGroupTestUrl(group)" title="删除">
              <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-3.5 h-3.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
              </svg>
            </button>
          </div>
          <div class="flex items-center gap-1.5">
            <select
              v-model="newGroupTestUrl.group"
              class="select select-xs select-bordered w-28 shrink-0"
            >
              <option value="" disabled hidden></option>
              <option v-for="name in availableGroups" :key="name" :value="name">{{ name }}</option>
            </select>
            <svg class="w-3 h-3 shrink-0 text-base-content/30" viewBox="0 0 12 12">
              <path d="M2 6h6M6 3l3 3-3 3" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            <input
              v-model="newGroupTestUrl.url"
              type="text"
              class="input input-xs input-bordered flex-1"
              placeholder="测速地址"
              @keyup.enter="addGroupTestUrl"
            />
            <button
              class="btn btn-ghost btn-xs btn-square min-h-0 h-5 w-5"
              @click="addGroupTestUrl"
              title="添加"
            >+</button>
          </div>
        </div>
      </div>
      <div class="form-control">
        <label class="label cursor-pointer justify-start gap-2">
          <input
            type="checkbox"
            class="toggle toggle-sm toggle-primary"
            v-model="config.ipv6TestEnabled"
          />
          <span class="label-text text-xs">IPv6 连通性测试</span>
          <span class="text-xs text-base-content/40">测速时同时检测节点 IPv6 支持</span>
        </label>
      </div>
    </div>

    <div class="bg-base-200 rounded-lg p-4 space-y-3">
      <h2 class="font-semibold text-sm">路径配置</h2>
      <div class="form-control">
        <label class="label"><span class="label-text text-xs">服务名称</span></label>
        <input
          v-model="config.serviceName"
          type="text"
          class="input input-sm input-bordered"
          placeholder="sing-box"
        />
      </div>
      <div class="form-control">
        <label class="label"><span class="label-text text-xs">sing-box 可执行文件路径</span></label>
        <input
          v-model="config.singboxPath"
          type="text"
          class="input input-sm input-bordered"
          placeholder="C:\sing-box\sing-box.exe"
        />
      </div>
      <div class="form-control">
        <label class="label"><span class="label-text text-xs">配置文件路径</span></label>
        <input
          v-model="config.configPath"
          type="text"
          class="input input-sm input-bordered"
          placeholder="C:\sing-box\config.json"
        />
      </div>
      <div class="form-control">
        <label class="label"><span class="label-text text-xs">工作目录</span></label>
        <input
          v-model="config.workingDir"
          type="text"
          class="input input-sm input-bordered"
          placeholder="留空则使用配置文件所在目录"
        />
      </div>
      <div class="flex items-center gap-2">
        <button class="btn btn-sm btn-ghost" @click="checkVersion">检测版本</button>
        <span v-if="singboxVersion" class="text-xs text-base-content/60">{{ singboxVersion }}</span>
      </div>
    </div>

    <div class="bg-base-200 rounded-lg p-4 space-y-3">
      <h2 class="font-semibold text-sm">外观</h2>
      <div class="flex gap-2">
        <button
          v-for="t in ['light', 'dark', 'dracula', 'nord']"
          :key="t"
          class="btn btn-sm"
          :class="config.theme === t ? 'btn-primary' : 'btn-ghost'"
          @click="updateConfig({ theme: t })"
        >
          {{ t }}
        </button>
      </div>
    </div>
  </div>
</template>
