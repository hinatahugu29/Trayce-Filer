use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// オーバーレイ（ホットキーで出す窓一覧）専用のラベル。
/// このラベルの窓だけはファイラではなく一覧UIを描く。
pub const OVERLAY_LABEL: &str = "overlay";
pub const WINDOW_TRAY_CHANGED: &str = "window-tray-changed";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowTrayChanged {
  label: String,
  paths: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct WindowInfo {
  pub label: String,
  /// その窓が今開いているディレクトリの絶対パス。
  pub path: String,
  /// 一覧の並び順に使う。最後に前面へ来た時刻(ms)。
  pub last_focused: u128,
  /// その窓で現在アクティブなタブの収集トレイ。
  pub tray_paths: Vec<String>,
}

#[derive(Default)]
pub struct Registry {
  windows: Mutex<HashMap<String, WindowInfo>>,
  /// 窓ラベルの連番。閉じても再利用しないので、ラベルの衝突が起きない。
  next_id: Mutex<u32>,
}

fn now_ms() -> u128 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_millis())
    .unwrap_or(0)
}

impl Registry {
  fn record(&self, label: &str, path: &str) {
    let mut w = self.windows.lock().unwrap();
    w.insert(
      label.to_string(),
      WindowInfo {
        label: label.to_string(),
        path: path.to_string(),
        last_focused: now_ms(),
        tray_paths: Vec::new(),
      },
    );
  }

  fn forget(&self, label: &str) {
    self.windows.lock().unwrap().remove(label);
  }

  /// 最後に前面へ来た順（新しい順）。
  /// 「さっき見てたやつ」を上に出すのが、散らかった窓を探す時の一番の手掛かりになる。
  fn sorted(&self) -> Vec<WindowInfo> {
    let mut list: Vec<WindowInfo> = self.windows.lock().unwrap().values().cloned().collect();
    list.sort_by(|a, b| b.last_focused.cmp(&a.last_focused));
    list
  }
}

/// 新しいファイラ窓を開く。`path` はその窓の初期ディレクトリ。
#[tauri::command]
pub async fn open_window(app: AppHandle, path: String) -> Result<String, String> {
  let label = {
    let reg = app.state::<Registry>();
    let mut id = reg.next_id.lock().unwrap();
    *id += 1;
    format!("filer-{}", *id)
  };

  // 既存窓と少しずらして出す。完全に重ねると「増えたことに気付けない」。
  let offset = (label.len() as f64 % 5.0) * 28.0;

  WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
    .title(&path)
    .inner_size(1000.0, 700.0)
    .position(120.0 + offset, 120.0 + offset)
    .build()
    .map_err(|e| format!("窓を作れません: {e}"))?;

  app.state::<Registry>().record(&label, &path);
  Ok(label)
}

/// 全窓の一覧。オーバーレイが「今どこが開いているか」を描くために使う。
#[tauri::command]
pub fn list_windows(app: AppHandle) -> Vec<WindowInfo> {
  app.state::<Registry>().sorted()
}

/// 指定した窓を前面に出す。オーバーレイから選択された時に呼ばれる。
#[tauri::command]
pub fn focus_window(app: AppHandle, label: String) -> Result<(), String> {
  let win = app
    .get_webview_window(&label)
    .ok_or_else(|| format!("窓 {label} が見つかりません"))?;

  // 最小化されていると set_focus だけでは出てこない。
  if win.is_minimized().unwrap_or(false) {
    let _ = win.unminimize();
  }
  win.set_focus().map_err(|e| e.to_string())?;

  // 選んだ窓を一覧の先頭へ持ってくる。
  if let Some(info) = app.state::<Registry>().windows.lock().unwrap().get_mut(&label) {
    info.last_focused = now_ms();
  }
  Ok(())
}

/// 窓がディレクトリを移動したことを登録する。
/// タイトルバーにもパスを出しておくと、タスクバーのプレビューからも判別できる。
#[tauri::command]
pub fn set_window_path(app: AppHandle, label: String, path: String) {
  if let Some(info) = app.state::<Registry>().windows.lock().unwrap().get_mut(&label) {
    info.path = path.clone();
  }
  if let Some(win) = app.get_webview_window(&label) {
    let _ = win.set_title(&path);
  }
}

/// 別WebViewである俯瞰オーバーレイから読めるよう、現在タブのトレイを窓レジストリへ同期する。
#[tauri::command]
pub fn set_window_tray(app: AppHandle, label: String, paths: Vec<String>) {
  if let Some(info) = app.state::<Registry>().windows.lock().unwrap().get_mut(&label) {
    info.tray_paths = paths.clone();
  }
  let _ = app.emit(WINDOW_TRAY_CHANGED, WindowTrayChanged { label, paths });
}

/// 窓が前面に来たことを記録する。一覧の並び順に効く。
#[tauri::command]
pub fn touch_window(app: AppHandle, label: String) {
  if let Some(info) = app.state::<Registry>().windows.lock().unwrap().get_mut(&label) {
    info.last_focused = now_ms();
  }
}

/// その窓が開くべきパスを返す。
///
/// `open_window` で指定したパスはレジストリにしか無いので、
/// 新しい窓のフロントはこれを読んで初期ディレクトリを決める。
/// これを使わないと、どの窓を開いても常にホームが出る。
#[tauri::command]
pub fn window_initial_path(app: AppHandle, label: String) -> Option<String> {
  app
    .state::<Registry>()
    .windows
    .lock()
    .unwrap()
    .get(&label)
    .map(|info| info.path.clone())
}

/// 起動直後の最初の窓をレジストリに載せる。
/// main 窓は Rust 側の open_window を通らないので、フロントから自己申告してもらう。
#[tauri::command]
pub fn register_window(app: AppHandle, label: String, path: String) {
  app.state::<Registry>().record(&label, &path);
}

#[tauri::command]
pub fn unregister_window(app: AppHandle, label: String) {
  app.state::<Registry>().forget(&label);
}

/// オーバーレイの表示/非表示を切り替える。ホットキーから呼ばれる。
/// 初回だけ窓を作り、以降は show/hide で出し入れする（毎回作ると遅い）。
pub fn toggle_overlay(app: &AppHandle) {
  if let Some(win) = app.get_webview_window(OVERLAY_LABEL) {
    if win.is_visible().unwrap_or(false) {
      let _ = win.hide();
    } else {
      let _ = win.show();
      let _ = win.set_focus();
    }
    return;
  }

  let builder = WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("index.html".into()))
    .title("Filer — 開いている窓")
    .inner_size(380.0, 620.0)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true) // 角丸の外側を抜くため
    .resizable(true);

  // 画面の右端に寄せる。中央に出すと作業中の窓を隠してしまう。
  let builder = match app.primary_monitor() {
    Ok(Some(monitor)) => {
      let size = monitor.size();
      let scale = monitor.scale_factor();
      let logical_w = size.width as f64 / scale;
      let logical_h = size.height as f64 / scale;
      builder.position(logical_w - 380.0 - 24.0, (logical_h - 620.0) / 2.0)
    }
    _ => builder.center(),
  };

  if let Err(e) = builder.build() {
    log::error!("オーバーレイを作れません: {e}");
  }
}

/// オーバーレイ自身が「閉じる」時に使う。次回はまた show で復帰する。
#[tauri::command]
pub fn hide_overlay(app: AppHandle) {
  if let Some(win) = app.get_webview_window(OVERLAY_LABEL) {
    let _ = win.hide();
  }
}

/// オーバーレイの表示モード（コンパクトリスト ⇄ 大画面俯瞰ワークベンチ）を切り替える。
#[tauri::command]
pub fn set_overlay_mode(app: AppHandle, mode: String) -> Result<(), String> {
  if let Some(win) = app.get_webview_window(OVERLAY_LABEL) {
    let monitor = app.primary_monitor().ok().flatten();
    if mode == "workbench" {
      if let Some(m) = monitor {
        let size = m.size();
        let scale = m.scale_factor();
        let logical_w = size.width as f64 / scale;
        let logical_h = size.height as f64 / scale;
        let target_w = (logical_w * 0.88).clamp(960.0, 1600.0);
        let target_h = (logical_h * 0.84).clamp(650.0, 1000.0);
        let target_x = (logical_w - target_w) / 2.0;
        let target_y = (logical_h - target_h) / 2.0;
        let _ = win.set_size(tauri::LogicalSize::new(target_w, target_h));
        let _ = win.set_position(tauri::LogicalPosition::new(target_x, target_y));
      } else {
        let _ = win.set_size(tauri::LogicalSize::new(1100.0, 750.0));
        let _ = win.center();
      }
    } else {
      if let Some(m) = monitor {
        let size = m.size();
        let scale = m.scale_factor();
        let logical_w = size.width as f64 / scale;
        let logical_h = size.height as f64 / scale;
        let _ = win.set_size(tauri::LogicalSize::new(380.0, 620.0));
        let _ = win.set_position(tauri::LogicalPosition::new(
          logical_w - 380.0 - 24.0,
          (logical_h - 620.0) / 2.0,
        ));
      } else {
        let _ = win.set_size(tauri::LogicalSize::new(380.0, 620.0));
      }
    }
  }
  Ok(())
}

/// 指定したラベルのウィンドウを閉じる。
#[tauri::command]
pub fn close_window(app: AppHandle, label: String) -> Result<(), String> {
  if let Some(win) = app.get_webview_window(&label) {
    win.close().map_err(|e| e.to_string())?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sorts_most_recently_focused_first() {
    let reg = Registry::default();
    reg.record("filer-1", r"C:\a");
    reg.record("filer-2", r"C:\b");
    // record は now_ms() を使うので、順序を確定させるため明示的に上書きする。
    {
      let mut w = reg.windows.lock().unwrap();
      w.get_mut("filer-1").unwrap().last_focused = 100;
      w.get_mut("filer-2").unwrap().last_focused = 200;
    }

    let sorted = reg.sorted();
    assert_eq!(sorted[0].label, "filer-2");
    assert_eq!(sorted[1].label, "filer-1");
  }

  #[test]
  fn forget_removes_the_window() {
    let reg = Registry::default();
    reg.record("filer-1", r"C:\a");
    reg.forget("filer-1");
    assert!(reg.sorted().is_empty());
  }

  #[test]
  fn record_overwrites_path_for_same_label() {
    let reg = Registry::default();
    reg.record("filer-1", r"C:\a");
    reg.record("filer-1", r"C:\b");
    let sorted = reg.sorted();
    assert_eq!(sorted.len(), 1);
    assert_eq!(sorted[0].path, r"C:\b");
  }
}
