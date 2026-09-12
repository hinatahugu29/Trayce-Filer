use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Emitter, Manager};

pub const SEARCH_BATCH: &str = "search-batch";
pub const SEARCH_DONE: &str = "search-done";
const BATCH_SIZE: usize = 200;
const RESULT_LIMIT: usize = 50_000;

#[derive(Default)]
pub struct Searches {
  running: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntry {
  pub path: String,
  pub name: String,
  pub is_dir: bool,
  pub size: u64,
  pub modified: u128,
  pub ext: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchBatchEvent {
  pub id: String,
  pub entries: Vec<SearchEntry>,
  pub scanned: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchDoneEvent {
  pub id: String,
  pub matched: usize,
  pub scanned: usize,
  pub cancelled: bool,
  pub truncated: bool,
  pub warning_count: usize,
  pub error: Option<String>,
}

#[derive(Default)]
struct ScanOutcome {
  matched: usize,
  scanned: usize,
  truncated: bool,
  warning_count: usize,
}

fn normalize(value: &str) -> String {
  value.replace('\u{3000}', " ").to_lowercase()
}

#[derive(Debug, PartialEq, Eq)]
enum QueryToken {
  Include(String),
  Exclude(String),
}

#[derive(Debug, PartialEq, Eq)]
struct QueryClause(Vec<QueryToken>);

fn parse_query(query: &str) -> Vec<QueryClause> {
  normalize(query)
    .split_whitespace()
    .filter_map(|part| {
      let tokens = part
        .split('|')
        .filter_map(|value| {
          let value = value.trim();
          if value.is_empty() {
            return None;
          }
          if let Some(excluded) = value.strip_prefix('!').or_else(|| value.strip_prefix('-')) {
            (!excluded.is_empty()).then(|| QueryToken::Exclude(excluded.replace('/', "\\")))
          } else {
            Some(QueryToken::Include(value.replace('/', "\\")))
          }
        })
        .collect::<Vec<_>>();
      (!tokens.is_empty()).then_some(QueryClause(tokens))
    })
    .collect()
}

fn matches_query(name: &str, path: &str, query: &str, match_path: bool) -> bool {
  let name = normalize(name);
  let path = if match_path { normalize(path).replace('/', "\\") } else { String::new() };
  let contains = |term: &str| name.contains(term) || (match_path && path.contains(term));

  parse_query(query).iter().all(|clause| {
    let included = clause
      .0
      .iter()
      .filter_map(|token| match token {
        QueryToken::Include(term) => Some(term),
        QueryToken::Exclude(_) => None,
      })
      .collect::<Vec<_>>();
    let exclusions_clear = clause.0.iter().all(|token| match token {
      QueryToken::Exclude(term) => !contains(term),
      QueryToken::Include(_) => true,
    });
    exclusions_clear && (included.is_empty() || included.iter().any(|term| contains(term)))
  })
}

fn search_entry(path: PathBuf, is_dir: bool) -> SearchEntry {
  let name = path
    .file_name()
    .map(|value| value.to_string_lossy().to_string())
    .unwrap_or_else(|| path.to_string_lossy().to_string());
  let metadata = std::fs::metadata(&path).ok();
  let size = metadata.as_ref().map(|value| value.len()).unwrap_or(0);
  let modified = metadata
    .and_then(|value| value.modified().ok())
    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
    .map(|value| value.as_millis())
    .unwrap_or(0);
  let ext = if is_dir {
    String::new()
  } else {
    path
      .extension()
      .map(|value| value.to_string_lossy().to_lowercase())
      .unwrap_or_default()
  };
  SearchEntry { path: path.to_string_lossy().to_string(), name, is_dir, size, modified, ext }
}

fn scan_roots<F>(
  roots: &[String],
  query: &str,
  match_path: bool,
  cancel: &AtomicBool,
  mut emit_batch: F,
) -> Result<ScanOutcome, String>
where
  F: FnMut(Vec<SearchEntry>, usize),
{
  if query.trim().is_empty() {
    return Err("検索語を入力してください".into());
  }
  if roots.is_empty() {
    return Err("検索対象のフォルダを指定してください".into());
  }

  let mut stack = Vec::new();
  let mut seen_roots = HashSet::new();
  for root in roots {
    let path = Path::new(root);
    if !path.is_dir() {
      return Err(format!("検索対象を開けません: {root}"));
    }
    let canonical = path.canonicalize().map_err(|error| format!("検索対象を開けません: {root} ({error})"))?;
    let identity = canonical.to_string_lossy().to_lowercase();
    if seen_roots.insert(identity) {
      stack.push(canonical);
    }
  }

  let mut outcome = ScanOutcome::default();
  let mut batch = Vec::with_capacity(BATCH_SIZE);
  while let Some(dir) = stack.pop() {
    if cancel.load(Ordering::Relaxed) {
      break;
    }
    let read = match std::fs::read_dir(&dir) {
      Ok(read) => read,
      Err(_) => {
        outcome.warning_count += 1;
        continue;
      }
    };

    for item in read {
      if cancel.load(Ordering::Relaxed) {
        break;
      }
      let Ok(item) = item else {
        outcome.warning_count += 1;
        continue;
      };
      let Ok(file_type) = item.file_type() else {
        outcome.warning_count += 1;
        continue;
      };
      let path = item.path();
      let is_dir = file_type.is_dir();
      outcome.scanned += 1;

      if matches_query(&item.file_name().to_string_lossy(), &path.to_string_lossy(), query, match_path)
      {
        batch.push(search_entry(path.clone(), is_dir));
        outcome.matched += 1;
        if outcome.matched >= RESULT_LIMIT {
          outcome.truncated = true;
        }
      }
      if is_dir && !file_type.is_symlink() {
        stack.push(path);
      }
      if batch.len() >= BATCH_SIZE {
        emit_batch(std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE)), outcome.scanned);
      }
      if outcome.truncated {
        break;
      }
    }
    if outcome.truncated {
      break;
    }
  }
  if !batch.is_empty() {
    emit_batch(batch, outcome.scanned);
  }
  Ok(outcome)
}

#[tauri::command]
pub fn start_search(
  app: AppHandle,
  id: String,
  roots: Vec<String>,
  query: String,
  match_path: bool,
) -> Result<(), String> {
  if id.trim().is_empty() {
    return Err("検索IDが空です".into());
  }
  let cancel = Arc::new(AtomicBool::new(false));
  {
    let searches = app.state::<Searches>();
    let mut running = searches.running.lock().unwrap();
    if running.contains_key(&id) {
      return Err("同じ検索がすでに実行中です".into());
    }
    running.insert(id.clone(), Arc::clone(&cancel));
  }

  std::thread::spawn(move || {
    let result = scan_roots(&roots, &query, match_path, &cancel, |entries, scanned| {
      let _ = app.emit(SEARCH_BATCH, SearchBatchEvent { id: id.clone(), entries, scanned });
    });
    let cancelled = cancel.load(Ordering::Relaxed);
    let done = match result {
      Ok(outcome) => SearchDoneEvent {
        id: id.clone(),
        matched: outcome.matched,
        scanned: outcome.scanned,
        cancelled,
        truncated: outcome.truncated,
        warning_count: outcome.warning_count,
        error: None,
      },
      Err(error) => SearchDoneEvent {
        id: id.clone(),
        matched: 0,
        scanned: 0,
        cancelled,
        truncated: false,
        warning_count: 0,
        error: Some(error),
      },
    };
    app.state::<Searches>().running.lock().unwrap().remove(&id);
    let _ = app.emit(SEARCH_DONE, done);
  });
  Ok(())
}

#[tauri::command]
pub fn cancel_search(app: AppHandle, id: String) {
  if let Some(cancel) = app.state::<Searches>().running.lock().unwrap().get(&id) {
    cancel.store(true, Ordering::Relaxed);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use std::sync::atomic::AtomicU64;
  use std::time::SystemTime;

  static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

  fn temp_dir(name: &str) -> PathBuf {
    let unique = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
      "filer_search_{}_{}_{}_{}",
      std::process::id(),
      SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
      unique,
      name
    ))
  }

  #[test]
  fn query_is_case_insensitive_and_uses_and_terms() {
    assert!(matches_query("Blue Icon.PNG", r"C:\Assets\Blue Icon.PNG", "blue png", true));
    assert!(!matches_query("Blue Icon.PNG", r"C:\Assets\Blue Icon.PNG", "blue jpg", true));
    assert!(matches_query("report.txt", r"C:\Project Alpha\report.txt", "alpha", true));
    assert!(!matches_query("report.txt", r"C:\Project Alpha\report.txt", "alpha", false));
  }

  #[test]
  fn query_supports_or_exclusion_and_full_width_spaces() {
    assert!(matches_query("blue icon.png", r"C:\Assets\blue icon.png", "jpg|png　!draft", true));
    assert!(!matches_query("blue draft.png", r"C:\Assets\blue draft.png", "jpg|png !draft", true));
    assert!(!matches_query("blue icon.txt", r"C:\Assets\blue icon.txt", "jpg|png", true));
    assert_eq!(
      parse_query("blue|green -draft"),
      vec![
        QueryClause(vec![QueryToken::Include("blue".into()), QueryToken::Include("green".into())]),
        QueryClause(vec![QueryToken::Exclude("draft".into())]),
      ]
    );
  }

  #[test]
  fn scan_streams_matches_and_skips_non_matches() {
    let root = temp_dir("matches");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("blue-one.txt"), b"one").unwrap();
    fs::write(root.join("nested").join("blue-two.txt"), b"two").unwrap();
    fs::write(root.join("other.txt"), b"other").unwrap();
    let mut found = Vec::new();
    let outcome = scan_roots(
      &[root.to_string_lossy().to_string()],
      "blue txt",
      true,
      &AtomicBool::new(false),
      |batch, _| found.extend(batch.into_iter().map(|entry| entry.name)),
    )
    .unwrap();
    found.sort();
    assert_eq!(found, ["blue-one.txt", "blue-two.txt"]);
    assert_eq!(outcome.matched, 2);
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn cancelled_scan_does_no_work() {
    let root = temp_dir("cancelled");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("match.txt"), b"data").unwrap();
    let cancel = AtomicBool::new(true);
    let outcome = scan_roots(
      &[root.to_string_lossy().to_string()],
      "match",
      true,
      &cancel,
      |_, _| panic!("cancelled scan must not emit"),
    )
    .unwrap();
    assert_eq!(outcome.scanned, 0);
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn duplicate_roots_do_not_duplicate_results() {
    let root = temp_dir("duplicate_roots");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("match.txt"), b"data").unwrap();
    let root_text = root.to_string_lossy().to_string();
    let mut found = Vec::new();
    let outcome = scan_roots(
      &[root_text.clone(), root_text],
      "match",
      true,
      &AtomicBool::new(false),
      |batch, _| found.extend(batch),
    )
    .unwrap();
    assert_eq!(outcome.matched, 1);
    assert_eq!(found.len(), 1);
    fs::remove_dir_all(root).unwrap();
  }
}
