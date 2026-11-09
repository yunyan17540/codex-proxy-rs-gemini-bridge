<script setup lang="ts">
import { onMounted, onUnmounted, shallowRef } from 'vue'
import { ZButton } from '@codex-proxy/ui/button'
import { ZCard } from '@codex-proxy/ui/card'
import { ZConfigProvider } from '@codex-proxy/ui/config-provider'
import { zhHans } from '@codex-proxy/ui/locale'
import GoogleLogin from './GoogleLogin.vue'
import { request, type Status } from './api'

const state = shallowRef<Status | null>(null)
const loading = shallowRef(false)
const linking = shallowRef(false)
const error = shallowRef('')
const notice = shallowRef('')
let disposed = false
async function load() {
  loading.value = true; error.value = ''
  try { const value = await request<Status>('GET', 'api/status'); if (!disposed) state.value = value }
  catch (cause) { if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause) }
  finally { if (!disposed) loading.value = false }
}
async function connect() {
  linking.value = true; error.value = ''; notice.value = ''
  try {
    const result = await request<{ groupName: string; message: string }>('POST', 'api/connect', {})
    if (!disposed) notice.value = `${result.message}：${result.groupName}`
  } catch (cause) { if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause) }
  finally { if (!disposed) linking.value = false }
}
onMounted(load)
onUnmounted(() => { disposed = true })
</script>

<template>
  <ZConfigProvider :locale="zhHans">
    <main class="stack">
      <ZCard padding="none" class="panel">
        <div class="bar">
          <div class="metrics">
            <div class="metric"><strong>{{ state?.running ? '运行中' : loading ? '连接中' : '未连接' }}</strong><span>本机 Gemini 引擎</span></div>
            <div class="metric"><strong>{{ state?.accounts.length ?? '—' }}</strong><span>Google 账号</span></div>
            <div class="metric"><strong>{{ state?.models.length ?? '—' }}</strong><span>Gemini 模型</span></div>
          </div>
          <ZButton :loading="loading" :disabled="linking" @click="load">刷新状态</ZButton>
        </div>
      </ZCard>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <p v-if="notice" class="notice" role="status">{{ notice }}</p>
      <GoogleLogin :pending-url="state?.pendingLoginUrl ?? null" :pending-provider="state?.pendingLoginProvider ?? null" :last-error="state?.lastLoginError ?? null" @complete="load" />
      <ZCard padding="none" class="panel">
        <h2 class="section-title">已授权账号</h2>
        <ul v-if="state?.accounts.length" class="rows"><li v-for="account in state.accounts" :key="account.id" class="account"><div>{{ account.email || account.id }}</div><span class="muted">{{ account.disabled ? '已停用' : account.unavailable ? '暂不可用' : account.status || '已授权' }}</span></li></ul>
        <p v-else class="fine-print">暂无账号，完成 Google 授权后刷新</p>
      </ZCard>
      <ZCard padding="none" class="panel">
        <div class="bar"><h2 class="section-title">连接到现有网关</h2><ZButton type="primary" :loading="linking" :disabled="!state?.models.length || loading" @click="connect">同步模型并连接</ZButton></div>
        <p class="fine-print">创建本插件的 Gemini 上游与独立分组，仅允许下方模型</p>
        <div v-if="state?.models.length" class="model-list"><code v-for="model in state.models" :key="model" class="model">{{ model }}</code></div>
        <p v-else class="muted">尚无可用模型，需先完成 Google 授权</p>
      </ZCard>
      <details class="muted"><summary>本机数据与运行说明</summary><p>引擎 {{ state?.engineVersion }} · Google 模块 {{ state?.googleModuleVersion }}</p><p>账号数据：{{ state?.dataDirectory ?? '以插件配置为准' }}</p><p>停用插件会停止 Gemini 引擎并中断未完成的 Gemini 请求，账号数据会保留</p><p>若使用密钥限制了分组，需要把本插件生成的 Gemini 分组加入该密钥的可用范围</p></details>
    </main>
  </ZConfigProvider>
</template>
