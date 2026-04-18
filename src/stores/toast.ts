import { ref } from 'vue'

export interface ToastPayload {
  message: string
  type?: 'error' | 'info'
}

export interface ToastItem extends ToastPayload {
  id: number
  duration: number
  remainingMs: number
  paused: boolean
  lastTick: number
}

const toasts = ref<ToastItem[]>([])
let toastId = 0
let ticker: ReturnType<typeof setInterval> | null = null

function stopTicker() {
  if (ticker) {
    clearInterval(ticker)
    ticker = null
  }
}

function ensureTicker() {
  if (ticker) {
    return
  }
  ticker = setInterval(() => {
    if (toasts.value.length === 0) {
      stopTicker()
      return
    }

    const now = Date.now()
    const expiredIds: number[] = []

    for (const toast of toasts.value) {
      if (toast.paused) {
        toast.lastTick = now
        continue
      }
      const elapsed = now - toast.lastTick
      if (elapsed <= 0) {
        continue
      }
      toast.remainingMs = Math.max(0, toast.remainingMs - elapsed)
      toast.lastTick = now
      if (toast.remainingMs <= 0) {
        expiredIds.push(toast.id)
      }
    }

    if (expiredIds.length > 0) {
      toasts.value = toasts.value.filter((toast) => !expiredIds.includes(toast.id))
    }
    if (toasts.value.length === 0) {
      stopTicker()
    }
  }, 50)
}

export function useToastStore() {
  function removeToast(id: number) {
    toasts.value = toasts.value.filter((toast) => toast.id !== id)
    if (toasts.value.length === 0) {
      stopTicker()
    }
  }

  function pushToast(input: ToastPayload, duration = 5000) {
    const now = Date.now()
    const id = ++toastId
    toasts.value.push({
      id,
      ...input,
      duration,
      remainingMs: duration,
      paused: false,
      lastTick: now,
    })
    ensureTicker()
  }

  function pauseToast(id: number) {
    const toast = toasts.value.find((item) => item.id === id)
    if (!toast || toast.paused) {
      return
    }
    const now = Date.now()
    const elapsed = now - toast.lastTick
    if (elapsed > 0) {
      toast.remainingMs = Math.max(0, toast.remainingMs - elapsed)
      toast.lastTick = now
    }
    toast.paused = true
    if (toast.remainingMs <= 0) {
      removeToast(id)
    }
  }

  function resumeToast(id: number) {
    const toast = toasts.value.find((item) => item.id === id)
    if (!toast || !toast.paused) {
      return
    }
    toast.paused = false
    toast.lastTick = Date.now()
    ensureTicker()
  }

  return {
    toasts,
    pushToast,
    removeToast,
    pauseToast,
    resumeToast,
  }
}
