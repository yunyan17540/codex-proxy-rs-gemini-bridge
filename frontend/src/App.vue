<script setup lang="ts">
import { onMounted, onUnmounted, shallowRef } from 'vue'
import { ZButton } from '@codex-proxy/ui/button'
import { ZCard } from '@codex-proxy/ui/card'
import { ZConfigProvider } from '@codex-proxy/ui/config-provider'
import { zhHans } from '@codex-proxy/ui/locale'
import GoogleLogin from './GoogleLogin.vue'
import { request, type Status, type GoogleAccount } from './api'

const state = shallowRef<Status | null>(null)
const loading = shallowRef(false)
const linking = shallowRef(false)
const error = shallowRef('')
const notice = shallowRef('')
const login = shallowRef<InstanceType<typeof GoogleLogin> | null>(null)
const loginPending = shallowRef(false)
const acting = shallowRef('')
const confirmDelete = shallowRef('')
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
    await load()
  } catch (cause) { if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause) }
  finally { if (!disposed) linking.value = false }
}
async function accountAction(account: GoogleAccount, action: 'connect' | 'delete') {
  acting.value = account.id; error.value = ''; notice.value = ''
  try {
    const result = await request<{ message: string }>('POST', `api/accounts/${action}`, { account_id: account.id })
    if (!disposed) { notice.value = result.message; confirmDelete.value = '' }
    await load()
  } catch (cause) { if (!disposed) error.value = cause instanceof Error ? cause.message : String(cause) }
  finally { if (!disposed) acting.value = '' }
}
function reauthorize(account: GoogleAccount) {
  confirmDelete.value = ''
  void login.value?.reauthorize(account)
  document.querySelector('#google-login')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
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
          <ZButton :loading="loading" :disabled="linking || !!acting" @click="load">刷新状态</ZButton>
        </div>
      </ZCard>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <p v-if="notice" class="notice" role="status">{{ notice }}</p>
      <GoogleLogin id="google-login" ref="login" :pending-url="state?.pendingLoginUrl ?? null" :pending-provider="state?.pendingLoginProvider ?? null" :pending-account="state?.accounts.find(a => a.id === state?.pendingLoginAccount) ?? null" :last-error="state?.lastLoginError ?? null" @complete="load" @pending="loginPending = $event" />
      <ZCard padding="none" class="panel">
        <h2 class="section-title">已授权账号</h2>
        <p class="fine-print account-help">每个 Google 账号独立连接、独立统计用量。额度受限和授权失效分别处理。</p>
        <ul v-if="state?.accounts.length" class="rows">
          <li v-for="account in state.accounts" :key="account.id" class="account">
            <div class="bar">
              <div class="account-identity"><strong>{{ account.email || account.id }}</strong><span class="account-state" :class="account.status">{{ account.message }}</span></div>
              <div class="actions">
                <ZButton :disabled="account.status === 'removed' || loginPending || linking || !!acting" @click="reauthorize(account)">重新授权</ZButton>
                <ZButton :disabled="account.status === 'removed' || loginPending || linking || !!acting" :loading="acting === account.id" @click="accountAction(account, 'connect')">同步连接</ZButton>
                <ZButton :disabled="loginPending || linking || !!acting" @click="confirmDelete = account.id">删除</ZButton>
              </div>
            </div>
            <p class="fine-print account-detail">{{ account.provider === 'antigravity' ? 'Antigravity' : 'Code Assist' }} · {{ account.models.length }} 个模型 · {{ account.gatewayAccountId ? '已建立独立网关连接' : '尚未连接网关' }}</p>
            <p v-if="account.nextRetryAfter" class="fine-print">下次可重试时间：{{ new Date(account.nextRetryAfter).toLocaleString() }}</p>
            <div v-if="confirmDelete === account.id" class="delete-confirm" role="group" :aria-label="`确认删除 ${account.email || account.id}`">
              <p>删除此账号的本机授权和对应网关连接？删除后需要重新添加并授权，Google 账号本身不会删除。</p>
              <div class="actions"><ZButton type="primary" :loading="acting === account.id" @click="accountAction(account, 'delete')">确认删除</ZButton><ZButton :disabled="!!acting" @click="confirmDelete = ''">取消</ZButton></div>
            </div>
          </li>
        </ul>
        <p v-else class="fine-print">暂无账号，完成 Google 授权后刷新</p>
      </ZCard>
      <ZCard padding="none" class="panel">
        <div class="bar"><h2 class="section-title">连接到现有网关</h2><ZButton type="primary" :loading="linking" :disabled="!state?.accounts.length || loading || loginPending || !!acting" @click="connect">同步全部账号并连接</ZButton></div>
        <p class="fine-print">每个 Google 账号创建一条独立上游，共用 Gemini 分组；旧版合并上游会停用并保留历史。</p>
        <div v-if="state?.models.length" class="model-list"><code v-for="model in state.models" :key="model" class="model">{{ model }}</code></div>
        <p v-else class="muted">尚无可用模型，需先完成 Google 授权</p>
      </ZCard>
      <details class="muted"><summary>本机数据与运行说明</summary><p>引擎 {{ state?.engineVersion }} · Google 模块 {{ state?.googleModuleVersion }}</p><p>账号数据：{{ state?.dataDirectory ?? '以插件配置为准' }}</p><p>停用插件会停止 Gemini 引擎并中断未完成的 Gemini 请求，账号数据会保留</p><p>若使用密钥限制了分组，需要把本插件生成的 Gemini 分组加入该密钥的可用范围</p></details>
    </main>
  </ZConfigProvider>
</template>
