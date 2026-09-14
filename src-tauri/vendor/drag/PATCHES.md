# Trayce で加えた変更

元: crates.io の `drag` 2.1.1（CrabNebula の `tauri-plugin-drag` が使う部品）。
`src-tauri/Cargo.toml` の `[patch.crates-io]` で、このフォルダの版を使っている。

## 直した問題

ネットワークドライブ（例: `T:\品質管理\...`）にあるファイルをドラッグすると、アプリが落ちていた。

1. 元のコードは `dunce::canonicalize` でパスを変換していた。これはネットワークドライブを
   `\\?\UNC\svtkfl1\TK\...` の形に展開する。
2. Windows の `ILCreateFromPathW` はこの形を受け付けず、NULL を返す。
3. 元のコードはその失敗を確かめずに `unwrap()` していたため、パニックしてプロセスが終了した。

## 変更点（`src/platform_impl/windows/mod.rs`）

- `dunce::canonicalize` を `plain_path` に置き換えた。`std::path::absolute` でドライブ文字のまま
  絶対パスにし、`\\?\` や `\\?\UNC\` が付いていれば通常の形に戻す。存在しないパスはエラー。
- `get_file_data_object(...).unwrap()` と `get_shell_item_array(...).unwrap()` をやめ、
  失敗したらエラーを返すようにした（アプリは落ちず、ドラッグだけが失敗する）。
- `ILCreateFromPathW` が NULL を返したパスがあれば、配列を作らずに失敗として扱う。

## 元に戻すとき

上流で同じ問題が直ったら、`Cargo.toml` の `[patch.crates-io]` とこのフォルダを削除する。
