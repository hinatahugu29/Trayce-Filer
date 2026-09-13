//! 一括名前変更。
//!
//! 規則から新しい名前を作り、適用前に一覧で確かめられるようにする。
//! 適用は「全部を一時名へ → 目的の名前へ」の二段階で行い、`a`↔`b` の入れ替えや
//! 連鎖（a→b, b→c）でも途中で衝突しないようにする。途中で失敗したら元へ戻す。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RenameRule {
  /// 置き換える文字列。空なら置換しない。
  pub find: String,
  pub replace: String,
  /// true なら大文字小文字を区別して探す。
  pub match_case: bool,
  /// 新しい名前（拡張子を除く部分）の形。`{name}` は置換後の元の名前、`{n}` は連番。空なら `{name}`。
  pub template: String,
  /// 連番の開始値。
  pub start: u64,
  /// 連番の桁数（足りない分は 0 で埋める）。
  pub digits: usize,
  /// `lower` / `upper` で拡張子を除く部分の大文字小文字を揃える。それ以外は変えない。
  pub case: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RenameStatus {
  Ok,
  Unchanged,
  Invalid,
  Conflict,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreview {
  pub path: String,
  pub name: String,
  pub new_name: String,
  pub status: RenameStatus,
  pub message: String,
}

/// 拡張子は変えない。付け替えたつもりが無いのに開けなくなる事故を避けるため。
fn split_ext(name: &str, is_dir: bool) -> (&str, &str) {
  if is_dir {
    return (name, "");
  }
  match name.rfind('.') {
    Some(index) if index > 0 => name.split_at(index),
    _ => (name, ""),
  }
}

fn replace_text(text: &str, find: &str, replace: &str, match_case: bool) -> String {
  if find.is_empty() {
    return text.to_string();
  }
  if match_case {
    return text.replace(find, replace);
  }
  let chars: Vec<char> = text.chars().collect();
  let needle: Vec<char> = find.chars().collect();
  let same = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
  let mut out = String::with_capacity(text.len());
  let mut index = 0;
  while index < chars.len() {
    if index + needle.len() <= chars.len() && needle.iter().enumerate().all(|(k, c)| same(chars[index + k], *c)) {
      out.push_str(replace);
      index += needle.len();
    } else {
      out.push(chars[index]);
      index += 1;
    }
  }
  out
}

fn new_name_for(name: &str, is_dir: bool, index: usize, rule: &RenameRule) -> String {
  let (stem, ext) = split_ext(name, is_dir);
  let replaced = replace_text(stem, &rule.find, &rule.replace, rule.match_case);
  let template = if rule.template.trim().is_empty() { "{name}" } else { rule.template.as_str() };
  let number = format!("{:0width$}", rule.start + index as u64, width = rule.digits.max(1));
  let shaped = template.replace("{name}", &replaced).replace("{n}", &number);
  let shaped = match rule.case.as_str() {
    "lower" => shaped.to_lowercase(),
    "upper" => shaped.to_uppercase(),
    _ => shaped,
  };
  format!("{shaped}{ext}")
}

/// Windows は大文字小文字を区別しないので、同一性はこの形で比べる。
fn key(path: &Path) -> String {
  path.to_string_lossy().to_lowercase()
}

/// 規則を当てた結果の一覧。`paths` の並び順で連番を振る。
pub fn plan(paths: &[String], rule: &RenameRule) -> Vec<RenamePreview> {
  let drafts: Vec<(PathBuf, String, String)> = paths
    .iter()
    .enumerate()
    .map(|(index, path)| {
      let path = PathBuf::from(path);
      let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
      let new_name = new_name_for(&name, path.is_dir(), index, rule);
      (path, name, new_name)
    })
    .collect();

  // 名前が変わるもの。変わるものが元々いた場所は、二段階の適用で空くので衝突に数えない。
  let leaving: HashSet<String> = drafts.iter().filter(|(_, name, new_name)| name != new_name).map(|(path, _, _)| key(path)).collect();
  let mut target_counts: HashMap<String, usize> = HashMap::new();
  for (path, name, new_name) in &drafts {
    if name != new_name {
      *target_counts.entry(key(&path.with_file_name(new_name))).or_default() += 1;
    }
  }

  drafts
    .into_iter()
    .map(|(path, name, new_name)| {
      let target = path.with_file_name(&new_name);
      let (status, message) = if name == new_name {
        (RenameStatus::Unchanged, String::new())
      } else if let Err(error) = crate::fs_ops::validate_name(&new_name) {
        (RenameStatus::Invalid, error)
      } else if target_counts.get(&key(&target)).copied().unwrap_or(0) > 1 {
        (RenameStatus::Conflict, "同じ名前になる項目があります".into())
      } else if key(&target) != key(&path) && target.exists() && !leaving.contains(&key(&target)) {
        (RenameStatus::Conflict, "その名前は既に存在します".into())
      } else {
        (RenameStatus::Ok, String::new())
      };
      RenamePreview { path: path.to_string_lossy().to_string(), name, new_name, status, message }
    })
    .collect()
}

/// (元, 先) の組を二段階で名前変更する。途中で失敗したら、それまでの変更を元に戻す。
pub(crate) fn rename_pairs(pairs: &[(PathBuf, PathBuf)]) -> Result<(), String> {
  let pid = std::process::id();
  let mut staged: Vec<PathBuf> = Vec::with_capacity(pairs.len());
  for (index, (from, _)) in pairs.iter().enumerate() {
    let temporary = from.with_file_name(format!(".trayce-rename-{pid}-{index}.tmp"));
    if let Err(error) = std::fs::rename(from, &temporary) {
      for (back, (original, _)) in staged.iter().zip(pairs) {
        let _ = std::fs::rename(back, original);
      }
      return Err(format!("{} の名前を変えられません: {error}", from.display()));
    }
    staged.push(temporary);
  }
  for (index, (_, to)) in pairs.iter().enumerate() {
    if let Err(error) = std::fs::rename(&staged[index], to) {
      for (done, (_, placed)) in pairs.iter().enumerate().take(index) {
        let _ = std::fs::rename(placed, &staged[done]);
      }
      for (temporary, (original, _)) in staged.iter().zip(pairs) {
        let _ = std::fs::rename(temporary, original);
      }
      return Err(format!("{} へ名前を変えられません: {error}", to.display()));
    }
  }
  Ok(())
}

/// 規則を当てて名前を変える。1件でも不正・衝突があれば何も変えない。戻り値は (元, 先)。
pub fn apply(paths: &[String], rule: &RenameRule) -> Result<Vec<(PathBuf, PathBuf)>, String> {
  let previews = plan(paths, rule);
  if let Some(bad) = previews.iter().find(|p| matches!(p.status, RenameStatus::Invalid | RenameStatus::Conflict)) {
    return Err(format!("{}: {}", bad.name, bad.message));
  }
  let pairs: Vec<(PathBuf, PathBuf)> = previews
    .into_iter()
    .filter(|p| p.status == RenameStatus::Ok)
    .map(|p| {
      let from = PathBuf::from(&p.path);
      let to = from.with_file_name(&p.new_name);
      (from, to)
    })
    .collect();
  rename_pairs(&pairs)?;
  Ok(pairs)
}

#[tauri::command]
pub fn plan_bulk_rename(paths: Vec<String>, rule: RenameRule) -> Vec<RenamePreview> {
  plan(&paths, &rule)
}

/// 戻り値は名前を変えた件数。取り消し履歴に積む。
#[tauri::command]
pub fn apply_bulk_rename(app: AppHandle, paths: Vec<String>, rule: RenameRule) -> Result<usize, String> {
  let pairs = apply(&paths, &rule)?;
  let count = pairs.len();
  if count > 0 {
    app.state::<crate::undo::UndoStack>().push(crate::undo::UndoAction::BatchRename { pairs });
  }
  Ok(count)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("filer_rename_test_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
  }

  fn rule() -> RenameRule {
    RenameRule { start: 1, digits: 1, ..RenameRule::default() }
  }

  #[test]
  fn names_are_built_from_replace_template_numbering_and_case() {
    let r = RenameRule { find: "draft".into(), replace: "final".into(), template: "{n}_{name}".into(), start: 9, digits: 3, ..rule() };
    assert_eq!(new_name_for("Report-DRAFT.docx", false, 0, &r), "009_Report-final.docx", "既定は大文字小文字を区別せず置換");
    assert_eq!(new_name_for("Report-DRAFT.docx", false, 2, &r), "011_Report-final.docx");

    let exact = RenameRule { find: "draft".into(), replace: "x".into(), match_case: true, ..rule() };
    assert_eq!(new_name_for("DRAFT draft.txt", false, 0, &exact), "DRAFT x.txt");

    let upper = RenameRule { case: "upper".into(), ..rule() };
    assert_eq!(new_name_for("photo.jpg", false, 0, &upper), "PHOTO.jpg", "拡張子は変えない");
    assert_eq!(new_name_for("my.folder", true, 0, &upper), "MY.FOLDER", "フォルダは . 以降も名前");
    assert_eq!(new_name_for(".gitignore", false, 0, &upper), ".GITIGNORE", "先頭の . は拡張子ではない");
  }

  #[test]
  fn plan_flags_invalid_duplicate_and_existing_targets() {
    let root = scratch("plan");
    for name in ["a.txt", "b.txt", "taken.txt"] {
      std::fs::write(root.join(name), b"x").unwrap();
    }
    let paths: Vec<String> = ["a.txt", "b.txt"].iter().map(|n| root.join(n).to_string_lossy().to_string()).collect();

    let same = RenameRule { template: "same".into(), ..rule() };
    assert!(plan(&paths, &same).iter().all(|p| p.status == RenameStatus::Conflict), "同じ名前になる");

    let taken = RenameRule { template: "taken".into(), ..rule() };
    assert_eq!(plan(&paths[..1], &taken)[0].status, RenameStatus::Conflict, "既存と衝突");

    let invalid = RenameRule { template: "a:b".into(), ..rule() };
    assert_eq!(plan(&paths[..1], &invalid)[0].status, RenameStatus::Invalid);

    assert_eq!(plan(&paths[..1], &rule())[0].status, RenameStatus::Unchanged);
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 入れ替え（a→b, b→a）は、互いの元の場所が空くので衝突にしない。
  #[test]
  fn swapping_names_works_and_can_be_undone() {
    let root = scratch("swap");
    std::fs::write(root.join("a.txt"), b"A").unwrap();
    std::fs::write(root.join("b.txt"), b"B").unwrap();
    let paths: Vec<String> = ["a.txt", "b.txt"].iter().map(|n| root.join(n).to_string_lossy().to_string()).collect();
    let find_ab = RenameRule { find: "a".into(), replace: "#".into(), ..rule() };
    // a→#, b→b（変わらない）の組み合わせでまず単純な変更を確かめる。
    assert_eq!(apply(&paths[..1], &find_ab).unwrap().len(), 1);
    assert_eq!(std::fs::read(root.join("#.txt")).unwrap(), b"A");
    std::fs::rename(root.join("#.txt"), root.join("a.txt")).unwrap();

    let pairs = vec![(root.join("a.txt"), root.join("b.txt")), (root.join("b.txt"), root.join("a.txt"))];
    rename_pairs(&pairs).unwrap();
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"B");
    assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), b"A");

    let reversed: Vec<(PathBuf, PathBuf)> = pairs.iter().map(|(from, to)| (to.clone(), from.clone())).collect();
    rename_pairs(&reversed).unwrap();
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"A", "取り消しで元に戻る");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn numbering_follows_the_given_order() {
    let root = scratch("numbering");
    for name in ["z.png", "m.png", "a.png"] {
      std::fs::write(root.join(name), b"x").unwrap();
    }
    let paths: Vec<String> = ["z.png", "m.png", "a.png"].iter().map(|n| root.join(n).to_string_lossy().to_string()).collect();
    let numbered = RenameRule { template: "img_{n}".into(), start: 1, digits: 2, ..RenameRule::default() };
    apply(&paths, &numbered).unwrap();
    for (index, original) in ["z", "m", "a"].iter().enumerate() {
      assert!(root.join(format!("img_{:02}.png", index + 1)).exists(), "{original} の番号");
    }
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn a_failure_leaves_nothing_renamed() {
    let root = scratch("rollback");
    std::fs::write(root.join("a.txt"), b"A").unwrap();
    let pairs = vec![
      (root.join("a.txt"), root.join("renamed.txt")),
      (root.join("missing.txt"), root.join("other.txt")),
    ];
    assert!(rename_pairs(&pairs).is_err());
    assert!(root.join("a.txt").exists(), "途中で失敗したら元へ戻す");
    assert!(!root.join("renamed.txt").exists());
    let _ = std::fs::remove_dir_all(&root);
  }
}
