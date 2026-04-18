import { invoke } from '@tauri-apps/api/core'

async function httpPing(url: string): Promise<number> {
  try {
    return await invoke<number>('http_ping', { url, count: 3 })
  } catch {
    return 0
  }
}

export const getWechatLatency = () => httpPing('https://res.wx.qq.com/favicon.ico')

export const getBilibiliLatency = () => httpPing('https://i0.hdslb.com/favicon.ico')

export const getGithubLatency = () => httpPing('https://github.github.io')

export const getCloudflareLatency = () => httpPing('https://www.cloudflare.com/favicon.ico')

export const getYoutubeLatency = () => httpPing('https://yt3.ggpht.com/favicon.ico')
