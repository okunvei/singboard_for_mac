import { invoke } from '@tauri-apps/api/core'

export interface IPInfo {
  ip: string
  country: string
  organization: string
}

async function fetchText(url: string): Promise<string> {
  return invoke<string>('fetch_url', { url })
}

async function fetchJson(url: string): Promise<any> {
  const text = await fetchText(url)
  return JSON.parse(text)
}

export async function getIPFromIpipnet() {
  const text = await fetchText('http://myip.ipip.net?t=' + Date.now())
  const ipMatch = text.match(/IP[：:]\s*(\S+)/)
  const locMatch = text.match(/来自于[：:]\s*(.+)/)
  const ip = ipMatch?.[1] ?? ''
  const location = locMatch?.[1]?.trim().split(/\s+/).filter(Boolean) ?? []
  return { ip, location }
}

export interface IPGeoInfo {
  ip: string
  asn: number | null
  asnOrganization: string
  city: string
  region: string
  country: string
  organization: string
}

const geoipCache = new Map<string, IPGeoInfo>()

export async function getGeoIPForIP(ip: string): Promise<IPGeoInfo> {
  const cached = geoipCache.get(ip)
  if (cached) return cached

  const data = await fetchJson(`https://api.ip.sb/geoip/${ip}`)
  const info: IPGeoInfo = {
    ip: data.ip ?? ip,
    asn: data.asn ?? null,
    asnOrganization: data.asn_organization ?? '',
    city: data.city ?? '',
    region: data.region ?? '',
    country: data.country ?? '',
    organization: data.organization ?? '',
  }
  geoipCache.set(ip, info)
  return info
}

export async function getIPFromIpsb(): Promise<IPInfo> {
  const data = await fetchJson('https://api.ip.sb/geoip?t=' + Date.now())
  return {
    ip: data.ip,
    country: data.country,
    organization: data.organization,
  }
}
