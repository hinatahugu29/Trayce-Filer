use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// 履歴の保持件数。多すぎると探すのに探す羽目になるので、この辺で頭打ちにする。
const HISTORY_CAP: usize = 200;
const SEARCH_HISTORY_CAP: usize = 15;

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
  pub path: String,
  /// 最後に訪れた時刻(ms)。
  pub at: u128,
}

/// A searched set of roots. Kept separately from directory navigation history because
/// revisiting a search target is a different workflow from reopening a folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchLocationEntry {
  pub paths: Vec<String>,
  pub at: u128,
}

/// 設定。
///
/// **すべてのフィールドに `#[serde(default)]` を付ける。**
/// 設定項目を後から増やしても、既存の state.json がそのまま読めるようにするため。
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
  /// 起動時に隠しファイルを表示するか。
  pub show_hidden: bool,
  /// 起動時にサイドバーを開くか。
  pub show_sidebar: bool,
  /// 起動時にプレビューを開くか。
  pub show_preview: bool,
  /// ゴミ箱へ送る前に確認するか。
  /// **既定は true**。誤操作で消えるのがファイラで一番怖い。
  pub confirm_trash: bool,
  /// 既定の並べ替えキー（`name` / `size` / `modified` / `ext`）。
  pub sort_key: String,
  pub sort_descending: bool,
  /// ディレクトリを常に先頭へ。
  pub dirs_first: bool,
  /// 起動時に直前のセッション（タブ・ペイン）を復元するか。
  pub restore_session: bool,
  /// 窓一覧オーバーレイのホットキー。Tauri のショートカット表記。
  pub overlay_hotkey: String,
  /// アプリ内ショートカット。`アクション名 -> キー表記` の対応。
  /// 未設定のアクションは組み込みの既定値を使う。
  pub shortcuts: std::collections::HashMap<String, String>,
  /// 検索で中へ潜らないフォルダ名（大文字小文字は区別しない）。
  /// 中身が膨大で探す対象になりにくい場所を最初から読まず、読み込みを速くし結果の雑音を減らす。
  pub search_excludes: Vec<String>,
}

/// 検索で既定で除くフォルダ名。
pub fn default_search_excludes() -> Vec<String> {
  [".git", "node_modules", "$RECYCLE.BIN", "System Volume Information"].into_iter().map(String::from).collect()
}

impl Default for Settings {
  fn default() -> Self {
    Self {
      show_hidden: false,
      show_sidebar: true,
      show_preview: false,
      confirm_trash: true,
      sort_key: "name".into(),
      sort_descending: false,
      dirs_first: true,
      restore_session: true,
      overlay_hotkey: "CmdOrCtrl+Shift+Space".into(),
      shortcuts: std::collections::HashMap::new(),
      search_excludes: default_search_excludes(),
    }
  }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaneKind {
  /// The default preserves sessions saved before pane roles were introduced.
  #[default]
  Directory,
  Search,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedSearchState {
  pub scope_paths: Vec<String>,
  pub query: String,
  pub match_path: bool,
  pub sort_key: String,
  pub sort_descending: bool,
  pub dirs_first: bool,
  pub show_preview: Option<bool>,
  pub show_history: Option<bool>,
  pub recent_queries: Vec<String>,
}

impl Default for SavedSearchState {
  fn default() -> Self {
    Self {
      scope_paths: Vec::new(),
      query: String::new(),
      match_path: true,
      sort_key: "name".into(),
      sort_descending: false,
      dirs_first: true,
      show_preview: None,
      show_history: None,
      recent_queries: Vec::new(),
    }
  }
}

/// ペインが訪れた場所ごとの作業状態。
///
/// ペインは「現在地」を1つしか持たないため、移動すると前の場所の
/// スクロール位置・選択・絞り込みが失われる。親へ戻るのと無関係な場所へ
/// 跳ぶのが同じコストになるのはこれが原因なので、場所をキーにして覚えておく。
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedPathState {
  pub path: String,
  pub scroll_top: f64,
  pub selected: Vec<String>,
  pub cursor: usize,
  /// フォルダ内絞り込み語。戻った時に「ファイルが消えている」と誤解させないよう、
  /// 復元する側は絞り込み中であることを画面で明示すること。
  pub filter: String,
  pub sort_key: String,
  pub sort_descending: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedPaneState {
  pub path: String,
  #[serde(default)]
  pub kind: PaneKind,
  /// 転送先として固定し、移動させないペイン。古いセッションには無いので既定は false。
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub pinned: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub search: Option<SavedSearchState>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sidebar: Option<SavedSidebarState>,
  #[serde(default)]
  pub selected_entry: Option<String>,
  #[serde(default)]
  pub scroll_top: Option<f64>,
  /// 訪れた場所ごとの作業状態。新しいものが先頭の LRU で、上限は保存する側で切る。
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub path_states: Vec<SavedPathState>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSidebarState {
  pub primary: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub secondary: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub split_ratio: Option<f64>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedTabState {
  pub panes: Vec<SavedPaneState>,
  pub active_pane_index: usize,
  /// 選んでいるトレイの中身。複数トレイより前の保存データとの互換のため残す。
  #[serde(default)]
  pub tray_paths: Vec<String>,
  /// 名前付きの全トレイ。無ければ `tray_paths` を1つのトレイとして扱う。
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub trays: Vec<SavedTray>,
  #[serde(default)]
  pub active_tray_index: usize,
}

/// 「納品用」「確認待ち」のように目的別に分けた収集トレイ。
#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedTray {
  pub name: String,
  #[serde(default)]
  pub paths: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
  pub tabs: Vec<SavedTabState>,
  pub active_tab_index: usize,
}

/// 配置テンプレートの1ペイン分。
///
/// `pathMode` が相対（current / parent / child）なら、適用時に指定された
/// 基準フォルダから実際のパスを組み立てる。絶対パスで持つと単なるブックマークに
/// なってしまい、「この形を今いる場所に当てる」ができない。
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LayoutPane {
  pub kind: PaneKind,
  /// `absolute` / `current` / `parent` / `child`
  pub path_mode: String,
  /// absolute なら絶対パス、child なら基準からの相対名。それ以外では使わない。
  pub path: String,
  pub pinned: bool,
  pub sidebar: Option<SavedSidebarState>,
  /// 検索ペインとして展開する時の初期検索語。
  pub query: String,
}

/// 名前を付けて呼び出せるペイン配置。
///
/// セッション（いまの状態）とは寿命が違うので別に持つ。
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Layout {
  pub name: String,
  pub panes: Vec<LayoutPane>,
}

/// 移動種別の集計。
///
/// 「俯瞰のような跳躍向けの装置に投資すべきか」は、跳躍（other）の割合と
/// 出戻りの多さで決まる。議論では決まらないので数えて判断する。
/// 1〜2週間ぶんを見たいので、窓を閉じても消えないようここに置く。
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NavTally {
  pub parent: u64,
  pub child: u64,
  pub descendant: u64,
  pub sibling: u64,
  pub other: u64,
  /// 30秒以内に元の場所へ戻った回数。入らずに覗ければ要らなかった往復。
  pub quick_returns: u64,
  /// 数え始めた時刻(ms)。0 なら未開始。何日ぶんの数字かが分からないと判断できない。
  pub since: u128,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct State {
  #[serde(default)]
  pub favorites: Vec<String>,
  #[serde(default)]
  pub history: Vec<HistoryEntry>,
  #[serde(default)]
  pub search_locations: Vec<SearchLocationEntry>,
  #[serde(default)]
  pub settings: Settings,
  #[serde(default)]
  pub last_session: Option<SessionState>,
  /// 名前付きの配置テンプレート。並び順がそのまま Ctrl+1..9 の割り当てになる。
  #[serde(default)]
  pub layouts: Vec<Layout>,
  /// 移動種別の累計。
  #[serde(default)]
  pub nav_tally: NavTally,
}

#[derive(Default)]
pub struct Store {
  state: Mutex<State>,
  /// 保存先。setup で決まるまでは None。
  file: Mutex<Option<PathBuf>>,
  /// 書き込み係のスレッドへ「保存して」と伝える口。attach 前（テスト等）は None で、その場で書く。
  saver: Mutex<Option<Sender<()>>>,
  /// 書き込み係と終了時の flush が同じ一時ファイルを同時に触らないようにする。
  write_lock: Mutex<()>,
  /// 開いている窓ごとの直近のセッション。窓が閉じた時に `last_session` へ昇格させる。
  /// どの窓も保存するので、単に最後に保存された内容ではなく「最後に閉じた窓」を残すために分けて持つ。
  window_sessions: Mutex<std::collections::HashMap<String, SessionState>>,
}

/// 保存要求がこの時間途切れたら書く。
/// セッション状態はタブやペインが少し変わるたびに届くので、毎回ディスクへ書かない。
const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);

fn now_ms() -> u128 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_millis())
    .unwrap_or(0)
}

// ---- 純粋なロジック。I/O から切り離してテストできるようにする ----

/// 履歴に積む。同じ場所は重複させず先頭へ引き上げ、上限で切る。
///
/// 同じフォルダを行き来した時に履歴が同じ名前で埋まると使い物にならないので、
/// 重複排除は必須。
pub fn push_history(history: &mut Vec<HistoryEntry>, path: &str, at: u128) {
  history.retain(|e| e.path != path);
  history.insert(0, HistoryEntry { path: path.to_string(), at });
  history.truncate(HISTORY_CAP);
}

fn search_location_key(paths: &[String]) -> Vec<String> {
  paths
    .iter()
    .map(|path| path.trim().trim_end_matches(['\\', '/']).to_lowercase())
    .filter(|path| !path.is_empty())
    .collect()
}

pub fn push_search_location(
  history: &mut Vec<SearchLocationEntry>,
  paths: Vec<String>,
  at: u128,
) {
  let paths: Vec<String> = paths
    .into_iter()
    .map(|path| path.trim().to_string())
    .filter(|path| !path.is_empty())
    .collect();
  if paths.is_empty() {
    return;
  }
  let key = search_location_key(&paths);
  history.retain(|entry| search_location_key(&entry.paths) != key);
  history.insert(0, SearchLocationEntry { paths, at });
  history.truncate(SEARCH_HISTORY_CAP);
}

/// お気に入りの同一性。Windows なので大文字小文字と区切りの向き・末尾を無視する。
///
/// 近くの `search_location_key` も似た正規化をするが、あちらは `C:\` を `C:` まで
/// 削る。検索場所のキーとしては実害がないものの、お気に入りはドライブ直下を
/// 登録しうるので別に持つ。フロント側の `pathIdentity` と規則を合わせてある。
fn favorite_key(path: &str) -> String {
  let trimmed = path.trim().replace('/', "\\");
  let is_drive_root = trimmed.len() == 3 && trimmed.ends_with(":\\");
  if is_drive_root {
    trimmed.to_lowercase()
  } else {
    trimmed.trim_end_matches('\\').to_lowercase()
  }
}

/// 落とされたパスを末尾へ足す。既に登録されているものは飛ばす。
///
/// `toggle_favorite` は名前どおり在籍を反転するので、ドロップからは呼べない。
/// 登録済みのものを落とすと解除になり、足すつもりの操作で消えることになる。
/// 戻り値は実際に足した件数で、呼ぶ側が「何件入ったか」を言えるようにする。
pub fn add_favorites(favorites: &mut Vec<String>, paths: &[String]) -> usize {
  let mut known: Vec<String> = favorites.iter().map(|p| favorite_key(p)).collect();
  let mut added = 0;
  for path in paths {
    let path = path.trim();
    if path.is_empty() {
      continue;
    }
    let key = favorite_key(path);
    if known.contains(&key) {
      continue;
    }
    known.push(key);
    favorites.push(path.to_string());
    added += 1;
  }
  added
}

/// 登録されていなければ足し、されていれば外す。戻り値は操作後に登録されているか。
pub fn toggle_favorite(favorites: &mut Vec<String>, path: &str) -> bool {
  if let Some(i) = favorites.iter().position(|p| p == path) {
    favorites.remove(i);
    false
  } else {
    favorites.push(path.to_string());
    true
  }
}

/// 並べ替え。範囲外の添字が来ても何もしない（UI 側の取り違えで壊さないため）。
pub fn reorder(favorites: &mut Vec<String>, from: usize, to: usize) {
  if from >= favorites.len() || to >= favorites.len() || from == to {
    return;
  }
  let item = favorites.remove(from);
  favorites.insert(to, item);
}

// ---- I/O ----

impl Store {
  /// 保存先を決めて、既存の内容を読み込む。
  pub fn attach(&self, app: &AppHandle) {
    let Ok(dir) = app.path().app_config_dir() else {
      log::warn!("設定ディレクトリを解決できません。お気に入りと履歴は保存されません");
      return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("state.json");

    if let Ok(text) = std::fs::read_to_string(&file) {
      match serde_json::from_str::<State>(&text) {
        Ok(loaded) => *self.state.lock().unwrap() = loaded,
        // 壊れていても起動は止めない。上書き保存で回復する。
        Err(e) => log::warn!("{} を読めません: {e}", file.display()),
      }
    }
    *self.file.lock().unwrap() = Some(file);

    let (tx, rx) = channel::<()>();
    let handle = app.clone();
    std::thread::spawn(move || {
      while rx.recv().is_ok() {
        // 続けて届く要求はまとめて1回にする。
        loop {
          match rx.recv_timeout(SAVE_DEBOUNCE) {
            Ok(()) => continue,
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => break,
          }
        }
        handle.state::<Store>().save_now();
      }
    });
    *self.saver.lock().unwrap() = Some(tx);
  }

  /// 保存を予約する。書き込み係がいなければその場で書く。
  fn request_save(&self) {
    let sent = self.saver.lock().unwrap().as_ref().map(|tx| tx.send(()).is_ok()).unwrap_or(false);
    if !sent {
      self.save_now();
    }
  }

  /// 予約を待たずに書く。終了時に呼び、直前の変更を取りこぼさない。
  pub fn flush(&self) {
    self.save_now();
  }

  fn save_now(&self) {
    let _writing = self.write_lock.lock().unwrap_or_else(|value| value.into_inner());
    let Some(file) = self.file.lock().unwrap().clone() else { return };
    let state = self.state.lock().unwrap().clone();
    let Ok(text) = serde_json::to_string_pretty(&state) else { return };

    // 直接書くと、書き込み中に落ちた時に既存の内容ごと失う。
    // 一時ファイルへ書いてから差し替える。
    let tmp = file.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
      if let Err(e) = std::fs::rename(&tmp, &file) {
        log::warn!("保存に失敗: {e}");
      }
    }
  }

  fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
    let out = f(&mut self.state.lock().unwrap());
    self.request_save();
    out
  }

  /// 窓のセッションを覚える。落ちた時にも直近が残るよう `last_session` も更新する。
  fn record_window_session(&self, label: &str, session: SessionState) {
    self.window_sessions.lock().unwrap().insert(label.to_string(), session.clone());
    self.with(|s| s.last_session = Some(session));
  }

  /// 閉じた窓のセッションを次回起動時の復元対象にする。
  ///
  /// 窓は1枚ずつ閉じられ、最後に閉じた窓がここを最後に通る。
  /// 別の窓がその後に保存していても、閉じた順が優先される。
  pub fn promote_window_session(&self, label: &str) {
    let Some(session) = self.window_sessions.lock().unwrap().remove(label) else { return };
    self.with(|s| s.last_session = Some(session));
  }

  /// 検索で除くフォルダ名。検索の開始時に読む（設定を変えたら次の読み込みから効く）。
  pub fn search_excludes(&self) -> Vec<String> {
    self.state.lock().unwrap().settings.search_excludes.clone()
  }

  /// 設定されたオーバーレイのホットキー。
  /// setup 時（コマンド経由でない場所）から読むために用意している。
  pub fn overlay_hotkey(&self) -> String {
    self.state.lock().unwrap().settings.overlay_hotkey.clone()
  }
}

// ---- コマンド ----

#[tauri::command]
pub fn list_favorites(app: AppHandle) -> Vec<String> {
  app.state::<Store>().state.lock().unwrap().favorites.clone()
}

/// 登録済みなら解除、未登録なら登録。戻り値は操作後に登録されているか。
#[tauri::command]
pub fn toggle_favorite_cmd(app: AppHandle, path: String) -> bool {
  app.state::<Store>().with(|s| toggle_favorite(&mut s.favorites, &path))
}

#[tauri::command]
pub fn add_favorites_cmd(app: AppHandle, paths: Vec<String>) -> usize {
  app.state::<Store>().with(|s| add_favorites(&mut s.favorites, &paths))
}

#[tauri::command]
pub fn remove_favorite(app: AppHandle, path: String) {
  app.state::<Store>().with(|s| s.favorites.retain(|p| p != &path));
}

#[tauri::command]
pub fn reorder_favorite(app: AppHandle, from: usize, to: usize) {
  app.state::<Store>().with(|s| reorder(&mut s.favorites, from, to));
}

#[tauri::command]
pub fn list_history(app: AppHandle) -> Vec<HistoryEntry> {
  app.state::<Store>().state.lock().unwrap().history.clone()
}

#[tauri::command]
pub fn record_history(app: AppHandle, path: String) {
  app.state::<Store>().with(|s| push_history(&mut s.history, &path, now_ms()));
}

#[tauri::command]
pub fn clear_history(app: AppHandle) {
  app.state::<Store>().with(|s| s.history.clear());
}

#[tauri::command]
pub fn list_search_locations(app: AppHandle) -> Vec<SearchLocationEntry> {
  app.state::<Store>().state.lock().unwrap().search_locations.clone()
}

#[tauri::command]
pub fn record_search_location(app: AppHandle, paths: Vec<String>) {
  app
    .state::<Store>()
    .with(|state| push_search_location(&mut state.search_locations, paths, now_ms()));
}

#[tauri::command]
pub fn remove_search_location(app: AppHandle, paths: Vec<String>) {
  let key = search_location_key(&paths);
  app
    .state::<Store>()
    .with(|state| state.search_locations.retain(|entry| search_location_key(&entry.paths) != key));
}

#[tauri::command]
pub fn clear_search_locations(app: AppHandle) {
  app.state::<Store>().with(|state| state.search_locations.clear());
}

/// パスの実在と種別。
///
/// 実在だけでは足りない。トレイはファイルとフォルダを同じ一覧に持つが、
/// フォルダは「行き先」にもなるため、呼び出し側が動作を出し分ける必要がある。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathKind {
  pub exists: bool,
  pub is_dir: bool,
}

/// 実在と種別をまとめて調べる。
///
/// お気に入り・履歴・トレイは、消えた場所を指したまま残る。掴んでから
/// 「開けません」と言われるより、先に灰色で示すほうが親切。
///
/// 1件につき `is_dir` と `is_file` で最大2回 stat する。★や履歴が
/// 切断されたネットワークドライブを指していると1件で数秒かかるため、
/// `(async)` でメインスレッドから外すことがとりわけ重要になる。
#[tauri::command(async)]
pub fn path_kinds(paths: Vec<String>) -> Vec<PathKind> {
  paths.iter().map(|p| path_kind(Path::new(p))).collect()
}

/// 1件ぶんの判定。テストしやすいよう I/O の呼び出し方だけを切り出す。
fn path_kind(path: &Path) -> PathKind {
  // is_dir() は「存在しない」と「ファイルである」を区別しない。
  // トレイのファイルが全件「見つかりません」になっていたのはこの取り違えが原因。
  let is_dir = path.is_dir();
  PathKind { exists: is_dir || path.is_file(), is_dir }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
  app.state::<Store>().state.lock().unwrap().settings.clone()
}

/// 設定を丸ごと差し替えて保存する。
#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) {
  app.state::<Store>().with(|s| s.settings = settings);
}

/// 設定を初期値へ戻す。
#[tauri::command]
pub fn reset_settings(app: AppHandle) -> Settings {
  app.state::<Store>().with(|s| {
    s.settings = Settings::default();
    s.settings.clone()
  })
}

/// 移動種別の累計を取得する。
#[tauri::command]
pub fn get_nav_tally(app: AppHandle) -> NavTally {
  app.state::<Store>().state.lock().unwrap().nav_tally
}

/// 移動種別の累計を保存する。フロントが一定回数ごとにまとめて送る。
#[tauri::command]
pub fn save_nav_tally(app: AppHandle, tally: NavTally) {
  app.state::<Store>().with(|s| {
    let mut next = tally;
    // 数え始めは最初の保存時に確定させる。以後は上書きしない。
    if next.since == 0 {
      next.since = if s.nav_tally.since == 0 { now_ms() } else { s.nav_tally.since };
    }
    s.nav_tally = next;
  });
}

/// 保存されている配置テンプレートを取得する。
#[tauri::command]
pub fn get_layouts(app: AppHandle) -> Vec<Layout> {
  app.state::<Store>().state.lock().unwrap().layouts.clone()
}

/// 配置テンプレートを丸ごと差し替えて保存する。
#[tauri::command]
pub fn save_layouts(app: AppHandle, layouts: Vec<Layout>) {
  app.state::<Store>().with(|s| s.layouts = layouts);
}

/// セッション状態（直前に開いていたタブとペイン）を保存する。
/// `label` はその窓。最後に閉じた窓のセッションが次回起動時に復元される。
#[tauri::command]
pub fn save_session_state(app: AppHandle, label: String, session: SessionState) {
  app.state::<Store>().record_window_session(&label, session);
}

/// 保存されているセッション状態を取得する。
#[tauri::command]
pub fn get_session_state(app: AppHandle) -> Option<SessionState> {
  app.state::<Store>().state.lock().unwrap().last_session.clone()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn paths(history: &[HistoryEntry]) -> Vec<&str> {
    history.iter().map(|e| e.path.as_str()).collect()
  }

  /// トレイはファイルとフォルダを同じ一覧に持つ。両者を取り違えると、
  /// ファイルが全件「見つかりません」になる（実際にそうなっていた）。
  #[test]
  fn path_kind_separates_missing_from_file() {
    let dir = std::env::temp_dir().join("trayce_path_kind_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let file = dir.join("note.txt");
    std::fs::write(&file, b"x").unwrap();
    let missing = dir.join("nope.txt");

    assert_eq!(path_kind(&dir), PathKind { exists: true, is_dir: true });
    assert_eq!(path_kind(&file), PathKind { exists: true, is_dir: false });
    assert_eq!(path_kind(&missing), PathKind { exists: false, is_dir: false });

    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn history_puts_the_newest_first() {
    let mut h = Vec::new();
    push_history(&mut h, r"C:\a", 1);
    push_history(&mut h, r"C:\b", 2);
    assert_eq!(paths(&h), vec![r"C:\b", r"C:\a"]);
  }

  /// 同じ所を行き来した時に履歴が埋まると使い物にならない。
  #[test]
  fn history_does_not_duplicate_the_same_path() {
    let mut h = Vec::new();
    push_history(&mut h, r"C:\a", 1);
    push_history(&mut h, r"C:\b", 2);
    push_history(&mut h, r"C:\a", 3);

    assert_eq!(paths(&h), vec![r"C:\a", r"C:\b"], "再訪は先頭へ引き上げる");
    assert_eq!(h[0].at, 3, "時刻も更新される");
  }

  #[test]
  fn history_is_capped() {
    let mut h = Vec::new();
    for i in 0..(HISTORY_CAP + 50) {
      push_history(&mut h, &format!("C:\\{i}"), i as u128);
    }
    assert_eq!(h.len(), HISTORY_CAP);
    assert_eq!(h[0].path, format!("C:\\{}", HISTORY_CAP + 49), "最新が残る");
  }

  #[test]
  fn search_location_history_is_separate_deduplicated_and_capped() {
    let mut history = Vec::new();
    push_search_location(&mut history, vec![r"C:\Work\".into()], 1);
    push_search_location(&mut history, vec![r"c:\work".into()], 2);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].paths, [r"c:\work"]);
    assert_eq!(history[0].at, 2);

    for index in 0..20 {
      push_search_location(&mut history, vec![format!(r"D:\scope-{index}")], index);
    }
    assert_eq!(history.len(), SEARCH_HISTORY_CAP);
    assert_eq!(history[0].paths, [r"D:\scope-19"]);
  }

  #[test]
  fn search_location_history_preserves_multiple_roots() {
    let mut history = Vec::new();
    push_search_location(&mut history, vec![r"C:\one".into(), r"D:\two".into()], 1);
    assert_eq!(history[0].paths, [r"C:\one", r"D:\two"]);
    push_search_location(&mut history, vec!["  ".into()], 2);
    assert_eq!(history.len(), 1);
  }

  #[test]
  fn toggling_a_favorite_adds_then_removes() {
    let mut f = Vec::new();
    assert!(toggle_favorite(&mut f, r"C:\a"));
    assert_eq!(f, vec![r"C:\a".to_string()]);
    assert!(!toggle_favorite(&mut f, r"C:\a"));
    assert!(f.is_empty());
  }

  #[test]
  fn adding_favorites_skips_ones_already_there() {
    let mut f = vec![r"C:\a".to_string()];
    // 落とした3件のうち新しいのは2件。戻り値はその2件を指す。
    let added = add_favorites(
      &mut f,
      &[r"C:\a".to_string(), r"C:\b".to_string(), r"C:\c".to_string()],
    );
    assert_eq!(added, 2);
    assert_eq!(f, vec![r"C:\a", r"C:\b", r"C:\c"]);
  }

  #[test]
  fn adding_favorites_ignores_case_and_trailing_separators() {
    let mut f = vec![r"C:\Work".to_string()];
    let added = add_favorites(&mut f, &[r"c:\work\".to_string(), "C:/Work".to_string()]);
    assert_eq!(added, 0);
    assert_eq!(f, vec![r"C:\Work"]);
  }

  #[test]
  fn adding_favorites_keeps_a_drive_root_distinct() {
    // 末尾の区切りを無条件に削ると C:\ が C: になり、別物が同じものになってしまう。
    let mut f = Vec::new();
    let added = add_favorites(&mut f, &[r"C:\".to_string(), "C:".to_string()]);
    assert_eq!(added, 2);
    assert_eq!(f, vec![r"C:\", "C:"]);
  }

  #[test]
  fn adding_favorites_drops_duplicates_inside_one_drop() {
    // 同じドロップに同じものが2つ入っていても、増えるのは1つだけ。空文字は数えない。
    let mut f = Vec::new();
    let added = add_favorites(&mut f, &[r"C:\a".to_string(), r"C:\A".to_string(), "  ".to_string()]);
    assert_eq!(added, 1);
    assert_eq!(f, vec![r"C:\a"]);
  }

  #[test]
  fn reorder_moves_an_item() {
    let mut f = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    reorder(&mut f, 0, 2);
    assert_eq!(f, vec!["b", "c", "a"]);
  }

  #[test]
  fn reorder_backwards_works() {
    let mut f = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    reorder(&mut f, 2, 0);
    assert_eq!(f, vec!["c", "a", "b"]);
  }

  /// UI の添字ずれで並びを壊さないこと。
  #[test]
  fn reorder_ignores_out_of_range_indices() {
    let mut f = vec!["a".to_string(), "b".to_string()];
    reorder(&mut f, 5, 0);
    reorder(&mut f, 0, 9);
    reorder(&mut f, 1, 1);
    assert_eq!(f, vec!["a", "b"]);
  }

  /// 保存形式は前方互換にしておく。片方しか無いファイルでも読めること。
  #[test]
  fn state_deserializes_with_missing_fields() {
    let s: State = serde_json::from_str("{}").unwrap();
    assert!(s.favorites.is_empty());
    assert!(s.history.is_empty());

    let s: State = serde_json::from_str(r#"{"favorites":["C:\\a"]}"#).unwrap();
    assert_eq!(s.favorites, vec![r"C:\a".to_string()]);
  }

  fn session_at(path: &str) -> SessionState {
    SessionState {
      tabs: vec![SavedTabState {
        panes: vec![SavedPaneState { path: path.into(), ..Default::default() }],
        ..Default::default()
      }],
      active_tab_index: 0,
    }
  }

  fn restored_path(store: &Store) -> String {
    store.state.lock().unwrap().last_session.as_ref().unwrap().tabs[0].panes[0].path.clone()
  }

  /// 最後に保存した窓ではなく、最後に閉じた窓が次回の復元対象になる。
  #[test]
  fn the_last_closed_window_wins_over_the_last_saved_one() {
    let store = Store::default();
    store.record_window_session("main", session_at(r"C:\main"));
    store.record_window_session("filer-1", session_at(r"D:\detached"));
    // 後から main が保存し直す（別の窓で作業していた）。
    store.record_window_session("main", session_at(r"C:\main-later"));
    assert_eq!(restored_path(&store), r"C:\main-later", "落ちた時に備えて直近は常に残す");

    // main を先に閉じ、切り離した窓を最後に閉じる。
    store.promote_window_session("main");
    store.promote_window_session("filer-1");
    assert_eq!(restored_path(&store), r"D:\detached");
  }

  /// セッションを保存しない窓（オーバーレイ等）が閉じても、復元対象を消さない。
  #[test]
  fn closing_a_window_without_a_session_keeps_the_previous_one() {
    let store = Store::default();
    store.record_window_session("main", session_at(r"C:\main"));
    store.promote_window_session("main");
    store.promote_window_session("overlay");
    assert_eq!(restored_path(&store), r"C:\main");
  }

  #[test]
  fn session_without_tray_restores_an_empty_tray() {
    let session: SessionState = serde_json::from_str(
      r#"{"tabs":[{"panes":[{"path":"C:\\work"}],"activePaneIndex":0}],"activeTabIndex":0}"#,
    )
    .unwrap();

    assert_eq!(session.tabs.len(), 1);
    assert!(session.tabs[0].tray_paths.is_empty());
    assert_eq!(session.tabs[0].panes[0].kind, PaneKind::Directory);
    assert!(session.tabs[0].panes[0].search.is_none());
    assert!(session.tabs[0].panes[0].sidebar.is_none());
  }

  #[test]
  fn named_trays_round_trip_and_older_sessions_keep_a_single_tray() {
    let session: SessionState = serde_json::from_str(
      r#"{"tabs":[{"panes":[{"path":"C:\\w"}],"activePaneIndex":0,"trayPaths":["C:\\b"],"trays":[{"name":"納品用","paths":["C:\\a"]},{"name":"確認待ち","paths":["C:\\b"]}],"activeTrayIndex":1}],"activeTabIndex":0}"#,
    )
    .unwrap();
    let tab = &session.tabs[0];
    assert_eq!(tab.trays.len(), 2);
    assert_eq!(tab.trays[0], SavedTray { name: "納品用".into(), paths: vec![r"C:\a".into()] });
    assert_eq!(tab.active_tray_index, 1);
    let encoded = serde_json::to_string(&session).unwrap();
    assert!(encoded.contains(r#""activeTrayIndex":1"#) && encoded.contains("確認待ち"));

    let older: SessionState =
      serde_json::from_str(r#"{"tabs":[{"panes":[],"activePaneIndex":0,"trayPaths":["C:\\x"]}],"activeTabIndex":0}"#).unwrap();
    assert!(older.tabs[0].trays.is_empty(), "古い形式は trays を持たない（フロントが trayPaths から1つ作る）");
    assert_eq!(older.tabs[0].tray_paths, [r"C:\x"]);
    assert_eq!(older.tabs[0].active_tray_index, 0);
  }

  #[test]
  fn pinned_panes_round_trip_and_default_to_unpinned() {
    let session: SessionState = serde_json::from_str(
      r#"{"tabs":[{"panes":[{"path":"C:\\dest","pinned":true},{"path":"C:\\browse"}],"activePaneIndex":1}],"activeTabIndex":0}"#,
    )
    .unwrap();
    assert!(session.tabs[0].panes[0].pinned);
    assert!(!session.tabs[0].panes[1].pinned, "古いセッションや未指定は固定しない");

    let encoded = serde_json::to_string(&session).unwrap();
    assert_eq!(encoded.matches("pinned").count(), 1, "固定していないペインには書かない");
  }

  #[test]
  fn split_sidebar_state_round_trips() {
    let session: SessionState = serde_json::from_str(
      r#"{"tabs":[{"panes":[{"path":"C:\\work","sidebar":{"primary":"tree","secondary":"history","splitRatio":0.62}}],"activePaneIndex":0}],"activeTabIndex":0}"#,
    )
    .unwrap();

    let sidebar = session.tabs[0].panes[0].sidebar.as_ref().unwrap();
    assert_eq!(sidebar.primary, "tree");
    assert_eq!(sidebar.secondary.as_deref(), Some("history"));
    assert_eq!(sidebar.split_ratio, Some(0.62));

    let encoded = serde_json::to_string(&session).unwrap();
    assert!(encoded.contains(r#""secondary":"history""#));
    assert!(encoded.contains(r#""splitRatio":0.62"#));
  }

  #[test]
  fn search_pane_state_round_trips_without_results() {
    let session: SessionState = serde_json::from_str(
      r#"{"tabs":[{"panes":[{"path":"C:\\work","kind":"search","search":{"scopePaths":["C:\\work","D:\\assets"],"query":"blue icon","matchPath":true,"sortKey":"modified","sortDescending":true,"dirsFirst":false,"showPreview":true,"recentQueries":["blue icon","green|red !draft"]}}],"activePaneIndex":0}],"activeTabIndex":0}"#,
    )
    .unwrap();

    let pane = &session.tabs[0].panes[0];
    assert_eq!(pane.kind, PaneKind::Search);
    let search = pane.search.as_ref().unwrap();
    assert_eq!(search.scope_paths, vec![r"C:\work", r"D:\assets"]);
    assert_eq!(search.query, "blue icon");
    assert!(search.match_path);
    assert_eq!(search.sort_key, "modified");
    assert!(search.sort_descending);
    assert!(!search.dirs_first);
    assert_eq!(search.show_preview, Some(true));
    assert_eq!(search.recent_queries, ["blue icon", "green|red !draft"]);

    let encoded = serde_json::to_string(&session).unwrap();
    assert!(encoded.contains(r#""kind":"search""#));
    assert!(!encoded.contains("results"));
  }

  #[test]
  fn older_search_pane_gets_safe_presentation_defaults() {
    let search: SavedSearchState = serde_json::from_str(
      r#"{"scopePaths":["C:\\work"],"query":"report","matchPath":true}"#,
    )
    .unwrap();
    assert_eq!(search.sort_key, "name");
    assert!(!search.sort_descending);
    assert!(search.dirs_first);
    assert_eq!(search.show_preview, None);
    assert_eq!(search.show_history, None);
    assert!(search.recent_queries.is_empty());
  }

  /// 設定を追加する前の state.json でも読めること。
  /// 既存ユーザーの設定ファイルを壊さないための担保。
  #[test]
  fn settings_default_when_absent_from_saved_state() {
    let s: State = serde_json::from_str(r#"{"favorites":[],"history":[]}"#).unwrap();
    assert!(s.settings.confirm_trash, "確認ダイアログは既定で有効であるべき");
    assert!(s.settings.show_sidebar);
    assert!(!s.settings.show_preview);
    assert_eq!(s.settings.sort_key, "name");
  }

  /// 除外設定を追加する前の設定でも、既定の除外が効くこと。空にした利用者の選択は保つこと。
  #[test]
  fn search_excludes_default_for_older_settings_and_respect_an_empty_choice() {
    let older: State = serde_json::from_str(r#"{"settings":{"showHidden":true}}"#).unwrap();
    assert_eq!(older.settings.search_excludes, default_search_excludes());

    let cleared: State = serde_json::from_str(r#"{"settings":{"searchExcludes":[]}}"#).unwrap();
    assert!(cleared.settings.search_excludes.is_empty(), "除外しないと決めた設定を既定で上書きしない");
  }

  /// 設定項目を後から増やしても、一部しか無い JSON が読めること。
  #[test]
  fn settings_tolerates_partial_json() {
    let s: State =
      serde_json::from_str(r#"{"settings":{"showHidden":true,"sortKey":"modified"}}"#).unwrap();
    assert!(s.settings.show_hidden);
    assert_eq!(s.settings.sort_key, "modified");
    // 書かれていなかった項目は既定のまま
    assert!(s.settings.confirm_trash);
    assert!(s.settings.dirs_first);
  }

  #[test]
  fn settings_round_trip_through_json() {
    let mut settings = Settings::default();
    settings.show_hidden = true;
    settings.confirm_trash = false;
    settings.shortcuts.insert("undo".into(), "Ctrl+U".into());

    let text = serde_json::to_string(&settings).unwrap();
    let back: Settings = serde_json::from_str(&text).unwrap();

    assert!(back.show_hidden);
    assert!(!back.confirm_trash);
    assert_eq!(back.shortcuts.get("undo").map(String::as_str), Some("Ctrl+U"));
  }
}
