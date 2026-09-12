use serde::{Deserialize, Serialize};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

pub const SEARCH_PROGRESS: &str = "search-progress";
pub const SEARCH_RESULTS: &str = "search-results";
pub const SEARCH_DONE: &str = "search-done";
const RESULT_LIMIT: usize = 50_000;

#[derive(Default)]
pub struct Searches {
  sessions: Mutex<HashMap<String, Arc<SearchSession>>>,
}

struct SearchSession {
  control: ScanControl,
  entries: Arc<Mutex<Vec<Arc<CachedEntry>>>>,
  query_tx: Sender<SearchCommand>,
}

#[derive(Clone)]
struct ScanControl {
  cancelled: Arc<AtomicBool>,
  paused: Arc<(Mutex<bool>, Condvar)>,
}

impl ScanControl {
  fn new() -> Self {
    Self { cancelled: Arc::new(AtomicBool::new(false)), paused: Arc::new((Mutex::new(false), Condvar::new())) }
  }
  fn wait(&self) {
    let (lock, wake) = &*self.paused;
    let mut paused = lock.lock().unwrap_or_else(|value| value.into_inner());
    while *paused && !self.cancelled.load(Ordering::Relaxed) {
      paused = wake.wait(paused).unwrap_or_else(|value| value.into_inner());
    }
  }
  fn cancel(&self) {
    self.cancelled.store(true, Ordering::Relaxed);
    self.resume();
  }
  fn pause(&self) {
    *self.paused.0.lock().unwrap_or_else(|value| value.into_inner()) = true;
  }
  fn resume(&self) {
    *self.paused.0.lock().unwrap_or_else(|value| value.into_inner()) = false;
    self.paused.1.notify_all();
  }
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

struct CachedEntry {
  view: SearchEntry,
  name_lower: String,
  path_lower: String,
}

impl CachedEntry {
  fn new(path: PathBuf, is_dir: bool) -> Self {
    let name = path.file_name().map(|value| value.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
    let metadata = std::fs::metadata(&path).ok();
    let size = metadata.as_ref().map(|value| value.len()).unwrap_or(0);
    let modified = metadata.and_then(|value| value.modified().ok()).and_then(|value| value.duration_since(UNIX_EPOCH).ok()).map(|value| value.as_millis()).unwrap_or(0);
    let ext = if is_dir { String::new() } else { path.extension().map(|value| value.to_string_lossy().to_lowercase()).unwrap_or_default() };
    let path = path.to_string_lossy().to_string();
    Self { name_lower: normalize(&name), path_lower: normalize(&path).replace('/', "\\"), view: SearchEntry { path, name, is_dir, size, modified, ext } }
  }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgressEvent {
  id: String,
  indexed: usize,
  paused: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultsEvent {
  id: String,
  request_id: u64,
  entries: Vec<SearchEntry>,
  matched: usize,
  indexed: usize,
  truncated: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchDoneEvent {
  id: String,
  indexed: usize,
  cancelled: bool,
  warning_count: usize,
  error: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
  query: String,
  match_path: bool,
  sort_key: String,
  descending: bool,
  dirs_first: bool,
  limit: Option<usize>,
}

enum SearchCommand {
  Search { request_id: u64, options: SearchOptions },
  Refresh,
}

fn search_worker(app: AppHandle, id: String, entries: Arc<Mutex<Vec<Arc<CachedEntry>>>>, rx: Receiver<SearchCommand>) {
  let mut latest: Option<(u64, SearchOptions)> = None;
  while let Ok(first) = rx.recv() {
    apply_search_command(first, &mut latest);
    while let Ok(next) = rx.try_recv() {
      apply_search_command(next, &mut latest);
    }
    let Some((request_id, options)) = latest.clone() else { continue };
    let cached = entries.lock().unwrap().clone();
    let query = parse_query(&options.query);
    let mut matched = cached.iter().filter(|entry| matches_query(entry, &query, options.match_path)).cloned().collect::<Vec<_>>();
    let total = matched.len();
    matched.sort_unstable_by(|a, b| compare_entries(a, b, &options));
    let limit = options.limit.unwrap_or(RESULT_LIMIT).min(RESULT_LIMIT);
    matched.truncate(limit);
    let event = SearchResultsEvent {
      id: id.clone(), request_id, matched: total, indexed: cached.len(), truncated: total > matched.len(),
      entries: matched.into_iter().map(|entry| entry.view.clone()).collect(),
    };
    let _ = app.emit(SEARCH_RESULTS, event);
  }
}

fn apply_search_command(command: SearchCommand, latest: &mut Option<(u64, SearchOptions)>) {
  match command {
    SearchCommand::Search { request_id, options } => *latest = Some((request_id, options)),
    SearchCommand::Refresh => {}
  }
}

fn compare_entries(a: &CachedEntry, b: &CachedEntry, options: &SearchOptions) -> CmpOrdering {
  if options.dirs_first && a.view.is_dir != b.view.is_dir {
    return if a.view.is_dir { CmpOrdering::Less } else { CmpOrdering::Greater };
  }
  let order = match options.sort_key.as_str() {
    "size" => a.view.size.cmp(&b.view.size),
    "modified" => a.view.modified.cmp(&b.view.modified),
    "ext" => a.view.ext.cmp(&b.view.ext),
    "path" => a.path_lower.cmp(&b.path_lower),
    _ => a.name_lower.cmp(&b.name_lower),
  };
  let order = if options.descending { order.reverse() } else { order };
  order.then_with(|| a.path_lower.cmp(&b.path_lower))
}

fn scan_roots(app: AppHandle, id: String, roots: Vec<String>, session: Arc<SearchSession>) {
  std::thread::spawn(move || {
    let result = scan(&roots, &session.control, |batch, indexed| {
      session.entries.lock().unwrap().extend(batch);
      let _ = app.emit(SEARCH_PROGRESS, SearchProgressEvent { id: id.clone(), indexed, paused: false });
      let _ = session.query_tx.send(SearchCommand::Refresh);
    });
    let cancelled = session.control.cancelled.load(Ordering::Relaxed);
    let (indexed, warning_count, error) = match result {
      Ok((indexed, warnings)) => (indexed, warnings, None),
      Err(error) => (session.entries.lock().unwrap().len(), 0, Some(error)),
    };
    let _ = session.query_tx.send(SearchCommand::Refresh);
    let _ = app.emit(SEARCH_DONE, SearchDoneEvent { id, indexed, cancelled, warning_count, error });
  });
}

fn scan<F>(roots: &[String], control: &ScanControl, mut add: F) -> Result<(usize, usize), String>
where F: FnMut(Vec<Arc<CachedEntry>>, usize) {
  if roots.is_empty() { return Err("検索対象のフォルダを指定してください".into()); }
  let mut stack = Vec::new();
  let mut seen = HashSet::new();
  for root in roots {
    let path = Path::new(root);
    if !path.is_dir() { return Err(format!("検索対象を開けません: {root}")); }
    let canonical = path.canonicalize().map_err(|error| format!("検索対象を開けません: {root} ({error})"))?;
    if seen.insert(canonical.to_string_lossy().to_lowercase()) { stack.push(canonical); }
  }
  let mut indexed = 0;
  let mut warnings = 0;
  let mut batch = Vec::with_capacity(1000);
  let mut last_update = Instant::now();
  while let Some(dir) = stack.pop() {
    if control.cancelled.load(Ordering::Relaxed) { break; }
    control.wait();
    let read = match std::fs::read_dir(&dir) { Ok(value) => value, Err(_) => { warnings += 1; continue; } };
    for item in read {
      if control.cancelled.load(Ordering::Relaxed) { break; }
      let Ok(item) = item else { warnings += 1; continue };
      let Ok(kind) = item.file_type() else { warnings += 1; continue };
      let path = item.path();
      if kind.is_dir() && !kind.is_symlink() { stack.push(path.clone()); }
      batch.push(Arc::new(CachedEntry::new(path, kind.is_dir())));
      indexed += 1;
      let interval = if indexed < 20_000 { 150 } else if indexed < 40_000 { 300 } else { 500 };
      if batch.len() >= 1000 && last_update.elapsed() >= Duration::from_millis(interval) {
        add(std::mem::take(&mut batch), indexed);
        last_update = Instant::now();
      }
    }
  }
  if !batch.is_empty() { add(batch, indexed); }
  Ok((indexed, warnings))
}

fn normalize(value: &str) -> String { value.replace('\u{3000}', " ").to_lowercase() }

#[derive(Debug, PartialEq, Eq)]
enum QueryToken { Include(String), Exclude(String) }
type Query = Vec<Vec<QueryToken>>;
fn parse_query(query: &str) -> Query {
  normalize(query).split_whitespace().filter_map(|part| {
    let tokens = part.split('|').filter_map(|value| {
      let value = value.trim();
      if let Some(term) = value.strip_prefix('!').or_else(|| value.strip_prefix('-')) {
        (!term.is_empty()).then(|| QueryToken::Exclude(term.replace('/', "\\")))
      } else { (!value.is_empty()).then(|| QueryToken::Include(value.replace('/', "\\"))) }
    }).collect::<Vec<_>>();
    (!tokens.is_empty()).then_some(tokens)
  }).collect()
}
fn matches_query(entry: &CachedEntry, query: &Query, match_path: bool) -> bool {
  let contains = |term: &str| entry.name_lower.contains(term) || (match_path && entry.path_lower.contains(term));
  query.iter().all(|clause| {
    let includes = clause.iter().filter_map(|token| if let QueryToken::Include(term) = token { Some(term) } else { None }).collect::<Vec<_>>();
    clause.iter().all(|token| !matches!(token, QueryToken::Exclude(term) if contains(term))) && (includes.is_empty() || includes.iter().any(|term| contains(term)))
  })
}

#[tauri::command]
pub fn start_search(app: AppHandle, id: String, roots: Vec<String>) -> Result<(), String> {
  let control = ScanControl::new();
  let entries = Arc::new(Mutex::new(Vec::new()));
  let (query_tx, query_rx) = channel();
  let session = Arc::new(SearchSession { control, entries: Arc::clone(&entries), query_tx });
  let old = app.state::<Searches>().sessions.lock().unwrap().insert(id.clone(), Arc::clone(&session));
  if let Some(old) = old { old.control.cancel(); }
  let worker_app = app.clone();
  let worker_id = id.clone();
  std::thread::spawn(move || search_worker(worker_app, worker_id, entries, query_rx));
  scan_roots(app, id, roots, session);
  Ok(())
}

#[tauri::command]
pub fn filter_search(app: AppHandle, id: String, request_id: u64, options: SearchOptions) -> Result<(), String> {
  let session = app.state::<Searches>().sessions.lock().unwrap().get(&id).cloned().ok_or("検索が開始されていません")?;
  session.query_tx.send(SearchCommand::Search { request_id, options }).map_err(|_| "検索処理が終了しています".into())
}

#[tauri::command]
pub fn cancel_search(app: AppHandle, id: String) {
  if let Some(session) = app.state::<Searches>().sessions.lock().unwrap().remove(&id) { session.control.cancel(); }
}
#[tauri::command]
pub fn pause_search(app: AppHandle, id: String) {
  if let Some(session) = app.state::<Searches>().sessions.lock().unwrap().get(&id) { session.control.pause(); }
}
#[tauri::command]
pub fn resume_search(app: AppHandle, id: String) {
  if let Some(session) = app.state::<Searches>().sessions.lock().unwrap().get(&id) { session.control.resume(); }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use std::sync::atomic::AtomicU64;
  use std::time::SystemTime;

  static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

  fn temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
      "filer_cached_search_{}_{}_{}_{}",
      std::process::id(),
      SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
      NEXT_TEMP.fetch_add(1, Ordering::Relaxed),
      name
    ))
  }

  #[test]
  fn query_supports_and_or_exclusion() {
    let entry = CachedEntry::new(PathBuf::from(r"C:\Assets\Blue Icon.png"), false);
    assert!(matches_query(&entry, &parse_query("blue jpg|png !draft"), true));
    assert!(!matches_query(&entry, &parse_query("blue !assets"), true));
    assert!(!matches_query(&entry, &parse_query("assets"), false));
  }

  #[test]
  fn scan_builds_one_cache_entry_per_path_and_deduplicates_roots() {
    let root = temp_dir("scan");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("one.txt"), b"1").unwrap();
    fs::write(root.join("nested").join("two.txt"), b"2").unwrap();
    let text = root.to_string_lossy().to_string();
    let mut found = Vec::new();
    let result = scan(&[text.clone(), text], &ScanControl::new(), |batch, _| found.extend(batch)).unwrap();
    assert_eq!(result.0, 3);
    assert_eq!(found.len(), 3);
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn cancelled_scan_does_not_touch_the_filesystem() {
    let root = temp_dir("cancel");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("one.txt"), b"1").unwrap();
    let control = ScanControl::new();
    control.cancel();
    let mut found = Vec::new();
    let result = scan(&[root.to_string_lossy().to_string()], &control, |batch, _| found.extend(batch)).unwrap();
    assert_eq!(result.0, 0);
    assert!(found.is_empty());
    fs::remove_dir_all(root).unwrap();
  }
}
