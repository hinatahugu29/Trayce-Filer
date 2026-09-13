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
  /// 条件もキャッシュも変わっていなくても、表示中の結果が実在するか確かめて送り直す。
  Recheck,
}

fn search_worker(app: AppHandle, id: String, entries: Arc<Mutex<Vec<Arc<CachedEntry>>>>, rx: Receiver<SearchCommand>) {
  let mut latest: Option<(u64, SearchOptions)> = None;
  let mut state = FilterState::default();
  while let Ok(first) = rx.recv() {
    let mut force = false;
    apply_search_command(first, &mut latest, &mut force);
    while let Ok(next) = rx.try_recv() {
      apply_search_command(next, &mut latest, &mut force);
    }
    let Some((request_id, options)) = latest.as_ref() else { continue };
    if let Some(event) = state.run(&entries, &id, *request_id, options, force) {
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
struct FilterState {
  /// キャッシュの手元コピー。ロックは増えた分を写す間だけ取る。
  local: Vec<Arc<CachedEntry>>,
  /// `matched` がどこまでの `local` を評価済みか。
  evaluated: usize,
  matched: Vec<Arc<CachedEntry>>,
  /// 前回の結果を出した要求。これが変わったら一致集合を作り直す。
  request_id: Option<u64>,
  /// 読み込み後に消えた（移動・削除された）と分かった項目。以後の結果から除く。
  /// キャッシュ自体は末尾追加だけの前提を保つため、消さずに印だけ付ける。
  gone: HashSet<usize>,
  /// 実在確認。テストでは実ファイルを作らずに差し替える。
  exists: fn(&str) -> bool,
}

impl Default for FilterState {
  fn default() -> Self {
    Self {
      local: Vec::new(),
      evaluated: 0,
      matched: Vec::new(),
      request_id: None,
      gone: HashSet::new(),
      exists: |path| std::fs::symlink_metadata(path).is_ok(),
    }
  }
}

fn entry_key(entry: &Arc<CachedEntry>) -> usize {
  Arc::as_ptr(entry) as usize
}

impl FilterState {
  /// `force` は検索ペインに戻ってきた時など、変化が無くても結果を確かめ直したい場合。
  fn run(&mut self, entries: &Mutex<Vec<Arc<CachedEntry>>>, id: &str, request_id: u64, options: &SearchOptions, force: bool) -> Option<SearchResultsEvent> {
    {
      let guard = entries.lock().unwrap();
      if guard.len() > self.local.len() {
        self.local.extend_from_slice(&guard[self.local.len()..]);
      }
    }
    let same_request = self.request_id == Some(request_id);
    if same_request && self.evaluated == self.local.len() && !force {
      // 条件もキャッシュも変わっていない。同じ結果を IPC で送り直さない。
      return None;
    }
    if !same_request {
      self.request_id = Some(request_id);
      self.evaluated = 0;
      self.matched.clear();
    }
    let query = parse_query(&options.query);
    let gone = &self.gone;
    self.matched.extend(
      self.local[self.evaluated..]
        .iter()
        .filter(|entry| !gone.contains(&entry_key(entry)) && matches_query(entry, &query, options.match_path))
        .cloned(),
    );
    self.evaluated = self.local.len();

    let limit = options.limit.unwrap_or(RESULT_LIMIT).min(RESULT_LIMIT);
    // 表示するのは先頭 limit 件だけ。全件を並べ替えず、上位を選んでから並べる。
    // matched の並び自体に意味は無いので、その場で並べ替えてよい。
    let compare = |a: &Arc<CachedEntry>, b: &Arc<CachedEntry>| compare_entries(a, b, options);
    let (total, shown, mut top) = loop {
      let total = self.matched.len();
      if limit > 0 && total > limit {
        self.matched.select_nth_unstable_by(limit - 1, compare);
      }
      let shown = total.min(limit);
      // 読み込み後に移動・削除されたものを見せると、存在しないパスへ操作できてしまう。
      // 画面に出す分だけ確かめ、消えていたら除いて選び直す（全件は確かめない）。
      let missing: HashSet<usize> = self.matched[..shown].iter().filter(|entry| !(self.exists)(&entry.view.path)).map(entry_key).collect();
      if missing.is_empty() {
        break (total, shown, self.matched[..shown].to_vec());
      }
      self.matched.retain(|entry| !missing.contains(&entry_key(entry)));
      self.gone.extend(missing);
    };
    top.sort_unstable_by(compare);
    Some(SearchResultsEvent {
      id: id.to_string(), request_id, matched: total, indexed: self.local.len(), truncated: total > shown,
      entries: top.into_iter().map(|entry| entry.view.clone()).collect(),
    })
  }
}

fn apply_search_command(command: SearchCommand, latest: &mut Option<(u64, SearchOptions)>, force: &mut bool) {
  match command {
    SearchCommand::Search { request_id, options } => *latest = Some((request_id, options)),
    SearchCommand::Refresh => {}
    SearchCommand::Recheck => *force = true,
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

fn scan_roots(app: AppHandle, id: String, roots: Vec<String>, excludes: Vec<String>, session: Arc<SearchSession>) {
  std::thread::spawn(move || {
    let result = scan(&roots, &excludes, &session.control, |batch, indexed| {
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

/// `excludes` は中へ潜らないフォルダ名。そのフォルダ自体も結果に出さない。
/// 名前の比較は検索と同じ表記ゆれの畳み方で行う（`.GIT` も `.git` と同じ扱い）。
fn scan<F>(roots: &[String], excludes: &[String], control: &ScanControl, mut add: F) -> Result<(usize, usize), String>
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
  let excluded: HashSet<String> = excludes.iter().map(|name| normalize(name.trim())).filter(|name| !name.is_empty()).collect();
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
      if kind.is_dir() && !excluded.is_empty() && excluded.contains(&normalize(&item.file_name().to_string_lossy())) {
        continue;
      }
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

/// 検索用に表記の揺れを畳む。名前・パスと検索語の両方に同じものを掛ける。
///
/// - 英字の大文字小文字
/// - 全角英数記号（`Ａ`, `１`, `－`, `！`, `｜`）と半角
/// - 全角空白と半角空白
/// - 半角カナ（`ｶﾞ`）と全角カナ（`ガ`）
///
/// 全角の `！` `－` `｜` も半角になるので、日本語入力のまま除外や OR が書ける。
/// 文字数が変わる変換（濁点の合成）はここでだけ行い、表示用の文字列は変えない。
pub(crate) fn normalize(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for ch in value.chars() {
    let code = ch as u32;
    let folded = match code {
      0x3000 => ' ',
      0xFF01..=0xFF5E => char::from_u32(code - 0xFEE0).unwrap_or(ch),
      0xFF61..=0xFF9F => {
        // 濁点・半濁点は直前のカナに合成する。
        if code == 0xFF9E || code == 0xFF9F {
          if let Some(prev) = out.pop() {
            match compose_kana_mark(prev, code == 0xFF9F) {
              Some(composed) => { out.push(composed); continue; }
              None => out.push(prev),
            }
          }
        }
        HALFWIDTH_KANA.get((code - 0xFF61) as usize).copied().unwrap_or(ch)
      }
      _ => ch,
    };
    out.extend(folded.to_lowercase());
  }
  out
}

/// U+FF61〜U+FF9F の半角カナに対応する全角文字。
const HALFWIDTH_KANA: [char; 63] = [
  '。', '「', '」', '、', '・', 'ヲ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ャ', 'ュ', 'ョ', 'ッ', 'ー',
  'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ',
  'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ', 'マ', 'ミ',
  'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ', 'ン', '゛', '゜',
];

fn compose_kana_mark(base: char, handakuten: bool) -> Option<char> {
  let code = base as u32;
  if handakuten {
    // ハ行だけが半濁点を取る（ハ=30CF, ヒ=30D2, フ=30D5, ヘ=30D8, ホ=30DB）。
    return matches!(code, 0x30CF | 0x30D2 | 0x30D5 | 0x30D8 | 0x30DB).then(|| char::from_u32(code + 2)).flatten();
  }
  let composable = match code {
    0x30A6 | 0x30EF | 0x30F2 => return Some(match code { 0x30A6 => 'ヴ', 0x30EF => 'ヷ', _ => 'ヺ' }),
    // カ(30AB)〜チ(30C1): 清音と濁音が交互に並ぶ。
    0x30AB..=0x30C1 => (code - 0x30AB) % 2 == 0,
    // ッ(30C3)を挟み、ツ(30C4)〜ト(30C8)も交互。
    0x30C4..=0x30C8 => (code - 0x30C4) % 2 == 0,
    // ハ(30CF)〜ホ(30DB): 清音・濁音・半濁音の3つ組。
    0x30CF..=0x30DB => (code - 0x30CF) % 3 == 0,
    _ => false,
  };
  composable.then(|| char::from_u32(code + 1)).flatten()
}

/// 空白区切りの1語。`|` で並べた候補のどれかを含み、`!`/`-` の語をどれも含まない。
/// 含む語と除く語を解析時に分けておき、全件に対する判定でメモリ確保をしない。
#[derive(Debug, Default, PartialEq, Eq)]
struct QueryClause { include: Vec<String>, exclude: Vec<String> }

/// 名前の一致とは別に、属性で絞る条件。`!`/`-` を前に付けると反転する。
///
/// - `ext:png,jpg`       拡張子（`.` は付けても付けなくてもよい）
/// - `size:>10mb`        大きさ（`>` 以上 / `<` 以下。単位は b, kb, mb, gb。省略時は以上）
/// - `modified:7d`       更新が指定期間以内（h 時間, d 日, w 週。`today` は24時間以内）
/// - `type:file`         ファイルだけ（`dir` / `folder` でフォルダだけ）
///
/// 値が読めない時は条件にせず、普通の検索語として扱う（黙って全件を落とさないため）。
#[derive(Debug, PartialEq, Eq)]
enum FilterKind {
  Ext(Vec<String>),
  SizeAtLeast(u64),
  SizeAtMost(u64),
  ModifiedSince(u128),
  IsDir(bool),
}

#[derive(Debug, PartialEq, Eq)]
struct QueryFilter { kind: FilterKind, negate: bool }

impl QueryFilter {
  fn matches(&self, entry: &CachedEntry) -> bool {
    let view = &entry.view;
    let hit = match &self.kind {
      FilterKind::Ext(exts) => !view.is_dir && exts.iter().any(|ext| *ext == normalize(&view.ext)),
      FilterKind::SizeAtLeast(bytes) => !view.is_dir && view.size >= *bytes,
      FilterKind::SizeAtMost(bytes) => !view.is_dir && view.size <= *bytes,
      FilterKind::ModifiedSince(at) => view.modified >= *at,
      FilterKind::IsDir(is_dir) => view.is_dir == *is_dir,
    };
    hit != self.negate
  }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Query { clauses: Vec<QueryClause>, filters: Vec<QueryFilter> }

fn parse_query(query: &str) -> Query {
  let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
  parse_query_at(query, now)
}

/// `now` は更新日時の条件の基準。テストで固定するために分けている。
fn parse_query_at(query: &str, now: u128) -> Query {
  let mut parsed = Query::default();
  for part in normalize(query).split_whitespace() {
    let (negate, body) = match part.strip_prefix('!').or_else(|| part.strip_prefix('-')) {
      Some(rest) => (true, rest),
      None => (false, part),
    };
    if let Some(kind) = parse_filter(body, now) {
      parsed.filters.push(QueryFilter { kind, negate });
      continue;
    }
    let mut clause = QueryClause::default();
    for value in part.split('|').map(str::trim) {
      if let Some(term) = value.strip_prefix('!').or_else(|| value.strip_prefix('-')) {
        if !term.is_empty() { clause.exclude.push(term.replace('/', "\\")); }
      } else if !value.is_empty() {
        clause.include.push(value.replace('/', "\\"));
      }
    }
    if !clause.include.is_empty() || !clause.exclude.is_empty() {
      parsed.clauses.push(clause);
    }
  }
  parsed
}

fn parse_filter(body: &str, now: u128) -> Option<FilterKind> {
  let (key, value) = body.split_once(':')?;
  if value.is_empty() {
    return None;
  }
  match key {
    "ext" => {
      let exts: Vec<String> = value.split(',').map(|ext| ext.trim().trim_start_matches('.').to_string()).filter(|ext| !ext.is_empty()).collect();
      (!exts.is_empty()).then_some(FilterKind::Ext(exts))
    }
    "size" => {
      let (at_most, amount) = match value.as_bytes()[0] {
        b'<' => (true, &value[1..]),
        b'>' => (false, &value[1..]),
        _ => (false, value),
      };
      let bytes = parse_size(amount.trim_start_matches('='))?;
      Some(if at_most { FilterKind::SizeAtMost(bytes) } else { FilterKind::SizeAtLeast(bytes) })
    }
    "modified" | "date" => {
      let span_ms = if value == "today" { 24 * 3_600_000 } else { parse_span_ms(value)? };
      Some(FilterKind::ModifiedSince(now.saturating_sub(span_ms)))
    }
    "type" | "kind" => match value {
      "file" | "ファイル" => Some(FilterKind::IsDir(false)),
      "dir" | "folder" | "フォルダ" | "フォルダー" => Some(FilterKind::IsDir(true)),
      _ => None,
    },
    _ => None,
  }
}

/// `10mb` `1.5gb` `500` のような大きさ。単位は 1024 倍ずつ。
fn parse_size(text: &str) -> Option<u64> {
  let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(text.len());
  let number: f64 = text[..split].parse().ok()?;
  let unit: u64 = match &text[split..] {
    "" | "b" => 1,
    "k" | "kb" => 1 << 10,
    "m" | "mb" => 1 << 20,
    "g" | "gb" => 1 << 30,
    "t" | "tb" => 1 << 40,
    _ => return None,
  };
  (number >= 0.0).then(|| (number * unit as f64) as u64)
}

/// `24h` `7d` `2w` のような期間をミリ秒へ。
fn parse_span_ms(text: &str) -> Option<u128> {
  let (number, unit) = text.split_at(text.len().checked_sub(1)?);
  let count: u128 = number.parse().ok()?;
  let hour = 3_600_000;
  Some(count * match unit { "h" => hour, "d" => 24 * hour, "w" => 7 * 24 * hour, _ => return None })
}

fn matches_query(entry: &CachedEntry, query: &Query, match_path: bool) -> bool {
  let contains = |term: &String| entry.name_lower.contains(term.as_str()) || (match_path && entry.path_lower.contains(term.as_str()));
  query.filters.iter().all(|filter| filter.matches(entry))
    && query.clauses.iter().all(|clause| {
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
  let excludes = app.state::<crate::store::Store>().search_excludes();
  scan_roots(app, id, roots, excludes, session);
  Ok(())
}

#[tauri::command]
pub fn filter_search(app: AppHandle, id: String, request_id: u64, options: SearchOptions) -> Result<(), String> {
  let session = app.state::<Searches>().sessions.lock().unwrap().get(&id).cloned().ok_or("検索が開始されていません")?;
  session.query_tx.send(SearchCommand::Search { request_id, options }).map_err(|_| "検索処理が終了しています".into())
}

/// 表示中の結果が今も実在するか確かめ直してもらう。検索ペインへ戻ってきた時に呼ぶ。
#[tauri::command]
pub fn recheck_search(app: AppHandle, id: String) {
  if let Some(session) = app.state::<Searches>().sessions.lock().unwrap().get(&id) {
    let _ = session.query_tx.send(SearchCommand::Recheck);
  }
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

  /// TypeScript の foldForSearch と同じ表（tests/fixtures/search-normalize.json）で確かめる。
  /// 片方だけ規則を変えると、検索ペインとフォルダ内絞り込みで一致の結果が食い違う。
  #[test]
  fn normalize_matches_the_shared_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/search-normalize.json")).unwrap();
    let cases = fixture["normalize"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
      let (input, expected) = (case["input"].as_str().unwrap(), case["expected"].as_str().unwrap());
      assert_eq!(normalize(input), expected, "{}", case["note"]);
    }
  }

  /// 全角で打った検索語・除外・OR が、半角の名前にも効くこと。
  #[test]
  fn query_matching_ignores_case_and_width_on_both_sides() {
    let entry = CachedEntry::new(PathBuf::from(r"C:\資料\ｶﾀﾛｸﾞ_Draft2024.PDF"), false, None);
    assert!(matches_query(&entry, &parse_query("カタログ ｐｄｆ"), false));
    assert!(matches_query(&entry, &parse_query("DRAFT２０２４"), false));
    assert!(!matches_query(&entry, &parse_query("カタログ　！draft"), false), "全角の！と全角空白でも除外になる");
    assert!(!matches_query(&entry, &parse_query("カタログ －ｄｒａｆｔ"), false), "全角の－でも除外になる");
    assert!(matches_query(&entry, &parse_query("xlsx｜ＰＤＦ"), false), "全角の｜でも OR になる");
  }

  fn attr_entry(name: &str, is_dir: bool, size: u64, modified: u128) -> CachedEntry {
    let mut cached = CachedEntry::new(PathBuf::from(format!(r"C:\root\{name}")), is_dir, None);
    cached.view.size = size;
    cached.view.modified = modified;
    cached
  }

  const DAY: u128 = 24 * 3_600_000;
  const NOW: u128 = 100 * DAY;

  fn hits(query: &str, entry: &CachedEntry) -> bool {
    matches_query(entry, &parse_query_at(query, NOW), false)
  }

  #[test]
  fn attribute_filters_narrow_by_extension_size_date_and_kind() {
    let photo = attr_entry("Photo.PNG", false, 12 << 20, NOW - 2 * DAY);
    let note = attr_entry("note.txt", false, 300, NOW - 40 * DAY);
    let folder = attr_entry("photos", true, 0, NOW - DAY);

    assert!(hits("ext:png,jpg", &photo) && !hits("ext:png,jpg", &note), "拡張子（大文字小文字は問わない）");
    assert!(hits("ext:.txt", &note), "先頭の . は付けても良い");
    assert!(!hits("ext:png", &folder), "フォルダは拡張子の条件に一致しない");
    assert!(hits("size:>10mb", &photo) && !hits("size:>10mb", &note));
    assert!(hits("size:<1kb", &note) && !hits("size:<1kb", &photo));
    assert!(hits("modified:7d", &photo) && !hits("modified:7d", &note));
    assert!(hits("modified:today", &folder) && !hits("modified:today", &photo));
    assert!(hits("type:dir", &folder) && !hits("type:file", &folder));
    assert!(hits("photo type:file", &photo) && !hits("photo type:file", &folder), "名前の条件と組み合わせる");
    assert!(!hits("!ext:png", &photo) && hits("-ext:png", &note), "反転");
  }

  #[test]
  fn full_width_filters_work_and_unreadable_values_fall_back_to_text() {
    let photo = attr_entry("photo.png", false, 12 << 20, NOW);
    assert!(hits("ＥＸＴ：ＰＮＧ", &photo), "全角で打っても条件として働く");
    assert!(hits("ｓｉｚｅ：＞１０ｍｂ", &photo));

    let parsed = parse_query_at("size:huge c:", NOW);
    assert!(parsed.filters.is_empty(), "読めない値は条件にしない");
    assert_eq!(parsed.clauses.len(), 2, "普通の検索語として残る");
    let odd = attr_entry("size:huge.txt", false, 1, NOW);
    assert!(hits("size:huge", &odd));
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
    let mut state = state_where_all_exist();
    let event = state.run(&entries, "s", 1, &options("", "size", 5), false).unwrap();
    assert_eq!(event.matched, 50);
    assert!(event.truncated);
    let sizes: Vec<u64> = event.entries.iter().map(|entry| entry.size).collect();
    assert_eq!(sizes, vec![0, 1, 2, 3, 4]);
  }

  /// 走査で増えた分だけを追加評価し、条件が変わったら作り直す。
  #[test]
  fn filter_state_tracks_new_entries_and_resets_on_a_new_request() {
    let entries = Mutex::new(vec![entry("apple.txt", 1), entry("banana.txt", 2)]);
    let mut state = state_where_all_exist();
    let first = state.run(&entries, "s", 1, &options("a", "name", 10), false).unwrap();
    assert_eq!(names(&first), vec!["apple.txt", "banana.txt"]);

    assert!(state.run(&entries, "s", 1, &options("a", "name", 10), false).is_none(), "変化が無ければ送り直さない");
    assert!(state.run(&entries, "s", 1, &options("a", "name", 10), true).is_some(), "確かめ直しの依頼には応じる");

    entries.lock().unwrap().push(entry("avocado.txt", 3));
    entries.lock().unwrap().push(entry("cherry.txt", 4));
    let grown = state.run(&entries, "s", 1, &options("a", "name", 10), false).unwrap();
    assert_eq!(names(&grown), vec!["apple.txt", "avocado.txt", "banana.txt"]);
    assert_eq!(grown.indexed, 4);

    let narrowed = state.run(&entries, "s", 2, &options("cherry", "name", 10), false).unwrap();
    assert_eq!(names(&narrowed), vec!["cherry.txt"]);
    assert_eq!(narrowed.matched, 1);
  }

  fn state_where_all_exist() -> FilterState {
    FilterState { exists: |_| true, ..FilterState::default() }
  }

  /// 読み込み後に消えた項目は表示から除き、上限の枠を次の候補で埋める。
  #[test]
  fn results_that_no_longer_exist_are_dropped_and_backfilled() {
    let entries = Mutex::new(vec![entry("a.txt", 1), entry("b-moved.txt", 2), entry("c.txt", 3), entry("d.txt", 4)]);
    let mut state = FilterState { exists: |path| !path.contains("moved"), ..FilterState::default() };

    let event = state.run(&entries, "s", 1, &options("", "name", 2), false).unwrap();
    assert_eq!(names(&event), vec!["a.txt", "c.txt"], "消えた b の枠を c で埋める");
    assert_eq!(event.matched, 3, "件数からも除く");

    let other = state.run(&entries, "s", 2, &options("b", "name", 10), false).unwrap();
    assert!(other.entries.is_empty(), "条件を変えても消えた項目は戻らない");
  }

  #[test]
  fn scan_builds_one_cache_entry_per_path_and_deduplicates_roots() {
    let root = temp_dir("scan");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("one.txt"), b"1").unwrap();
    fs::write(root.join("nested").join("two.txt"), b"2").unwrap();
    let text = root.to_string_lossy().to_string();
    let mut found = Vec::new();
    let result = scan(&[text.clone(), text], &[], &ScanControl::new(), |batch, _| found.extend(batch)).unwrap();
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
    let result = scan(&[root.to_string_lossy().to_string()], &[], &control, |batch, _| found.extend(batch)).unwrap();
    assert_eq!(result.0, 0);
    assert!(found.is_empty());
    fs::remove_dir_all(root).unwrap();
  }

  /// 除外したフォルダは中へ潜らず、フォルダ自体も結果に出さない。名前の大文字小文字は問わない。
  #[test]
  fn scan_skips_excluded_folder_names() {
    let root = temp_dir("excludes");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("Node_Modules").join("pkg")).unwrap();
    fs::write(root.join("src").join("keep.txt"), b"k").unwrap();
    fs::write(root.join("Node_Modules").join("pkg").join("skip.txt"), b"s").unwrap();

    let mut found = Vec::new();
    scan(&[root.to_string_lossy().to_string()], &["node_modules".into()], &ScanControl::new(), |batch, _| found.extend(batch)).unwrap();
    let names: Vec<&str> = found.iter().map(|entry| entry.view.name.as_str()).collect();
    assert!(names.contains(&"keep.txt"));
    assert!(!names.iter().any(|name| name.eq_ignore_ascii_case("node_modules") || *name == "pkg" || *name == "skip.txt"), "{names:?}");
    fs::remove_dir_all(root).unwrap();
  }
}
