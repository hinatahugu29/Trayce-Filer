use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const TRANSFER_PROGRESS: &str = "transfer-progress";
pub const TRANSFER_DONE: &str = "transfer-done";

/// 進行中の転送。中断のために旗だけ持つ。
#[derive(Default)]
pub struct Transfers {
  running: Mutex<HashMap<u64, Arc<AtomicBool>>>,
  next_id: Mutex<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
  pub id: u64,
  /// 総量を数えている最中かどうか。大きな木ではここで待たされる。
  pub scanning: bool,
  pub files_done: u64,
  pub files_total: u64,
  pub bytes_done: u64,
  pub bytes_total: u64,
  /// 転送速度(B/s)。
  pub bytes_per_sec: u64,
  /// 完了予測残り時間(秒)。
  pub eta_secs: Option<u64>,
  /// いま処理しているファイル。
  pub current: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneEvent {
  pub id: u64,
  pub dest: String,
  pub created: usize,
  pub cancelled: bool,
  pub error: Option<String>,
}

/// コピー / 移動を始める。戻り値は中断に使う ID。
///
/// 本体は別スレッドで走らせる。ここで待つと、大きなコピーの間
/// UI が固まって「落ちた」ように見える。
#[tauri::command]
pub fn start_transfer(
  app: AppHandle,
  paths: Vec<String>,
  dest: String,
  move_files: bool,
) -> u64 {
  let id = {
    let state = app.state::<Transfers>();
    let mut next = state.next_id.lock().unwrap();
    *next += 1;
    *next
  };

  let cancel = Arc::new(AtomicBool::new(false));
  app.state::<Transfers>().running.lock().unwrap().insert(id, cancel.clone());

  std::thread::spawn(move || {
    let emit = |ev: ProgressEvent| {
      let _ = app.emit(TRANSFER_PROGRESS, ev);
    };

    // まず総量を数える。分母が無いと進捗が出せない。
    emit(ProgressEvent {
      id,
      scanning: true,
      files_done: 0,
      files_total: 0,
      bytes_done: 0,
      bytes_total: 0,
      bytes_per_sec: 0,
      eta_secs: None,
      current: String::new(),
    });
    let (files_total, bytes_total) = super::fs_ops::scan_total_pub(&paths);

    let start_time = Instant::now();
    let mut last_time = Instant::now() - Duration::from_secs(1);

    let mut on_progress = |p: &super::fs_ops::Progress, current: &str| {
      let now = Instant::now();
      let elapsed_since_last = now.duration_since(last_time);
      if elapsed_since_last < Duration::from_millis(100) {
        return;
      }
      last_time = now;

      let total_elapsed_secs = start_time.elapsed().as_secs_f64();
      let bytes_per_sec = if total_elapsed_secs > 0.05 {
        (p.bytes_done as f64 / total_elapsed_secs) as u64
      } else {
        0
      };

      let eta_secs = if bytes_per_sec > 0 && bytes_total > p.bytes_done {
        let remaining_bytes = bytes_total - p.bytes_done;
        Some(remaining_bytes / bytes_per_sec)
      } else {
        None
      };

      let _ = app.emit(
        TRANSFER_PROGRESS,
        ProgressEvent {
          id,
          scanning: false,
          files_done: p.files_done,
          files_total,
          bytes_done: p.bytes_done,
          bytes_total,
          bytes_per_sec,
          eta_secs,
          current: current.to_string(),
        },
      );
    };

    let result =
      super::fs_ops::transfer_pub(&paths, &dest, move_files, &cancel, &mut on_progress);

    let cancelled = cancel.load(Ordering::Relaxed);
    let (created, error) = match result {
      Ok(pairs) => {
        let count = pairs.len();
        // 中断で0件だった場合、undo に積む意味が無い。
        if count > 0 {
          app.state::<crate::undo::UndoStack>().push(crate::undo::UndoAction::Transfer {
            pairs,
            was_move: move_files,
          });
        }
        (count, None)
      }
      Err(e) => (0, Some(e)),
    };

    app.state::<Transfers>().running.lock().unwrap().remove(&id);
    let _ = app.emit(TRANSFER_DONE, DoneEvent { id, dest, created, cancelled, error });
  });

  id
}

/// 進行中の転送を中断する。旗を立てるだけで、実際の停止は本体側が見て行う。
#[tauri::command]
pub fn cancel_transfer(app: AppHandle, id: u64) {
  if let Some(flag) = app.state::<Transfers>().running.lock().unwrap().get(&id) {
    flag.store(true, Ordering::Relaxed);
  }
}
