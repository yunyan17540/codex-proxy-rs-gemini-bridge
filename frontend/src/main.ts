import { createApp } from 'vue'
import App from './App.vue'
import '@codex-proxy/ui/styles.css'
import './utilities.css'
import './style.css'

const updateTheme = () => { document.documentElement.dataset.theme = window.codexProxyPlugin?.theme ?? 'light' }
updateTheme()
window.addEventListener('codex-proxy-themechange', updateTheme)
createApp(App).mount('#app')
