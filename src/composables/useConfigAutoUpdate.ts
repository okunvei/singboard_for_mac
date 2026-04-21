import { watch, onUnmounted } from 'vue'
import { useConfigStore } from '@/stores/config'
import { useToastStore } from '@/stores/toast'
import { fetchUrl, writeSingboxConfig, getRemoteConfigPath, validateSingboxConfig, copyToRunningConfig } from '@/bridge/config'
import type { ConfigProfile } from '@/types'

export function useConfigAutoUpdate() {
  const { config, configProfiles, activeConfigProfileId, updateConfigProfile } = useConfigStore()
  const { pushToast } = useToastStore()

  // timeoutId for initial delay + intervalId for recurring
  const timers = new Map<string, { timeout?: ReturnType<typeof setTimeout>; interval?: ReturnType<typeof setInterval> }>()

  async function updateRemoteProfile(profile: ConfigProfile) {
    try {
      const content = await fetchUrl(profile.source)
      const destPath = await getRemoteConfigPath(profile.id)
      await writeSingboxConfig(destPath, content)
      updateConfigProfile(profile.id, { lastUpdated: new Date().toISOString() })

      if (profile.id === activeConfigProfileId.value) {
        const sp = config.value.singboxPath
        if (sp) {
          try {
            await validateSingboxConfig(sp, destPath, config.value.workingDir)
            await copyToRunningConfig(destPath)
          } catch (e: any) {
            pushToast({ message: `自动更新「${profile.name}」配置校验失败: ` + (e?.message || e), type: 'error' }, 8000)
            return
          }
        }
      }
      pushToast({ message: `远程配置「${profile.name}」已自动更新`, type: 'info' })
    } catch (e: any) {
      pushToast({ message: `自动更新「${profile.name}」失败: ` + (e?.message || e), type: 'error' })
    }
  }

  function clearTimer(id: string) {
    const t = timers.get(id)
    if (!t) return
    if (t.timeout != null) clearTimeout(t.timeout)
    if (t.interval != null) clearInterval(t.interval)
    timers.delete(id)
  }

  function scheduleProfile(profile: ConfigProfile) {
    clearTimer(profile.id)
    if (profile.type !== 'remote' || profile.autoUpdateInterval <= 0) return

    const intervalMs = profile.autoUpdateInterval * 3_600_000
    const lastUpdated = profile.lastUpdated ? new Date(profile.lastUpdated).getTime() : 0
    const elapsed = Date.now() - lastUpdated
    const initialDelay = Math.max(0, intervalMs - elapsed)

    const startInterval = () => {
      updateRemoteProfile(profile)
      const iv = setInterval(() => updateRemoteProfile(profile), intervalMs)
      timers.set(profile.id, { interval: iv })
    }

    if (initialDelay === 0) {
      startInterval()
    } else {
      const to = setTimeout(startInterval, initialDelay)
      timers.set(profile.id, { timeout: to })
    }
  }

  function syncTimers(profiles: ConfigProfile[]) {
    const activeIds = new Set(
      profiles
        .filter((p) => p.type === 'remote' && p.autoUpdateInterval > 0)
        .map((p) => p.id),
    )

    // Remove timers for profiles no longer needing auto-update
    for (const id of timers.keys()) {
      if (!activeIds.has(id)) clearTimer(id)
    }

    // Add/update timers — only reschedule if interval or source changed
    for (const profile of profiles) {
      if (!activeIds.has(profile.id)) continue
      const existing = timers.get(profile.id)
      // If no timer exists yet, schedule it; interval/source changes are handled by the watcher
      if (!existing) scheduleProfile(profile)
    }
  }

  // Watch for profile list changes (add/remove) and interval/source changes
  const stopWatch = watch(
    () =>
      configProfiles.value
        .filter((p) => p.type === 'remote')
        .map((p) => ({ id: p.id, interval: p.autoUpdateInterval, source: p.source })),
    (curr, prev) => {
      const prevMap = new Map((prev ?? []).map((p) => [p.id, p]))
      for (const p of curr) {
        const old = prevMap.get(p.id)
        if (!old || old.interval !== p.interval || old.source !== p.source) {
          // Reschedule — find full profile
          const full = configProfiles.value.find((x) => x.id === p.id)
          if (full) scheduleProfile(full)
        }
      }
      // Remove stale timers
      const currIds = new Set(curr.map((p) => p.id))
      for (const id of timers.keys()) {
        if (!currIds.has(id)) clearTimer(id)
      }
    },
    { deep: true },
  )

  function start() {
    syncTimers(configProfiles.value)
  }

  function stop() {
    stopWatch()
    for (const id of [...timers.keys()]) clearTimer(id)
  }

  onUnmounted(stop)

  return { start, stop }
}
