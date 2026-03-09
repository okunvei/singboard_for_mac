import { invoke } from '@tauri-apps/api/core'

export interface DetectedRuntimeFiles {
  baseDir: string
  singboxPath?: string
  configPath?: string
  found: boolean
}

export async function readSingboxConfig(path: string): Promise<string> {
  return invoke<string>('read_config', { path })
}

export async function writeSingboxConfig(path: string, content: string): Promise<void> {
  return invoke('write_config', { path, content })
}

export async function validateSingboxConfig(
  singboxPath: string,
  configPath: string,
  workingDir?: string,
): Promise<string> {
  return invoke<string>('validate_config', { singboxPath, configPath, workingDir })
}

export async function validateSingboxConfigContent(
  singboxPath: string,
  configPath: string,
  content: string,
  workingDir?: string,
): Promise<string> {
  return invoke<string>('validate_config_content', { singboxPath, configPath, content, workingDir })
}

export async function getSingboxVersion(singboxPath: string): Promise<string> {
  return invoke<string>('get_singbox_version', { singboxPath })
}

export async function detectRuntimeFiles(baseDir?: string): Promise<DetectedRuntimeFiles> {
  return invoke<DetectedRuntimeFiles>('detect_runtime_files', { baseDir })
}

export async function srsMatch(path: string, query: string): Promise<boolean> {
  return invoke<boolean>('srs_match', { path, query })
}

export async function srsMatchProvider(
  workingDir: string,
  configPath: string,
  singboxPath: string,
  tag: string,
  query: string,
): Promise<boolean> {
  return invoke<boolean>('srs_match_provider', { workingDir, configPath, singboxPath, tag, query })
}
