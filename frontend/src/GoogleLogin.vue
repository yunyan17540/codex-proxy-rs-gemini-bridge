<script setup lang="ts">
import { shallowRef, watch } from 'vue'
import { ZButton } from '@codex-proxy/ui/button'
import { ZCard } from '@codex-proxy/ui/card'
import { ZInput } from '@codex-proxy/ui/input'
import { ZTextarea } from '@codex-proxy/ui/textarea'
import { useGoogleLogin } from './useGoogleLogin'

const emit = defineEmits<{ complete: [] }>()
const props = defineProps<{ pendingUrl: string | null; pendingProvider: 'antigravity' | 'gemini-cli' | null; lastError: string | null }>()
const provider = shallowRef<'antigravity' | 'gemini-cli'>('antigravity')
const projectId = shallowRef('')
const callbackUrl = shallowRef('')
const { url, busy, waiting, error, message, start, cancel, callback, open, resume } = useGoogleLogin(() => emit('complete'))
watch(() => props.pendingUrl, value => { if (value) resume(value) }, { immediate: true })
watch(() => props.pendingProvider, value => { if (value) provider.value = value }, { immediate: true })
watch(() => props.lastError, value => { if (value) error.value = value }, { immediate: true })
</script>

<template>
  <ZCard padding="none" class="panel">
    <div class="stack">
      <div><h2 class="section-title">添加 Google 账号</h2><p class="fine-print">个人账号使用 Antigravity 授权，可用模型与额度取决于 Google 实际授权</p></div>
      <div class="actions" aria-label="Google 登录通道">
        <ZButton :type="provider === 'antigravity' ? 'primary' : undefined" :aria-pressed="provider === 'antigravity'" :disabled="waiting || busy" @click="provider = 'antigravity'">个人账号 · Antigravity</ZButton>
        <ZButton :type="provider === 'gemini-cli' ? 'primary' : undefined" :aria-pressed="provider === 'gemini-cli'" :disabled="waiting || busy" @click="provider = 'gemini-cli'">Code Assist Standard / Enterprise</ZButton>
      </div>
      <p v-if="provider === 'antigravity'" class="fine-print">个人 Gemini CLI 登录已停止服务；Antigravity 通道会自动处理关联项目</p>
      <template v-else>
        <div class="field"><label for="project-id">Google Cloud 项目 ID（可选）</label><ZInput id="project-id" v-model="projectId" placeholder="具有 Code Assist 订阅的项目 ID" :disabled="waiting || busy" /></div>
        <p class="fine-print">需要有效的 Standard / Enterprise 服务资格；新建项目不会自动获得订阅。该模块会尝试为关联项目启用实验性预览渠道</p>
      </template>
      <div class="actions">
        <ZButton type="primary" :loading="busy" :disabled="waiting" @click="start(provider, provider === 'gemini-cli' ? projectId : '')">生成 Google 授权链接</ZButton>
        <ZButton :disabled="busy" @click="cancel">取消等待中的授权</ZButton>
      </div>
      <template v-if="url">
        <div><ZButton :disabled="busy" @click="open">在默认浏览器打开 Google 官方授权页</ZButton></div>
        <details><summary class="muted">复制授权链接到其他浏览器</summary><ZTextarea :model-value="url" readonly :rows="4" aria-label="Google 授权链接" /></details>
        <p class="fine-print" role="status">等待你完成登录，页面会自动检查结果</p>
        <details>
          <summary class="muted">浏览器回调未自动完成</summary>
          <div class="stack" style="margin-top: 12px">
            <ZTextarea v-model="callbackUrl" :rows="3" placeholder="粘贴浏览器地址栏里完整的 localhost 或 127.0.0.1 回调地址" aria-label="Google 回调地址" />
            <div><ZButton :disabled="!callbackUrl || busy" @click="callback(callbackUrl); callbackUrl = ''">提交回调</ZButton></div>
          </div>
        </details>
      </template>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <p v-if="message" class="notice" role="status">{{ message }}</p>
    </div>
  </ZCard>
</template>
