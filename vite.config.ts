import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte()],
  // ポートが埋まっていても黙って別ポートに退避させない。
  // 退避すると tauri.conf.json の devUrl と食い違い、白画面になる。
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // src-tauri は見ない。Rust 側の変更は cargo 自身が見張っているので、
      // ここで重ねて見る意味がない。
      //
      // 実害があるのは target/ で、cargo がリンク中の app_lib.dll を掴もうとして
      // Windows では EBUSY で落ちる（"resource busy or locked, watch ...app_lib.dll"）。
      // 起動時にリンクが走っているかどうかで結果が変わるため、再現が安定しない。
      // 数万ファイルを走査しなくなるぶん、起動も速くなる。
      ignored: ['**/src-tauri/**']
    }
  },
  build: {
    outDir: 'dist'
  }
})
