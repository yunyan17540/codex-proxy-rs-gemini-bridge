import { onUnmounted, shallowRef } from 'vue'
import { request } from './api'

export function useGoogleLogin(onComplete: () => void) {
  const url = shallowRef('')
  const busy = shallowRef(false)
  const waiting = shallowRef(false)
  const error = shallowRef('')
  const message = shallowRef('')
  let timer: ReturnType<typeof setTimeout> | undefined
  let disposed = false
  const stop = () => { if (timer) clearTimeout(timer); timer = undefined }

  async function poll() {
    if (disposed || !waiting.value) return
    try {
      const result = await request<{ status: string; message: string }>('POST', 'api/poll', {})
      if (disposed) return
      if (result.status === 'ok') { waiting.value = false; url.value = ''; message.value = result.message || 'Google 授权完成'; onComplete(); return }
      if (result.status === 'error') { waiting.value = false; url.value = ''; error.value = result.message; return }
      timer = setTimeout(poll, 2000)
    } catch (cause) { if (!disposed) { waiting.value = false; error.value = cause instanceof Error ? cause.message : String(cause) } }
  }
  async function start(provider: 'antigravity' | 'gemini-cli', projectId: string, accountId = '') {
    error.value = ''; message.value = ''; busy.value = true
    try {
      const result = await request<{ url: string }>('POST', 'api/login', { provider, project_id: projectId, account_id: accountId })
      if (disposed) return
      url.value = result.url; waiting.value = true; stop(); void poll()
    } catch (cause) { if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause) }
    finally { if (!disposed) busy.value = false }
  }
  function resume(value: string) {
    if (waiting.value || disposed) return
    url.value = value; waiting.value = true; stop(); void poll()
  }
  async function open() {
    busy.value = true; error.value = ''
    try { await request('POST', 'api/open-login', {}) }
    catch (cause) { if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause) }
    finally { if (!disposed) busy.value = false }
  }
  async function cancel() {
    busy.value = true; error.value = ''; stop()
    try {
      await request('POST', 'api/cancel', {})
      if (!disposed) { waiting.value = false; url.value = ''; message.value = '已取消授权' }
    } catch (cause) { if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause) }
    finally { if (!disposed) busy.value = false }
  }
  async function callback(value: string) {
    busy.value = true; error.value = ''
    try {
      await request('POST', 'api/callback', { redirect_url: value })
      if (!disposed) { message.value = '已提交回调，等待 Google 授权结果'; waiting.value = true; stop(); void poll() }
    } catch (cause) { if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause) }
    finally { if (!disposed) busy.value = false }
  }
  onUnmounted(() => { disposed = true; stop() })
  return { url, busy, waiting, error, message, start, cancel, callback, open, resume }
}
