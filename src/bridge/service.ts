import { invoke } from '@tauri-apps/api/core'
import type { ServiceStatus } from '@/types'

export async function queryServiceStatus(serviceName: string): Promise<ServiceStatus> {
  return invoke<ServiceStatus>('service_status', { serviceName })
}

export async function startService(serviceName: string): Promise<void> {
  return invoke('service_start', { serviceName })
}

export async function stopService(serviceName: string): Promise<void> {
  return invoke('service_stop', { serviceName })
}

export async function restartService(serviceName: string): Promise<void> {
  return invoke('service_restart', { serviceName })
}

export async function installService(
  serviceName: string,
  singboxPath: string,
  configPath: string,
  workingDir: string,
): Promise<void> {
  return invoke('service_install', { serviceName, singboxPath, configPath, workingDir })
}

export async function uninstallService(serviceName: string): Promise<void> {
  return invoke('service_uninstall', { serviceName })
}

export async function readServiceErrorLog(serviceName: string): Promise<string> {
  return invoke<string>('service_error_log', { serviceName })
}

/** 检查特权 Helper 是否正在运行 */
export async function helperRunning(): Promise<boolean> {
  return invoke<boolean>('helper_running')
}

/** 通过 Helper 清除 macOS 系统代理 */
export async function clearSystemProxy(): Promise<void> {
  return invoke('clear_system_proxy')
}
