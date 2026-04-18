import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { AppConfig, ClashApiProfile, ConfigProfile } from '@/types'
import { fetchUrl, writeSingboxConfig, getRemoteConfigPath, validateSingboxConfig, validateSingboxConfigContent, copyToRunningConfig } from '@/bridge/config'
import { useToastStore } from '@/stores/toast'

const autoUpdateTimers = new Map<string, ReturnType<typeof setInterval>>()
const autoUpdateInitialDelays = new Map<string, ReturnType<typeof setTimeout>>()

const STORAGE_KEY = 'singboard-config'

function createClashApiId(): string {
  return `api_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

function createConfigProfileId(): string {
  return `cfg_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

function createClashApiProfile(name: string, url: string, secret: string): ClashApiProfile {
  return {
    id: createClashApiId(),
    name,
    url,
    secret,
  }
}

function normalizeConfig(raw: any): AppConfig {
  const normalizeWindowsPath = (value: unknown): string => {
    if (typeof value !== 'string') return ''
    if (value.startsWith('\\\\?\\UNC\\')) return `\\\\${value.slice(8)}`
    if (value.startsWith('\\\\?\\')) return value.slice(4)
    return value
  }

  const savedProfiles = Array.isArray(raw?.clashApis)
    ? raw.clashApis
      .filter((item: any) => item && typeof item === 'object')
      .map((item: any): ClashApiProfile => ({
        id: typeof item.id === 'string' && item.id ? item.id : createClashApiId(),
        name: typeof item.name === 'string' && item.name.trim() ? item.name.trim() : 'API',
        url: typeof item.url === 'string' && item.url.trim() ? item.url.trim() : 'http://127.0.0.1:9090',
        secret: typeof item.secret === 'string' ? item.secret : '',
      }))
    : []

  const legacyUrl = typeof raw?.clashApiUrl === 'string' && raw.clashApiUrl.trim()
    ? raw.clashApiUrl.trim()
    : 'http://127.0.0.1:9090'
  const legacySecret = typeof raw?.clashApiSecret === 'string' ? raw.clashApiSecret : ''

  const clashApis: ClashApiProfile[] = savedProfiles.length > 0
    ? savedProfiles
    : [createClashApiProfile('默认 API', legacyUrl, legacySecret)]

  const activeClashApiId =
    typeof raw?.activeClashApiId === 'string'
    && clashApis.some((api) => api.id === raw.activeClashApiId)
      ? raw.activeClashApiId
      : clashApis[0].id

  // 配置文件列表
  const configProfiles: ConfigProfile[] = Array.isArray(raw?.configProfiles)
    ? raw.configProfiles
      .filter((item: any) => item && typeof item === 'object' && typeof item.id === 'string')
      .map((item: any): ConfigProfile => ({
        id: item.id,
        name: typeof item.name === 'string' && item.name.trim() ? item.name.trim() : '未命名配置',
        type: item.type === 'remote' ? 'remote' : 'local',
        source: typeof item.source === 'string' ? normalizeWindowsPath(item.source) : '',
        ...(typeof item.lastUpdated === 'string' ? { lastUpdated: item.lastUpdated } : {}),
        autoUpdateInterval: typeof item.autoUpdateInterval === 'number' && item.autoUpdateInterval >= 0 ? item.autoUpdateInterval : 0,
      }))
    : []

  // 迁移兼容：旧 configPath 自动创建本地配置
  const legacyConfigPath = normalizeWindowsPath(raw?.configPath)
  if (configProfiles.length === 0 && legacyConfigPath) {
    configProfiles.push({
      id: createConfigProfileId(),
      name: '默认配置',
      type: 'local',
      source: legacyConfigPath,
      autoUpdateInterval: 0,
    })
  }

  const activeConfigProfileId =
    typeof raw?.activeConfigProfileId === 'string'
    && configProfiles.some((p) => p.id === raw.activeConfigProfileId)
      ? raw.activeConfigProfileId
      : configProfiles.length > 0 ? configProfiles[0].id : ''

  return {
    clashApis,
    activeClashApiId,
    singboxPath: normalizeWindowsPath(raw?.singboxPath),
    workingDir: normalizeWindowsPath(raw?.workingDir),
    serviceName: typeof raw?.serviceName === 'string' && raw.serviceName ? raw.serviceName : 'sing-box',
    theme: typeof raw?.theme === 'string' && raw.theme ? raw.theme : 'light',
    latencyTestUrl: typeof raw?.latencyTestUrl === 'string' && raw.latencyTestUrl
      ? raw.latencyTestUrl
      : 'https://www.gstatic.com/generate_204',
    ipv6TestEnabled: typeof raw?.ipv6TestEnabled === 'boolean' ? raw.ipv6TestEnabled : false,
    groupTestUrls: raw?.groupTestUrls && typeof raw.groupTestUrls === 'object' && !Array.isArray(raw.groupTestUrls)
      ? Object.fromEntries(
          Object.entries(raw.groupTestUrls as Record<string, unknown>).filter((e): e is [string, string] => typeof e[1] === 'string' && e[1].length > 0)
        )
      : {},
    configProfiles,
    activeConfigProfileId,
    closeToTray: typeof raw?.closeToTray === 'boolean' ? raw.closeToTray : false,
    // --- 只需要在这里加入下面这一行 ---
    selfProxy: typeof raw?.selfProxy === 'string' ? raw.selfProxy : '',
  }
}

function loadConfig(): AppConfig {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved) {
      return normalizeConfig(JSON.parse(saved))
    }
  } catch { }
  return normalizeConfig({})
}

const config = ref<AppConfig>(loadConfig())

function applyTheme(theme: string) {
  document.documentElement.setAttribute('data-theme', theme)
}

applyTheme(config.value.theme)

watch(config, (val) => {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(val))
  applyTheme(val.theme)
}, { deep: true })

// 同步 closeToTray 状态到 Rust 后端
invoke('set_close_to_tray', { enabled: config.value.closeToTray }).catch(() => {})
watch(() => config.value.closeToTray, (val) => {
  invoke('set_close_to_tray', { enabled: val }).catch(() => {})
})

// 同步自身代理状态到 Rust 后端
invoke('set_self_proxy', { proxy: config.value.selfProxy }).catch(() => {})
watch(() => config.value.selfProxy, (val) => {
  invoke('set_self_proxy', { proxy: val }).catch(() => {})
})

// 核心逻辑移到外部：变成“后台服务”，增加临时校验环节
async function performUpdate(id: string) {
  const profile = config.value.configProfiles.find(p => p.id === id)
  if (!profile || profile.type !== 'remote') return

  // 在函数内部获取 toast，避免初始化顺序错误
  const { pushToast } = useToastStore()

  try {
    console.log(`[AutoUpdate] 正在执行: ${profile.name}`)
    const content = await fetchUrl(profile.source)
    const destPath = await getRemoteConfigPath(profile.id)

    // ✅ 先用 sing-box check 校验拉取到的内容，通过后才写入文件
    const sp = config.value.singboxPath
    const wd = config.value.workingDir
    if (sp) {
      await validateSingboxConfigContent(sp, destPath, content, wd)
    }

    // ✅ 校验通过，才允许覆盖写入
    await writeSingboxConfig(destPath, content)
    
    // 直接操作全局的 config ref
    profile.lastUpdated = new Date().toISOString()

    if (id === config.value.activeConfigProfileId) {
      if (sp) {
        await copyToRunningConfig(destPath)
      }
    }
    pushToast({ message: `更新成功: ${profile.name}`, type: 'info' })
  } catch (e: any) {
    console.error(`[AutoUpdate] 失败:`, e)
    pushToast({ message: `更新失败: ${profile.name}`, type: 'error' })
  }
}

function setupAutoUpdate() {
  // 1. 清理现有所有定时器
  autoUpdateTimers.forEach(t => clearInterval(t))
  autoUpdateTimers.clear()
  autoUpdateInitialDelays.forEach(t => clearTimeout(t))
  autoUpdateInitialDelays.clear()

  const now = Date.now()

  config.value.configProfiles.forEach(profile => {
    if (profile.type === 'remote' && profile.autoUpdateInterval > 0) {
      const intervalMs = profile.autoUpdateInterval * 3600_000
      const lastUpdatedMs = profile.lastUpdated ? new Date(profile.lastUpdated).getTime() : 0
      
      // 计算距离下次更新还剩多少时间
      const elapsed = now - lastUpdatedMs
      let remaining = intervalMs - elapsed

      // 如果已经超时，或者从未更新过，设置 10 秒后执行（给软件启动留一点缓冲时间）
      if (remaining <= 0) {
        console.log(`[AutoUpdate] ${profile.name} 已过期，将在 10 秒后触发补偿更新`)
        remaining = 10_000 
      }

      // 启动逻辑：先执行一次延迟任务，完成后开启常规循环
      const delayTimer = setTimeout(async () => {
        await performUpdate(profile.id)
        
        // 第一次补偿更新完成后，开启正常的周期循环
        const intervalTimer = setInterval(() => performUpdate(profile.id), intervalMs)
        autoUpdateTimers.set(profile.id, intervalTimer)
      }, remaining)

      autoUpdateInitialDelays.set(profile.id, delayTimer)
      
      console.log(`[AutoUpdate] ${profile.name} 调度成功：${(remaining / 1000).toFixed(1)} 秒后首次执行，后续间隔 ${profile.autoUpdateInterval} 小时`)
    }
  })
}

// 唯一的全局监听器：确保定时器不被意外重置
watch(
  () => config.value.configProfiles.map(p => `${p.id}-${p.source}-${p.autoUpdateInterval}`).join('|'),
  (newVal, oldVal) => {
    if (newVal !== oldVal) {
      setupAutoUpdate()
    }
  },
  { immediate: true }
)

export function useConfigStore() {
  const clashApis = computed(() => config.value.clashApis)
  const activeClashApiId = computed(() => config.value.activeClashApiId)
  const activeClashApi = computed(() =>
    config.value.clashApis.find((api) => api.id === config.value.activeClashApiId)
    ?? config.value.clashApis[0],
  )
  const clashApiUrl = computed(() => activeClashApi.value?.url ?? '')
  const clashApiSecret = computed(() => activeClashApi.value?.secret ?? '')
  const serviceName = computed(() => config.value.serviceName)

  const configProfiles = computed(() => config.value.configProfiles)
  const activeConfigProfileId = computed(() => config.value.activeConfigProfileId)
  const activeConfigProfile = computed(() =>
    config.value.configProfiles.find((p) => p.id === config.value.activeConfigProfileId),
  )

  function updateConfig(partial: Partial<AppConfig>) {
    config.value = normalizeConfig({ ...config.value, ...partial })
  }

  function setActiveClashApi(id: string) {
    if (config.value.clashApis.some((api) => api.id === id)) {
      config.value.activeClashApiId = id
    }
  }

  function addClashApi(name: string, url: string, secret: string): string {
    const profile = createClashApiProfile(name, url, secret)
    config.value.clashApis.push(profile)
    return profile.id
  }

  function updateActiveClashApi(partial: Partial<Omit<ClashApiProfile, 'id'>>) {
    const current = activeClashApi.value
    if (!current) return
    if (typeof partial.name === 'string') current.name = partial.name
    if (typeof partial.url === 'string') current.url = partial.url
    if (typeof partial.secret === 'string') current.secret = partial.secret
  }

  function removeClashApi(id: string): boolean {
    if (config.value.clashApis.length <= 1) return false
    const index = config.value.clashApis.findIndex((api) => api.id === id)
    if (index === -1) return false
    const removingActive = config.value.activeClashApiId === id
    config.value.clashApis.splice(index, 1)
    if (removingActive) {
      config.value.activeClashApiId = config.value.clashApis[0].id
    }
    return true
  }

  function setSingleClashApi(url: string, secret: string, name = '默认 API') {
    const profile = createClashApiProfile(name, url, secret)
    config.value.clashApis = [profile]
    config.value.activeClashApiId = profile.id
  }

  function addConfigProfile(name: string, type: 'local' | 'remote', source: string, autoUpdateInterval = 0): string {
    const profile: ConfigProfile = {
      id: createConfigProfileId(),
      name,
      type,
      source,
      lastUpdated: new Date().toISOString(),
      autoUpdateInterval,
    }
    config.value.configProfiles.push(profile)
    return profile.id
  }

  function removeConfigProfile(id: string): boolean {
    const index = config.value.configProfiles.findIndex((p) => p.id === id)
    if (index === -1) return false
    config.value.configProfiles.splice(index, 1)
    if (config.value.activeConfigProfileId === id) {
      config.value.activeConfigProfileId = config.value.configProfiles.length > 0
        ? config.value.configProfiles[0].id
        : ''
    }
    return true
  }

  function setActiveConfigProfile(id: string) {
    if (config.value.configProfiles.some((p) => p.id === id)) {
      config.value.activeConfigProfileId = id
    }
  }

  function updateConfigProfile(id: string, partial: Partial<Omit<ConfigProfile, 'id'>>) {
    const profile = config.value.configProfiles.find((p) => p.id === id)
    if (!profile) return
    Object.assign(profile, partial)
  }

  // 把手动更新的逻辑也统一到这里，避免代码重复
  async function manualUpdateRemote(id: string) {
    await performUpdate(id)
  }

  return {
    config,
    clashApis,
    activeClashApi,
    activeClashApiId,
    clashApiUrl,
    clashApiSecret,
    serviceName,
    configProfiles,
    activeConfigProfileId,
    activeConfigProfile,
    updateConfig,
    setActiveClashApi,
    addClashApi,
    updateActiveClashApi,
    removeClashApi,
    setSingleClashApi,
    addConfigProfile,
    removeConfigProfile,
    setActiveConfigProfile,
    updateConfigProfile,
    manualUpdateRemote,
    setupAutoUpdate,
  }
}
