export interface PluginBridge {
  readonly version: number
  readonly theme: 'light' | 'dark'
  request(input: { method: string; path: string; contentType?: string; body?: string }): Promise<{ status: number; contentType: string; body: ArrayBuffer }>
}
declare global { interface Window { readonly codexProxyPlugin?: PluginBridge } }

export interface Status {
  running: boolean
  baseUrl: string
  dataDirectory: string
  accounts: { id: string; email: string | null; status: string | null; disabled: boolean | null; unavailable: boolean | null }[]
  models: string[]
  engineVersion: string
  googleModuleVersion: string
  pendingLoginUrl: string | null
  pendingLoginProvider: 'antigravity' | 'gemini-cli' | null
  lastLoginError: string | null
}

export async function request<T>(method: 'GET' | 'POST', path: string, input?: unknown): Promise<T> {
  const bridge = window.codexProxyPlugin
  if (!bridge) throw new Error('请从 codex-proxy-rs 的插件页面打开')
  const response = await bridge.request({ method, path, ...(input === undefined ? {} : { contentType: 'application/json', body: JSON.stringify(input) }) })
  if (response.contentType.split(';')[0]?.trim() !== 'application/json') throw new Error('插件响应类型无效')
  const data = JSON.parse(new TextDecoder().decode(response.body))
  if (response.status < 200 || response.status >= 300) throw new Error(data?.error?.message ?? `插件请求失败（HTTP ${response.status}）`)
  return data as T
}
