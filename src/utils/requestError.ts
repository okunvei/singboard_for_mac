import axios from 'axios'

function normalizeReasonText(text: string): string {
  const normalized = text.replace(/\s+/g, ' ').trim()
  return normalized || '请求失败'
}

function extractReasonFromData(data: unknown): string | null {
  if (typeof data === 'string' && data.trim().length > 0) {
    return normalizeReasonText(data)
  }
  if (data && typeof data === 'object') {
    const record = data as Record<string, unknown>
    const message =
      (typeof record.message === 'string' && record.message)
      || (typeof record.error === 'string' && record.error)
      || (typeof record.msg === 'string' && record.msg)
    if (message && message.trim().length > 0) {
      return normalizeReasonText(message)
    }
  }
  return null
}

export function getRequestErrorReason(error: unknown): string {
  if (axios.isAxiosError(error)) {
    const dataReason = extractReasonFromData(error.response?.data)
    if (dataReason) {
      return dataReason
    }

    if (typeof error.message === 'string' && error.message.trim().length > 0) {
      return normalizeReasonText(error.message)
    }

    if (error.code === 'ECONNABORTED') {
      return '请求超时'
    }
    if (error.code === 'ERR_NETWORK') {
      return '网络错误'
    }
    return '请求失败'
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return normalizeReasonText(error.message)
  }
  if (typeof error === 'string' && error.trim().length > 0) {
    return normalizeReasonText(error)
  }
  return '未知错误'
}
