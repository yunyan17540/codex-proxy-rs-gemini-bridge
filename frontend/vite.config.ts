import vue from '@vitejs/plugin-vue'
import CodexProxyUI from '@codex-proxy/ui/vite'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

export default defineConfig({
  base: './',
  // 新宿主按元数据注入前缀，保留 cp 也兼容旧宿主固定的主题变量。
  plugins: [vue(), tailwindcss(), CodexProxyUI({ themePrefix: 'cp' }), {
    name: 'plugin-classic-script',
    transformIndexHtml: { order: 'post', handler: html => html.replace('type="module" crossorigin', 'defer') },
  }],
  build: {
    modulePreload: false, cssCodeSplit: false,
    rolldownOptions: { output: { format: 'iife', codeSplitting: false, entryFileNames: 'app.js', assetFileNames: 'app.[ext]' } },
  },
})
