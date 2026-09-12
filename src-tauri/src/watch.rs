use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// 変更が起きたディレクトリを知らせるイベント名。
pub const FS_CHANGED: &str = "fs-changed";

/// 監視中のディレクトリ。
///
/// 同じ場所を複数のペイン・窓が開くので、パスごとに参照数を持ち、
/// 最後の1つが離れた時にだけ監視を止める。
#[derive(Default)]
pub struct Watchers {
  entries: Mutex<HashMap<String, (RecommendedWatcher, usize)>>,
}

/// このディレクトリの変更を見張る。既に見ているなら参照数を増やすだけ。
#[tauri::command]
pub fn watch_dir(app: AppHandle, path: String) -> Result<(), String> {
  let state = app.state::<Watchers>();
  let mut entries = state.entries.lock().unwrap();

  if let Some((_, count)) = entries.get_mut(&path) {
    *count += 1;
    return Ok(());
  }

  let emit_to = app.clone();
  let emit_path = path.clone();
  let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
    // 中身を問わず「ここが変わった」とだけ伝える。
    // どう変わったかはフロントが読み直せば分かるので、種類ごとの差分は追わない。
    if res.is_ok() {
      let _ = emit_to.emit(FS_CHANGED, emit_path.clone());
    }
  })
  .map_err(|e| format!("監視を開始できません: {e}"))?;

  // 直下だけ見る。再帰にすると深い階層で大量のイベントが飛んでくる。
  watcher
    .watch(Path::new(&path), RecursiveMode::NonRecursive)
    .map_err(|e| format!("{path} を監視できません: {e}"))?;

  entries.insert(path, (watcher, 1));
  Ok(())
}

/// 監視をやめる。他にも見ている者がいれば参照数を減らすだけ。
#[tauri::command]
pub fn unwatch_dir(app: AppHandle, path: String) {
  let state = app.state::<Watchers>();
  let mut entries = state.entries.lock().unwrap();

  let Some((_, count)) = entries.get_mut(&path) else { return };
  *count -= 1;
  if *count == 0 {
    entries.remove(&path);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// 参照数の増減だけを検証する。watcher の生成は OS 依存なので、
  /// ここでは数の管理が壊れていないことに絞る。
  #[test]
  fn refcount_keeps_a_path_until_the_last_release() {
    let map: Mutex<HashMap<String, usize>> = Mutex::new(HashMap::new());

    let acquire = |p: &str| {
      let mut m = map.lock().unwrap();
      *m.entry(p.to_string()).or_insert(0) += 1;
    };
    let release = |p: &str| {
      let mut m = map.lock().unwrap();
      if let Some(c) = m.get_mut(p) {
        *c -= 1;
        if *c == 0 {
          m.remove(p);
        }
      }
    };

    acquire("C:\\a");
    acquire("C:\\a");
    release("C:\\a");
    assert!(map.lock().unwrap().contains_key("C:\\a"), "まだ見ている者がいる");
    release("C:\\a");
    assert!(!map.lock().unwrap().contains_key("C:\\a"), "最後で外れる");
  }
}
