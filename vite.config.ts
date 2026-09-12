import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte()],
  // ポートが埋まっていても黙って別ポートに退避させない。
  // 退避すると tauri.conf.json の devUrl と食い違い、白画面になる。
  server: {
    port: 5173,
    strictPort: true
  },
  build: {
    outDir: 'dist'
  }
})
