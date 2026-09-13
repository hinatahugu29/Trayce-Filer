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
  /// `metadata` は走査中の `DirEntry::metadata()` を渡す。Windows ではディレクトリ列挙の
  /// 結果に含まれているので追加の I/O が無い。以前はここで `fs::metadata` を呼び、
  /// 全ファイルにもう1回ずつディスクアクセスが発生していた。
  fn new(path: PathBuf, is_dir: bool, metadata: Option<std::fs::Metadata>) -> Self {
    let name = path.file_name().map(|value| value.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
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
  let mut state = FilterState::default();
  while let Ok(first) = rx.recv() {
    apply_search_command(first, &mut latest);
    while let Ok(next) = rx.try_recv() {
      apply_search_command(next, &mut latest);
    }
    let Some((request_id, options)) = latest.as_ref() else { continue };
    if let Some(event) = state.run(&entries, &id, *request_id, options) {
      let _ = app.emit(SEARCH_RESULTS, event);
    }
  }
}

/// 絞り込みの途中結果を持ち越す。
///
/// 走査中は数百msごとに再計算が走る。以前は毎回キャッシュ全体をロック下で複製し、
/// 全件を絞り込み、一致した全件を並べ替えていた（検索語が空なら全件の並べ替え）。
/// キャッシュはセッション中は末尾に追加されるだけなので、
/// 条件が同じなら「前回から増えた分」だけを見れば足りる。
#[derive(Default)]
struct FilterState {
  /// キャッシュの手元コピー。ロックは増えた分を写す間だけ取る。
  local: Vec<Arc<CachedEntry>>,
  /// `matched` がどこまでの `local` を評価済みか。
  evaluated: usize,
  matched: Vec<Arc<CachedEntry>>,
  /// 前回の結果を出した要求。これが変わったら一致集合を作り直す。
  request_id: Option<u64>,
}

impl FilterState {
  fn run(&mut self, entries: &Mutex<Vec<Arc<CachedEntry>>>, id: &str, request_id: u64, options: &SearchOptions) -> Option<SearchResultsEvent> {
    {
      let guard = entries.lock().unwrap();
      if guard.len() > self.local.len() {
        self.local.extend_from_slice(&guard[self.local.len()..]);
      }
    }
    let same_request = self.request_id == Some(request_id);
    if same_request && self.evaluated == self.local.len() {
      // 条件もキャッシュも変わっていない。同じ結果を IPC で送り直さない。
      return None;
    }
    if !same_request {
      self.request_id = Some(request_id);
      self.evaluated = 0;
      self.matched.clear();
    }
    let query = parse_query(&options.query);
    self.matched.extend(self.local[self.evaluated..].iter().filter(|entry| matches_query(entry, &query, options.match_path)).cloned());
    self.evaluated = self.local.len();

    let total = self.matched.len();
    let limit = options.limit.unwrap_or(RESULT_LIMIT).min(RESULT_LIMIT);
    // 表示するのは先頭 limit 件だけ。全件を並べ替えず、上位を選んでから並べる。
    // matched の並び自体に意味は無いので、その場で並べ替えてよい。
    let compare = |a: &Arc<CachedEntry>, b: &Arc<CachedEntry>| compare_entries(a, b, options);
    if limit > 0 && total > limit {
      self.matched.select_nth_unstable_by(limit - 1, compare);
    }
    let shown = total.min(limit);
    let mut top = self.matched[..shown].to_vec();
    top.sort_unstable_by(compare);
    Some(SearchResultsEvent {
      id: id.to_string(), request_id, matched: total, indexed: self.local.len(), truncated: total > shown,
      entries: top.into_iter().map(|entry| entry.view.clone()).collect(),
    })
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
    // canonicalize は Windows で `\\?\` を付ける。そのまま走査すると結果の全パスに付き、
    // 通常ペイン・トレイ・お気に入りの同じファイルと別物扱いになる。
    let plain = PathBuf::from(crate::fs_ops::strip_unc(&canonical));
    if seen.insert(plain.to_string_lossy().to_lowercase()) { stack.push(plain); }
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
      batch.push(Arc::new(CachedEntry::new(path, kind.is_dir(), item.metadata().ok())));
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

/// 空白区切りの1語。`|` で並べた候補のどれかを含み、`!`/`-` の語をどれも含まない。
/// 含む語と除く語を解析時に分けておき、全件に対する判定でメモリ確保をしない。
#[derive(Debug, Default, PartialEq, Eq)]
struct QueryClause { include: Vec<String>, exclude: Vec<String> }
type Query = Vec<QueryClause>;
fn parse_query(query: &str) -> Query {
  normalize(query).split_whitespace().filter_map(|part| {
    let mut clause = QueryClause::default();
    for value in part.split('|').map(str::trim) {
      if let Some(term) = value.strip_prefix('!').or_else(|| value.strip_prefix('-')) {
        if !term.is_empty() { clause.exclude.push(term.replace('/', "\\")); }
      } else if !value.is_empty() {
        clause.include.push(value.replace('/', "\\"));
      }
    }
    (!clause.include.is_empty() || !clause.exclude.is_empty()).then_some(clause)
  }).collect()
}
fn matches_query(entry: &CachedEntry, query: &Query, match_path: bool) -> bool {
  let contains = |term: &String| entry.name_lower.contains(term.as_str()) || (match_path && entry.path_lower.contains(term.as_str()));
  query.iter().all(|clause| {
    !clause.exclude.iter().any(contains) && (clause.include.is_empty() || clause.include.iter().any(contains))
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
    let entry = CachedEntry::new(PathBuf::from(r"C:\Assets\Blue Icon.png"), false, None);
    assert!(matches_query(&entry, &parse_query("blue jpg|png !draft"), true));
    assert!(!matches_query(&entry, &parse_query("blue !assets"), true));
    assert!(!matches_query(&entry, &parse_query("assets"), false));
    assert!(!matches_query(&entry, &parse_query("jpg|!icon"), false), "同じ語の中の除外も効く");
  }

  fn entry(name: &str, size: u64) -> Arc<CachedEntry> {
    let mut cached = CachedEntry::new(PathBuf::from(format!(r"C:\root\{name}")), false, None);
    cached.view.size = size;
    Arc::new(cached)
  }

  fn options(query: &str, sort_key: &str, limit: usize) -> SearchOptions {
    SearchOptions { query: query.into(), match_path: false, sort_key: sort_key.into(), descending: false, dirs_first: true, limit: Some(limit) }
  }

  fn names(event: &SearchResultsEvent) -> Vec<&str> {
    event.entries.iter().map(|entry| entry.name.as_str()).collect()
  }

  /// 上位だけを選んでから並べても、全件を並べて切った結果と一致すること。
  #[test]
  fn filter_returns_the_sorted_top_results_and_the_full_match_count() {
    let entries = Mutex::new((0..50).map(|i| entry(&format!("f{i:02}.txt"), (i * 37 % 50) as u64)).collect::<Vec<_>>());
    let mut state = FilterState::default();
    let event = state.run(&entries, "s", 1, &options("", "size", 5)).unwrap();
    assert_eq!(event.matched, 50);
    assert!(event.truncated);
    let sizes: Vec<u64> = event.entries.iter().map(|entry| entry.size).collect();
    assert_eq!(sizes, vec![0, 1, 2, 3, 4]);
  }

  /// 走査で増えた分だけを追加評価し、条件が変わったら作り直す。
  #[test]
  fn filter_state_tracks_new_entries_and_resets_on_a_new_request() {
    let entries = Mutex::new(vec![entry("apple.txt", 1), entry("banana.txt", 2)]);
    let mut state = FilterState::default();
    let first = state.run(&entries, "s", 1, &options("a", "name", 10)).unwrap();
    assert_eq!(names(&first), vec!["apple.txt", "banana.txt"]);

    assert!(state.run(&entries, "s", 1, &options("a", "name", 10)).is_none(), "変化が無ければ送り直さない");

    entries.lock().unwrap().push(entry("avocado.txt", 3));
    entries.lock().unwrap().push(entry("cherry.txt", 4));
    let grown = state.run(&entries, "s", 1, &options("a", "name", 10)).unwrap();
    assert_eq!(names(&grown), vec!["apple.txt", "avocado.txt", "banana.txt"]);
    assert_eq!(grown.indexed, 4);

    let narrowed = state.run(&entries, "s", 2, &options("cherry", "name", 10)).unwrap();
    assert_eq!(names(&narrowed), vec!["cherry.txt"]);
    assert_eq!(narrowed.matched, 1);
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
    assert!(found.iter().all(|entry| !entry.view.path.starts_with(r"\\?\")), "結果パスは通常ペインと同じ表記であるべき");
    assert!(found.iter().any(|entry| entry.view.path == root.join("one.txt").to_string_lossy()));
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
