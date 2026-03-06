import axios from 'axios'
import { useConfigStore } from '@/stores/config'
import type {
  ProxiesData,
  RulesData,
  ConnectionsSnapshot,
  ClashConfig,
  ProxyProvider,
  RuleProvider,
  RuleProviderDetail,
} from '@/types'

const api = axios.create({
  timeout: 15000,
})

api.interceptors.request.use((cfg) => {
  const { clashApiUrl, clashApiSecret } = useConfigStore()
  cfg.baseURL = clashApiUrl.value
  if (clashApiSecret.value) {
    cfg.headers.Authorization = `Bearer ${clashApiSecret.value}`
  }
  return cfg
})

export const fetchVersion = () => api.get<{ version: string }>('/version')

export const fetchProxies = () => api.get<ProxiesData>('/proxies')
export const fetchProxy = (name: string) => api.get(`/proxies/${encodeURIComponent(name)}`)
export const selectProxy = (group: string, name: string) =>
  api.put(`/proxies/${encodeURIComponent(group)}`, { name })
export const testLatency = (name: string, url: string, timeout: number) =>
  api.get<{ delay: number }>(`/proxies/${encodeURIComponent(name)}/delay`, {
    params: { url, timeout },
  })
export const testGroupLatency = (name: string, url: string, timeout: number) =>
  api.get(`/group/${encodeURIComponent(name)}/delay`, {
    params: { url, timeout },
  })

export const fetchRules = () => api.get<RulesData>('/rules')

export const fetchConnections = () => api.get<ConnectionsSnapshot>('/connections')
export const disconnectAll = () => api.delete('/connections')
export const disconnectById = (id: string) => api.delete(`/connections/${id}`)

export const fetchConfig = () => api.get<ClashConfig>('/configs')
export const patchConfig = (config: Partial<ClashConfig>) =>
  api.patch('/configs', config)

export const flushDnsCache = () => api.post('/cache/dns/flush')
export const flushFakeIpCache = () => api.post('/cache/fakeip/flush')

export const fetchProxyProviders = () =>
  api.get<{ providers: Record<string, ProxyProvider> }>('/providers/proxies')
export const updateProxyProvider = (name: string) =>
  api.put(`/providers/proxies/${encodeURIComponent(name)}`, null, { timeout: 120000 })
export const healthCheckProvider = (name: string) =>
  api.get(`/providers/proxies/${encodeURIComponent(name)}/healthcheck`, { timeout: 30000 })
export const fetchRuleProviders = () =>
  api.get<{ providers: Record<string, RuleProvider> }>('/providers/rules')
export const fetchRuleProviderDetail = (name: string) =>
  api.get<RuleProviderDetail>(`/providers/rules/${encodeURIComponent(name)}`)
export const updateRuleProvider = (name: string) =>
  api.put(`/providers/rules/${encodeURIComponent(name)}`, null, { timeout: 120000 })

export default api
