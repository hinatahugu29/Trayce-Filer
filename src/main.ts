import './app.css'
import { invoke } from '@tauri-apps/api/core'
import App from './App.svelte'

/**
 * フロント側の例外を Rust のログへ流す。
 *
 * WebView のコンソールは外から見えないので、これが無いと
 * 「画面が白いまま原因が分からない」という状態になる。
 */
function report(level: 'error' | 'warn', message: string) {
  invoke('log_ui', { level, message }).catch(() => {})
}

/** スタックは1行に潰す。複数行だとログの他の行と混ざって読みにくい。 */
function describe(e: unknown): string {
  if (e instanceof Error) {
    return `${e.name}: ${e.message} | ${(e.stack ?? '').split('\n').slice(1, 6).join(' <- ').trim()}`
  }
  return String(e)
}

window.addEventListener('error', (ev) => {
  report('error', `${describe(ev.error ?? ev.message)} @ ${ev.filename}:${ev.lineno}`)
})

window.addEventListener('unhandledrejection', (ev) => {
  report('error', `unhandled rejection: ${describe(ev.reason)}`)
})

const app = new App({
  target: document.getElementById('app')!,
})

export default app
