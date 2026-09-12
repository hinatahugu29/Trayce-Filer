use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// 取り消せる操作の履歴。上限を超えたら古いものから捨てる。
///
/// **フロントには内容を渡さない。** `trash::TrashItem` が `OsString` を持ち
/// シリアライズできないため、履歴は Rust 側だけで完結させ、
/// フロントには「取り消せるか」「何を取り消すか」のラベルだけ返す。
const HISTORY_CAP: usize = 50;

pub enum UndoAction {
  /// `to` を `from` の名前へ戻す。
  Rename { from: PathBuf, to: PathBuf },
  /// 空フォルダを削除して取り消す。中身が増えていたら安全のため諦める。
  CreateFolder { path: PathBuf },
  /// ゴミ箱から元の場所へ復元する。
  Trash { items: Vec<trash::TrashItem> },
  /// コピー/移動で作られたものを取り消す。
  Transfer {
    /// (元のパス, 作られたパス) の対応。移動なら created を from へ戻す。
    pairs: Vec<(PathBuf, PathBuf)>,
    /// true なら移動だった（元を消していたので戻す）。false ならコピー（作られた方を消すだけ）。
    was_move: bool,
  },
}

impl UndoAction {
  /// UI に出す短い説明。
  fn label(&self) -> String {
    match self {
      UndoAction::Rename { to, .. } => format!("名前の変更「{}」", name_of(to)),
      UndoAction::CreateFolder { path } => format!("フォルダー作成「{}」", name_of(path)),
      UndoAction::Trash { items } => {
        if items.len() == 1 {
          format!("ゴミ箱へ移動「{}」", items[0].name.to_string_lossy())
        } else {
          format!("ゴミ箱へ移動（{}件）", items.len())
        }
      }
      UndoAction::Transfer { pairs, was_move } => {
        let verb = if *was_move { "移動" } else { "コピー" };
        if pairs.len() == 1 {
          format!("{verb}「{}」", name_of(&pairs[0].1))
        } else {
          format!("{verb}（{}件）", pairs.len())
        }
      }
    }
  }
}

fn name_of(p: &std::path::Path) -> String {
  p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

#[derive(Default)]
pub struct UndoStack {
  entries: Mutex<Vec<UndoAction>>,
}

impl UndoStack {
  pub fn push(&self, action: UndoAction) {
    let mut entries = self.entries.lock().unwrap();
    entries.push(action);
    // 先頭から溢れた分を捨てる。古い履歴を無限に持ち続けない。
    let overflow = entries.len().saturating_sub(HISTORY_CAP);
    if overflow > 0 {
      entries.drain(0..overflow);
    }
  }
}

#[derive(Serialize)]
pub struct UndoState {
  pub available: bool,
  pub label: String,
}

/// いま取り消せる操作があるか、あれば何をするか。ボタンの有効/無効やツールチップに使う。
#[tauri::command]
pub fn undo_state(app: AppHandle) -> UndoState {
  let stack = app.state::<UndoStack>();
  let entries = stack.entries.lock().unwrap();
  match entries.last() {
    Some(a) => UndoState { available: true, label: a.label() },
    None => UndoState { available: false, label: String::new() },
  }
}

/// 直前の操作を取り消す。取り消した内容の説明を返す。
#[tauri::command]
pub fn undo_last(app: AppHandle) -> Result<String, String> {
  let action = {
    let stack = app.state::<UndoStack>();
    let mut entries = stack.entries.lock().unwrap();
    entries.pop()
  };

  let Some(action) = action else {
    return Err("取り消せる操作がありません".into());
  };

  let label = action.label();
  apply_undo(action)?;
  Ok(label)
}

fn apply_undo(action: UndoAction) -> Result<(), String> {
  match action {
    UndoAction::Rename { from, to } => {
      if !to.exists() {
        return Err(format!("{} が見つかりません（既に変更・削除された可能性があります）", name_of(&to)));
      }
      std::fs::rename(&to, &from).map_err(|e| format!("元に戻せません: {e}"))?;
    }

    UndoAction::CreateFolder { path } => {
      // 中身が増えていたら、作成の取り消しのつもりで中身ごと消すのは危険。
      // ユーザーが既に何か入れた可能性があるので安全側に倒して諦める。
      if let Ok(mut read) = std::fs::read_dir(&path) {
        if read.next().is_some() {
          return Err(format!(
            "{} には中身が追加されているため取り消せません",
            name_of(&path)
          ));
        }
        std::fs::remove_dir(&path).map_err(|e| format!("削除できません: {e}"))?;
      }
      // 既に無ければ何もしなくてよい。
    }

    UndoAction::Trash { items } => {
      trash::os_limited::restore_all(items).map_err(|e| format!("復元できません: {e}"))?;
    }

    UndoAction::Transfer { pairs, was_move } => {
      let mut failed = Vec::new();
      for (from, to) in pairs {
        if !to.exists() {
          failed.push(name_of(&to));
          continue;
        }
        if was_move {
          // 移動を戻す: 作られた方を元の場所へ。
          if std::fs::rename(&to, &from).is_err() {
            // ボリューム跨ぎなら rename が効かないので copy+delete で戻す。
            let copy_ok = if to.is_dir() {
              copy_dir_back(&to, &from).is_ok()
            } else {
              std::fs::copy(&to, &from).is_ok()
            };
            if copy_ok {
              let _ = if to.is_dir() {
                std::fs::remove_dir_all(&to)
              } else {
                std::fs::remove_file(&to)
              };
            } else {
              failed.push(name_of(&to));
            }
          }
        } else {
          // コピーを戻す: 作られた方を消すだけ。元は最初から触っていない。
          let removed = if to.is_dir() {
            std::fs::remove_dir_all(&to)
          } else {
            std::fs::remove_file(&to)
          };
          if removed.is_err() {
            failed.push(name_of(&to));
          }
        }
      }
      if !failed.is_empty() {
        return Err(format!("一部を元に戻せませんでした: {}", failed.join(", ")));
      }
    }
  }
  Ok(())
}

fn copy_dir_back(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
  std::fs::create_dir_all(dst)?;
  for entry in std::fs::read_dir(src)? {
    let entry = entry?;
    let target = dst.join(entry.file_name());
    if entry.file_type()?.is_dir() {
      copy_dir_back(&entry.path(), &target)?;
    } else {
      std::fs::copy(entry.path(), target)?;
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("filer_undo_test_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
  }

  #[test]
  fn undo_rename_restores_the_old_name() {
    let root = scratch("rename");
    let before = root.join("before.txt");
    let after = root.join("after.txt");
    std::fs::write(&before, b"x").unwrap();
    std::fs::rename(&before, &after).unwrap();

    apply_undo(UndoAction::Rename { from: before.clone(), to: after }).unwrap();

    assert!(before.exists(), "元の名前に戻っているべき");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn undo_rename_fails_gracefully_if_target_is_gone() {
    let root = scratch("rename_gone");
    let missing = root.join("gone.txt");
    let result = apply_undo(UndoAction::Rename { from: root.join("orig.txt"), to: missing });
    assert!(result.is_err(), "対象が無ければエラーで、パニックしない");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn undo_create_folder_removes_empty_folder() {
    let root = scratch("mkdir");
    let created = root.join("new");
    std::fs::create_dir(&created).unwrap();

    apply_undo(UndoAction::CreateFolder { path: created.clone() }).unwrap();

    assert!(!created.exists());
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 作成後にユーザーが中へ何か入れていたら、安全側に倒して諦める。
  #[test]
  fn undo_create_folder_refuses_if_not_empty() {
    let root = scratch("mkdir_notempty");
    let created = root.join("new");
    std::fs::create_dir(&created).unwrap();
    std::fs::write(created.join("important.txt"), b"do not lose me").unwrap();

    let result = apply_undo(UndoAction::CreateFolder { path: created.clone() });

    assert!(result.is_err());
    assert!(created.join("important.txt").exists(), "中身は無事であるべき");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn undo_transfer_copy_removes_the_created_file() {
    let root = scratch("undo_copy");
    let src = root.join("src.txt");
    let dst = root.join("dst.txt");
    std::fs::write(&src, b"x").unwrap();
    std::fs::copy(&src, &dst).unwrap();

    apply_undo(UndoAction::Transfer { pairs: vec![(src.clone(), dst.clone())], was_move: false })
      .unwrap();

    assert!(!dst.exists(), "コピー先は消えるべき");
    assert!(src.exists(), "コピー元は最初から触っていないので残る");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn undo_transfer_move_restores_the_source() {
    let root = scratch("undo_move");
    let src = root.join("src.txt");
    let dst = root.join("dst.txt");
    std::fs::write(&src, b"x").unwrap();
    std::fs::rename(&src, &dst).unwrap();

    apply_undo(UndoAction::Transfer { pairs: vec![(src.clone(), dst.clone())], was_move: true })
      .unwrap();

    assert!(src.exists(), "移動元に戻るべき");
    assert!(!dst.exists());
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn stack_drops_oldest_entries_beyond_the_cap() {
    let stack = UndoStack::default();
    for i in 0..(HISTORY_CAP + 5) {
      stack.push(UndoAction::CreateFolder { path: PathBuf::from(format!("C:\\{i}")) });
    }
    let entries = stack.entries.lock().unwrap();
    assert_eq!(entries.len(), HISTORY_CAP);
  }
}
