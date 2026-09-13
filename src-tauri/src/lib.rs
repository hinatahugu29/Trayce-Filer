mod archive;
mod fs_ops;
mod search;
mod store;
mod transfer;
mod undo;
mod watch;
mod windows;

use tauri::Manager;

/// 窓一覧オーバーレイを呼び出すホットキー。
/// Ctrl+Shift+Space は主要アプリと衝突しにくく、片手で押せる。
const OVERLAY_HOTKEY: &str = "CmdOrCtrl+Shift+Space";

/// drag out のプレビュー画像。
///
/// プラグインの `image` 引数は必須で、受け付けるのは
/// `data:image/png;base64,...` 形式か、実在するファイルパスのみ。
/// 空文字は `PathBuf::from("")` という無効パスに化けて失敗するため渡してはいけない。
/// アイコンを埋め込んで data URL にすることで、dev でもバンドル後でも同じ経路になる。
#[tauri::command]
fn drag_preview_icon() -> String {
  use base64::Engine;
  const ICON: &[u8] = include_bytes!("../icons/32x32.png");
  format!(
    "data:image/png;base64,{}",
    base64::engine::general_purpose::STANDARD.encode(ICON)
  )
}

/// D&D の検証結果をターミナル側のログに残す。
/// フロントの画面内ログはウィンドウを閉じると消えてしまい、検証の証跡にならない。
#[tauri::command]
fn log_dnd(message: String) {
  log::info!("[DND] {message}");
}

/// フロント側の例外をターミナルのログへ流す。
///
/// WebView のコンソールは外から見えないため、これが無いと
/// 画面が白いまま原因が分からない、という状態になる。
#[tauri::command]
fn log_ui(level: String, message: String) {
  match level.as_str() {
    "error" => log::error!("[UI] {message}"),
    "warn" => log::warn!("[UI] {message}"),
    _ => log::info!("[UI] {message}"),
  }
}

/// フロントに現在のホットキーを渡す。設定で変えられるので定数ではなく保存値を返す。
#[tauri::command]
fn overlay_hotkey(app: tauri::AppHandle) -> String {
  app.state::<store::Store>().overlay_hotkey()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(windows::Registry::default())
    .manage(store::Store::default())
    .manage(watch::Watchers::default())
    .manage(fs_ops::Clipboard::default())
    .manage(search::Searches::default())
    .manage(transfer::Transfers::default())
    .manage(undo::UndoStack::default())
    .setup(|app| {
      // お気に入りと履歴をディスクから復元する。
      app.state::<store::Store>().attach(app.handle());

      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      // ホットキーは OS 全体に登録するので、他アプリが前面でも効く。
      // 「散らかった窓を探す」用途では、Filer が前面でない時こそ押されるため必須。
      {
        use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

        app.handle().plugin(
          ShortcutBuilder::new()
            .with_handler(|app, _shortcut, event| {
              // press と release の両方で飛んでくるので、押した時だけ反応させる。
              if event.state() == ShortcutState::Pressed {
                windows::toggle_overlay(app);
              }
            })
            .build(),
        )?;

        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        // 設定で変更できる。読めない値が保存されていた場合に備え、失敗したら既定へ戻す。
        let configured = app.state::<store::Store>().overlay_hotkey();
        if let Err(e) = app.global_shortcut().register(configured.as_str()) {
          log::error!("ホットキー {configured} を登録できません: {e}");
          if configured != OVERLAY_HOTKEY {
            let _ = app.global_shortcut().register(OVERLAY_HOTKEY);
          }
        }
      }

      Ok(())
    })
    // 窓が閉じられたらレジストリからも消す。
    // 消し忘れると、オーバーレイに存在しない窓が並び続ける。
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::Destroyed = event {
        let app = window.app_handle();
        let label = window.label().to_string();
        app.state::<windows::Registry>();
        windows::unregister_window(app.clone(), label);
      }
    })
    // ファイルを他アプリへドラッグして出すためのプラグイン。
    // Tauri コアの API には drag out が無いため必須。
    .plugin(tauri_plugin_drag::init())
    // ファイルを既定のアプリで開く / エクスプローラーで場所を表示する。
    .plugin(tauri_plugin_opener::init())
    .invoke_handler(tauri::generate_handler![
      fs_ops::home_dir,
      fs_ops::list_dir,
      fs_ops::list_subdirs,
      fs_ops::drives,
      fs_ops::accept_dropped,
      fs_ops::create_folder,
      fs_ops::rename_entry,
      fs_ops::trash_entries,
      fs_ops::complete_path,
      fs_ops::preview_entry,
      fs_ops::set_clipboard,
      fs_ops::get_clipboard,
      fs_ops::paste_clipboard,
      watch::watch_dir,
      watch::unwatch_dir,
      transfer::start_transfer,
      transfer::cancel_transfer,
      search::start_search,
      search::filter_search,
      search::cancel_search,
      search::pause_search,
      search::resume_search,
      undo::undo_state,
      undo::undo_last,
      windows::open_window,
      windows::list_windows,
      windows::focus_window,
      windows::focus_pane,
      windows::focus_tab,
      windows::set_window_path,
      windows::set_window_context,
      windows::set_window_tray,
      windows::touch_window,
      windows::window_initial_path,
      windows::register_window,
      windows::unregister_window,
      windows::hide_overlay,
      windows::set_overlay_mode,
      windows::close_window,
      store::list_favorites,
      store::toggle_favorite_cmd,
      store::remove_favorite,
      store::reorder_favorite,
      store::list_history,
      store::record_history,
      store::clear_history,
      store::list_search_locations,
      store::record_search_location,
      store::remove_search_location,
      store::clear_search_locations,
      store::paths_exist,
      store::get_settings,
      store::save_settings,
      store::reset_settings,
      store::save_session_state,
      store::get_session_state,
      archive::compress_to_zip,
      archive::extract_zip,
      drag_preview_icon,
      log_dnd,
      log_ui,
      overlay_hotkey
    ])
    .build(tauri::generate_context!())
    .expect("error while building tauri application")
    .run(|app, event| {
      // 保存は間引いて書いているので、終了時に未書き込みの変更を書き切る。
      if let tauri::RunEvent::Exit = event {
        app.state::<store::Store>().flush();
      }
    });
}
