use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedPaneState {
  pub path: String,
  #[serde(default)]
  pub kind: PaneKind,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub search: Option<SavedSearchState>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sidebar: Option<SavedSidebarState>,
  #[serde(default)]
  pub selected_entry: Option<String>,
  #[serde(default)]
  pub scroll_top: Option<f64>,
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
  #[serde(default)]
  pub tray_paths: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
  pub tabs: Vec<SavedTabState>,
  pub active_tab_index: usize,
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
}

#[derive(Default)]
pub struct Store {
  state: Mutex<State>,
  /// 保存先。setup で決まるまでは None。
  file: Mutex<Option<PathBuf>>,
}

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
  }

  fn save(&self) {
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
    self.save();
    out
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

/// 保存されている場所がまだ存在するか。消えたフォルダを一覧で灰色にするのに使う。
#[tauri::command]
pub fn paths_exist(paths: Vec<String>) -> Vec<bool> {
  paths.iter().map(|p| Path::new(p).is_dir()).collect()
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

/// セッション状態（直前に開いていたタブとペイン）を保存する。
#[tauri::command]
pub fn save_session_state(app: AppHandle, session: SessionState) {
  app.state::<Store>().with(|s| s.last_session = Some(session));
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
