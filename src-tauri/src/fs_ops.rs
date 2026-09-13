use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 一覧の1項目。
///
/// **絶対パスは持たせない。** `親ディレクトリ + 区切り + name` で必ず再現できるのに、
/// 全項目に持たせると転送量が倍近くになる（WinSxS 27,498件で計測: 文字列 5.4MB のうち
/// 54% が path だった）。フロントで組み立てる。
#[derive(Serialize)]
pub struct Entry {
  pub name: String,
  pub is_dir: bool,
  /// バイト数。ディレクトリは中身を数えないので 0。
  pub size: u64,
  /// 最終更新時刻(ms)。取れなければ 0。
  /// 「さっきいじったやつ」を探すのに一番効く情報なので必ず持たせる。
  pub modified: u128,
  /// 小文字の拡張子（`.` を含まない）。無ければ空。
  pub ext: String,
  /// 隠し属性。既定では表示しないが、判定はここで済ませておく。
  pub hidden: bool,
}

/// 一覧の並べ方。フロントで並べ替えると、大量ファイル時に
/// 毎回全件を JS 側で回すことになるので Rust 側で確定させる。
#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
  Name,
  Size,
  Modified,
  Ext,
}

impl Default for SortKey {
  fn default() -> Self {
    Self::Name
  }
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
  #[serde(default)]
  pub key: SortKey,
  #[serde(default)]
  pub descending: bool,
  /// ディレクトリを常に先頭へ。ファイラでは混ぜない方が探しやすい。
  #[serde(default = "default_true")]
  pub dirs_first: bool,
  #[serde(default)]
  pub show_hidden: bool,
}

fn default_true() -> bool {
  true
}

/// `#[serde(default = ...)]` はデシリアライズ時にしか効かない。
/// `Default::default()` にも同じ既定を効かせるため手で書く
/// （derive のままだと `dirs_first` が false になり、指定なし呼び出しで並びが崩れる）。
impl Default for SortSpec {
  fn default() -> Self {
    Self {
      key: SortKey::default(),
      descending: false,
      dirs_first: true,
      show_hidden: false,
    }
  }
}

#[derive(Serialize)]
pub struct DirListing {
  /// 正規化された絶対パス。フロントの表示とドラッグの両方がこれを使う。
  pub path: String,
  /// ルートの場合は None。
  pub parent: Option<String>,
  pub entries: Vec<Entry>,
}

/// 起動時の初期ディレクトリ。ホームが取れなければカレントに落とす。
#[tauri::command]
pub fn home_dir() -> String {
  dirs::home_dir()
    .or_else(|| std::env::current_dir().ok())
    .unwrap_or_else(|| PathBuf::from("."))
    .to_string_lossy()
    .to_string()
}

/// 利用可能なドライブ。ツリーの根として使う。
///
/// Windows にはドライブをまとめて列挙する標準APIが std に無いので、
/// A〜Z を総当りして存在するものを拾う。26回の stat なので実用上は充分速い。
#[tauri::command]
pub fn drives() -> Vec<String> {
  (b'A'..=b'Z')
    .map(|c| format!("{}:\\", c as char))
    .filter(|d| Path::new(d).is_dir())
    .collect()
}

/// ディレクトリ直下のサブディレクトリだけを返す。
///
/// ツリーの展開に使う。`list_dir` でも同じことはできるが、
/// ファイルが数万ある場所を展開した時に、使わないファイル分まで
/// 詰めて IPC で送ることになるので分けている。
#[tauri::command]
pub fn list_subdirs(path: String, show_hidden: Option<bool>) -> Result<Vec<Entry>, String> {
  let show_hidden = show_hidden.unwrap_or(false);
  let read = std::fs::read_dir(&path).map_err(|e| format!("{path} を読めません: {e}"))?;

  let mut dirs: Vec<Entry> = read
    .flatten() // 権限が無い等で個別に失敗するものは読み飛ばす
    .filter(|item| item.file_type().map(|t| t.is_dir()).unwrap_or(false))
    .map(|item| to_entry(&item))
    // 一覧とツリーで見えるものが食い違うと混乱するので、同じ設定に従わせる。
    .filter(|e| show_hidden || !e.hidden)
    .collect();

  dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
  Ok(dirs)
}

/// ディレクトリを1階層読む。
#[tauri::command]
pub fn list_dir(path: String, sort: Option<SortSpec>) -> Result<DirListing, String> {
  let sort = sort.unwrap_or_default();
  let dir = Path::new(&path);

  // シンボリックリンクや `..` を畳んでから扱う。表示パスとドラッグで渡すパスを一致させるため。
  let dir = dir
    .canonicalize()
    .map_err(|e| format!("{path} を開けません: {e}"))?;

  let read = std::fs::read_dir(&dir).map_err(|e| format!("{} を読めません: {e}", dir.display()))?;

  let mut entries: Vec<Entry> = Vec::new();
  for item in read {
    // 権限が無い等で個別に失敗するものは、全体を落とさず読み飛ばす。
    let Ok(item) = item else { continue };
    let entry = to_entry(&item);
    if entry.hidden && !sort.show_hidden {
      continue;
    }
    entries.push(entry);
  }

  sort_entries(&mut entries, sort);

  Ok(DirListing {
    path: strip_unc(&dir),
    parent: dir.parent().map(strip_unc),
    entries,
  })
}

fn to_entry(item: &std::fs::DirEntry) -> Entry {
  let name = item.file_name().to_string_lossy().to_string();
  let meta = item.metadata().ok();
  let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

  Entry {
    // ディレクトリのサイズは中身の合計ではなく 0 にする。
    // 再帰的に数えると一覧を出すだけでディスクを舐めることになる。
    size: if is_dir { 0 } else { meta.as_ref().map(|m| m.len()).unwrap_or(0) },
    modified: meta
      .as_ref()
      .and_then(|m| m.modified().ok())
      .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
      .map(|d| d.as_millis())
      .unwrap_or(0),
    ext: if is_dir {
      String::new()
    } else {
      Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
    },
    hidden: is_hidden(&name, meta.as_ref()),
    is_dir,
    name,
  }
}

/// Windows の隠し属性と、Unix 流の先頭ドットの両方を隠しとみなす。
/// `.cargo` のような設定フォルダは属性が付いていないので、名前でも判定する。
fn is_hidden(name: &str, meta: Option<&std::fs::Metadata>) -> bool {
  if name.starts_with('.') {
    return true;
  }
  #[cfg(windows)]
  {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    if let Some(m) = meta {
      return m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0;
    }
  }
  #[cfg(not(windows))]
  let _ = meta;
  false
}

/// 並べ替え。同値になった時は必ず名前で決着させ、順序がぶれないようにする。
/// ぶれると、更新のたびに行が入れ替わって目で追えなくなる。
fn sort_entries(entries: &mut [Entry], sort: SortSpec) {
  entries.sort_by(|a, b| {
    if sort.dirs_first {
      let by_kind = b.is_dir.cmp(&a.is_dir);
      if by_kind != std::cmp::Ordering::Equal {
        return by_kind;
      }
    }

    let by_key = match sort.key {
      SortKey::Name => std::cmp::Ordering::Equal,
      SortKey::Size => a.size.cmp(&b.size),
      SortKey::Modified => a.modified.cmp(&b.modified),
      SortKey::Ext => a.ext.cmp(&b.ext),
    };

    let by_key = if sort.descending { by_key.reverse() } else { by_key };

    // 名前は最後の決着役。キーが名前の場合も降順を効かせる。
    let by_name = a.name.to_lowercase().cmp(&b.name.to_lowercase());
    let by_name = if sort.descending && sort.key == SortKey::Name {
      by_name.reverse()
    } else {
      by_name
    };

    by_key.then(by_name)
  });
}

/// 落とされたファイル群を `dest` ディレクトリへ取り込む。
///
/// `move_files` が false ならコピー。既存ファイルは黙って上書きせず、
/// `name (2).ext` のように退避名を作る。ファイラで黙って消えるのが一番怖いため。
#[tauri::command]
pub fn accept_dropped(paths: Vec<String>, dest: String, move_files: bool) -> Result<Vec<String>, String> {
  // 進捗も中断も要らない経路。中断しない旗と何もしない通知を渡すだけ。
  let never = std::sync::atomic::AtomicBool::new(false);
  let pairs = transfer(&paths, &dest, move_files, &never, &mut |_, _| {})?;
  Ok(pairs.into_iter().map(|(_, to)| to.to_string_lossy().to_string()).collect())
}

/// 転送の進み具合。
pub struct Progress {
  pub bytes_done: u64,
  pub files_done: u64,
}

/// 進捗付きで転送する。`transfer` モジュールから呼ぶ入口。
///
/// 戻り値は (元のパス, 作られたパス) のペア。undo で元へ戻す時に必要になる
/// （`paths` と結果は、自分自身の中へ入れた場合など件数がずれることがあるので
/// 呼び出し側で対応を取り直さず、ここで確定したペアをそのまま返す）。
pub fn transfer_pub(
  paths: &[String],
  dest: &str,
  move_files: bool,
  cancel: &std::sync::atomic::AtomicBool,
  on_progress: &mut dyn FnMut(&Progress, &str),
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
  transfer(paths, dest, move_files, cancel, on_progress)
}

/// 総量の事前集計。戻り値は (ファイル数, バイト数)。
///
/// 移動で `dest` と同じボリュームにある項目は rename で済むため数えない。
/// 巨大なフォルダを同じドライブ内で動かす時に、中身を全部舐める待ち時間を無くす。
pub fn scan_total_pub(paths: &[String], dest: &str, move_files: bool) -> (u64, u64) {
  let copied: Vec<String> = paths
    .iter()
    .filter(|p| !(move_files && same_volume(Path::new(p), Path::new(dest))))
    .cloned()
    .collect();
  scan_total(&copied)
}

/// 2つのパスが同じドライブ（`C:` や `\\server\share`）にあるか。
/// 判定できない場合は false にして、数え漏れより数え過ぎに倒す。
fn same_volume(a: &Path, b: &Path) -> bool {
  use std::path::Component;
  match (a.components().next(), b.components().next()) {
    (Some(Component::Prefix(x)), Some(Component::Prefix(y))) => {
      x.as_os_str().to_string_lossy().to_lowercase() == y.as_os_str().to_string_lossy().to_lowercase()
    }
    _ => false,
  }
}

/// コピー / 移動の本体。
///
/// `cancel` が立ったら速やかに諦める。`on_progress` は
/// 「今どこまで進んだか」と「今どのファイルか」を受け取る。
/// 戻り値は (元のパス, 作られたパス) のペア。
fn transfer(
  paths: &[String],
  dest: &str,
  move_files: bool,
  cancel: &std::sync::atomic::AtomicBool,
  on_progress: &mut dyn FnMut(&Progress, &str),
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
  use std::sync::atomic::Ordering;

  let dest_dir = Path::new(dest);
  if !dest_dir.is_dir() {
    return Err(format!("{dest} はディレクトリではありません"));
  }

  let mut progress = Progress { bytes_done: 0, files_done: 0 };
  let mut done = Vec::new();

  for p in paths {
    if cancel.load(Ordering::Relaxed) {
      break;
    }

    let src = PathBuf::from(p);
    let Some(name) = src.file_name() else {
      return Err(format!("{p} のファイル名を取得できません"));
    };

    // 自分自身の中へ入れようとした場合は何もしない（無限再帰やデータ消失を避ける）。
    if src.parent() == Some(dest_dir) {
      continue;
    }
    if src.is_dir() && dest_dir.starts_with(&src) {
      return Err(format!("{p} を自身の下へは移動できません"));
    }

    let target = unique_target(dest_dir, Path::new(name));

    // 同一ボリューム内の移動は、ファイルでもフォルダでも rename 1回で済む。
    // 以前はファイルしか試しておらず、数GBのフォルダを同じドライブ内で動かすだけで
    // 全コピー＋全削除になっていた。跨ぐと失敗するので下のコピー+削除に落とす。
    // 総量の集計（scan_total）もこの経路の項目は数えないので、進捗には加えない。
    if move_files && std::fs::rename(&src, &target).is_ok() {
      on_progress(&progress, &src.to_string_lossy());
      done.push((src, target));
      continue;
    }

    if src.is_dir() {
      // ディレクトリの再帰コピーは std に無いので自前。
      let completed = copy_dir_all(&src, &target, cancel, &mut progress, on_progress)
        .map_err(|e| format!("{p}: {e}"))?;
      if !completed {
        // 途中まで作ったフォルダは、揃っているように見えて中身が欠けている。
        // target は unique_target で新しく作った場所なので、丸ごと消してよい。
        let _ = std::fs::remove_dir_all(&target);
        break;
      }
      if move_files {
        std::fs::remove_dir_all(&src).map_err(|e| format!("{p} の削除に失敗: {e}"))?;
      }
    } else {
      let completed = copy_file(&src, &target, cancel, &mut progress, on_progress)
        .map_err(|e| format!("{p}: {e}"))?;
      if !completed {
        break;
      }
      if move_files {
        std::fs::remove_file(&src).map_err(|e| format!("{p} の削除に失敗: {e}"))?;
      }
    }

    // 中断された項目はここへ来ない。トレイや undo は「実際に終わったもの」だけを受け取る。
    done.push((src, target));
  }
  Ok(done)
}

/// 1ファイルをコピーする。
///
/// `std::fs::copy` を使わないのは、途中経過が取れず中断もできないため。
/// 数GBのファイルで固まって見えるのを避ける。
///
/// 戻り値は最後まで書けたか。中断時は書きかけを消して `Ok(false)` を返す
/// （`Ok(())` だと呼び出し側が完了と区別できず、トレイから項目が消えていた）。
fn copy_file(
  src: &Path,
  dst: &Path,
  cancel: &std::sync::atomic::AtomicBool,
  progress: &mut Progress,
  on_progress: &mut dyn FnMut(&Progress, &str),
) -> std::io::Result<bool> {
  use std::io::{Read, Write};
  use std::sync::atomic::Ordering;

  const CHUNK: usize = 1024 * 1024;

  let mut reader = std::fs::File::open(src)?;
  let mut writer = std::fs::File::create(dst)?;
  let mut buf = vec![0u8; CHUNK];
  let label = src.to_string_lossy().to_string();

  loop {
    if cancel.load(Ordering::Relaxed) {
      // 中途半端なファイルを残すと、見た目は揃っているのに中身が壊れている、
      // という最悪の状態になる。消してから抜ける。
      drop(writer);
      let _ = std::fs::remove_file(dst);
      return Ok(false);
    }

    let n = reader.read(&mut buf)?;
    if n == 0 {
      break;
    }
    writer.write_all(&buf[..n])?;
    progress.bytes_done += n as u64;
    on_progress(progress, &label);
  }

  writer.flush()?;
  // 自前コピーは更新日時を引き継がない（std::fs::copy / CopyFileEx は引き継ぐ）。
  // 「さっきいじったやつ」を日時で探すファイラなので、コピーで日時が今に化けるのは困る。
  if let Ok(modified) = reader.metadata().and_then(|meta| meta.modified()) {
    let _ = writer.set_modified(modified);
  }
  progress.files_done += 1;
  on_progress(progress, &label);
  Ok(true)
}

/// 転送前に総量を数える。進捗の分母を出すために要る。
///
/// この走査自体が大きな木では時間を食うので、呼び出し側は
/// 「集計中」を見せてから始める。
fn scan_total(paths: &[String]) -> (u64, u64) {
  // リンクは辿らない。ジャンクションが親を指していると無限に潜り、
  // 別の場所を指していると転送しない量まで分母に入る。
  // copy_dir_all も DirEntry::file_type（辿らない）で判定しているので揃える。
  fn walk(p: &Path, files: &mut u64, bytes: &mut u64) {
    let Ok(meta) = std::fs::symlink_metadata(p) else { return };
    if meta.is_dir() {
      let Ok(read) = std::fs::read_dir(p) else { return };
      for e in read.flatten() {
        walk(&e.path(), files, bytes);
      }
    } else {
      *files += 1;
      *bytes += meta.len();
    }
  }

  let mut files = 0;
  let mut bytes = 0;
  for p in paths {
    walk(Path::new(p), &mut files, &mut bytes);
  }
  (files, bytes)
}

/// 衝突したら `name (2).ext`, `name (3).ext` … と空きを探す。
fn unique_target(dir: &Path, name: &Path) -> PathBuf {
  let candidate = dir.join(name);
  if !candidate.exists() {
    return candidate;
  }

  let stem = name.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
  let ext = name.extension().map(|s| s.to_string_lossy().to_string());

  for n in 2..10_000 {
    let filename = match &ext {
      Some(e) => format!("{stem} ({n}).{e}"),
      None => format!("{stem} ({n})"),
    };
    let candidate = dir.join(filename);
    if !candidate.exists() {
      return candidate;
    }
  }
  // 現実には到達しない。到達したら上書きせずエラーにしたいが、
  // 戻り値の型上ここでは元の名前を返し、呼び出し側の copy が失敗する。
  dir.join(name)
}

fn copy_dir_all(
  src: &Path,
  dst: &Path,
  cancel: &std::sync::atomic::AtomicBool,
  progress: &mut Progress,
  on_progress: &mut dyn FnMut(&Progress, &str),
) -> std::io::Result<bool> {
  use std::sync::atomic::Ordering;

  std::fs::create_dir_all(dst)?;
  for entry in std::fs::read_dir(src)? {
    if cancel.load(Ordering::Relaxed) {
      return Ok(false);
    }
    let entry = entry?;
    let target = dst.join(entry.file_name());
    let completed = if entry.file_type()?.is_dir() {
      copy_dir_all(&entry.path(), &target, cancel, progress, on_progress)?
    } else {
      copy_file(&entry.path(), &target, cancel, progress, on_progress)?
    };
    if !completed {
      return Ok(false);
    }
  }
  Ok(true)
}

/// コピー / 切り取りで保持している内容。
///
/// **アプリ側に持つ**のが肝心。窓が複数あるのがこのファイラの前提なので、
/// 窓Aで切り取って窓Bで貼る、が成立しないと使い物にならない。
#[derive(Default)]
pub struct Clipboard {
  inner: std::sync::Mutex<ClipboardData>,
}

#[derive(Default, Clone, Serialize)]
pub struct ClipboardData {
  pub paths: Vec<String>,
  /// true なら切り取り（貼り付け時に元を消す）。
  pub cut: bool,
}

#[tauri::command]
pub fn set_clipboard(app: tauri::AppHandle, paths: Vec<String>, cut: bool) {
  use tauri::Manager;
  *app.state::<Clipboard>().inner.lock().unwrap() = ClipboardData { paths, cut };
}

#[tauri::command]
pub fn get_clipboard(app: tauri::AppHandle) -> ClipboardData {
  use tauri::Manager;
  app.state::<Clipboard>().inner.lock().unwrap().clone()
}

/// クリップボードの内容を `dest` へ貼り付ける。戻り値は作られたパス。
#[tauri::command]
pub fn paste_clipboard(app: tauri::AppHandle, dest: String) -> Result<Vec<String>, String> {
  use tauri::Manager;
  let data = app.state::<Clipboard>().inner.lock().unwrap().clone();
  if data.paths.is_empty() {
    return Ok(Vec::new());
  }

  let created = accept_dropped(data.paths, dest, data.cut)?;

  // 切り取りは一度しか貼れない。残すと二度目で「元が無い」エラーになる。
  if data.cut {
    *app.state::<Clipboard>().inner.lock().unwrap() = ClipboardData::default();
  }
  Ok(created)
}

/// 新しいフォルダを作る。名前が衝突したら退避名にする。戻り値は実際に作られたパス。
#[tauri::command]
pub fn create_folder(app: tauri::AppHandle, parent: String, name: String) -> Result<String, String> {
  use tauri::Manager;

  let target = create_folder_impl(&parent, &name)?;
  app.state::<crate::undo::UndoStack>().push(crate::undo::UndoAction::CreateFolder {
    path: target.clone(),
  });
  Ok(target.to_string_lossy().to_string())
}

fn create_folder_impl(parent: &str, name: &str) -> Result<PathBuf, String> {
  let parent_dir = Path::new(parent);
  if !parent_dir.is_dir() {
    return Err(format!("{parent} はディレクトリではありません"));
  }
  validate_name(name)?;

  let target = unique_target(parent_dir, Path::new(name));
  std::fs::create_dir(&target).map_err(|e| format!("作成に失敗: {e}"))?;
  Ok(target)
}

/// 名前を変える。戻り値は変更後のパス。
#[tauri::command]
pub fn rename_entry(app: tauri::AppHandle, path: String, new_name: String) -> Result<String, String> {
  use tauri::Manager;

  let target = rename_entry_impl(&path, &new_name)?;
  app.state::<crate::undo::UndoStack>().push(crate::undo::UndoAction::Rename {
    from: PathBuf::from(&path),
    to: target.clone(),
  });
  Ok(target.to_string_lossy().to_string())
}

fn rename_entry_impl(path: &str, new_name: &str) -> Result<PathBuf, String> {
  let src = Path::new(path);
  validate_name(new_name)?;

  let parent = src.parent().ok_or("親ディレクトリがありません")?;
  let target = parent.join(new_name);

  // Windows は大文字小文字を区別しないので、`name.txt` → `NAME.txt` のような
  // 改名でも target.exists() が真になる。パス文字列の比較では自分自身を
  // 「既に存在する」と誤判定するため、同じ実体を指しているかで判断する。
  if target.exists() && !is_same_entry(src, &target) {
    return Err(format!("{new_name} は既に存在します"));
  }
  std::fs::rename(src, &target).map_err(|e| format!("名前を変えられません: {e}"))?;
  Ok(target)
}

/// ゴミ箱へ送る。
///
/// **完全削除は提供しない。** ファイラの誤操作で戻せなくなるのが一番怖いので、
/// OS のゴミ箱に委ねて取り消せる状態を保つ。
#[tauri::command]
pub fn trash_entries(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
  use tauri::Manager;

  let n = trash_entries_impl(&paths)?;

  // 削除直後の一覧から、今送ったものを拾って undo 用に持っておく。
  // original_path で突き合わせる。
  if let Ok(all) = trash::os_limited::list() {
    let wanted: std::collections::HashSet<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let mut items: Vec<trash::TrashItem> =
      all.into_iter().filter(|i| wanted.contains(&i.original_path())).collect();
    // 同じ場所が複数回ゴミ箱に入っている場合、直近に消した時刻のものを選ぶ。
    items.sort_by_key(|i| std::cmp::Reverse(i.time_deleted));
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.original_path()));

    if !items.is_empty() {
      app.state::<crate::undo::UndoStack>().push(crate::undo::UndoAction::Trash { items });
    }
  }

  Ok(n)
}

fn trash_entries_impl(paths: &[String]) -> Result<usize, String> {
  if paths.is_empty() {
    return Ok(0);
  }
  trash::delete_all(paths).map_err(|e| format!("ゴミ箱へ送れません: {e}"))?;
  Ok(paths.len())
}

/// プレビューできる種類。フロントが表示方法を切り替えるための分類。
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Preview {
  /// 先頭 `max_bytes` までのテキスト。`truncated` はファイルがそれより大きかったか。
  Text { text: String, truncated: bool },
  /// フロント側で `convertFileSrc` を使い `<img>` に直接読ませる。
  Image,
  /// サイズが大きすぎる、拡張子が対象外、等でプレビューを諦めた場合。
  Unsupported { reason: String },
}

const PREVIEW_TEXT_EXT: &[&str] = &[
  "txt", "md", "markdown", "json", "toml", "yaml", "yml", "xml", "html", "htm", "css", "js",
  "ts", "jsx", "tsx", "svelte", "vue", "rs", "py", "go", "java", "c", "h", "cpp", "hpp", "cs",
  "sh", "ps1", "bat", "ini", "cfg", "conf", "log", "csv", "sql", "gitignore", "env",
];
const PREVIEW_IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg"];
/// これより大きいテキストは全部読まずに先頭だけにする。
const TEXT_PREVIEW_CAP: usize = 64 * 1024;
/// これより大きい画像はプレビューを諦める。巨大な画像を素で `<img>` に渡すと
/// デコードで固まる体感になるため、ファイラ側で線を引く。
const IMAGE_PREVIEW_CAP: u64 = 20 * 1024 * 1024;

/// ファイルの中身をどう見せられるか判定する。
///
/// 実際の画像デコードはフロント側（`<img>` + `convertFileSrc`）に任せ、
/// ここでは「見せてよいか」の判断とテキストの読み取りだけを行う。
/// Rust 側で画像デコードライブラリを持ち込むと依存が重くなるため避けた。
#[tauri::command]
pub fn preview_entry(path: String) -> Preview {
  let p = Path::new(&path);
  let ext = p
    .extension()
    .map(|e| e.to_string_lossy().to_lowercase())
    .unwrap_or_default();

  let Ok(meta) = std::fs::metadata(p) else {
    return Preview::Unsupported { reason: "読み取れません".into() };
  };
  if meta.is_dir() {
    return Preview::Unsupported { reason: "フォルダです".into() };
  }

  if PREVIEW_IMAGE_EXT.contains(&ext.as_str()) {
    if meta.len() > IMAGE_PREVIEW_CAP {
      return Preview::Unsupported { reason: "画像が大きすぎます".into() };
    }
    return Preview::Image;
  }

  if PREVIEW_TEXT_EXT.contains(&ext.as_str()) || ext.is_empty() {
    return match std::fs::read(p) {
      Ok(bytes) => {
        // NUL バイトを含んでいたらテキストとして解釈させない。
        // バイナリを無理に文字列化すると表示が壊れるだけでなく、
        // 巨大バイナリを丸ごと読む羽目にもなる。
        let probe = &bytes[..bytes.len().min(8192)];
        if probe.contains(&0) {
          return Preview::Unsupported { reason: "バイナリファイルです".into() };
        }
        let truncated = bytes.len() > TEXT_PREVIEW_CAP;
        let slice = &bytes[..bytes.len().min(TEXT_PREVIEW_CAP)];
        Preview::Text { text: String::from_utf8_lossy(slice).to_string(), truncated }
      }
      Err(e) => Preview::Unsupported { reason: format!("読めません: {e}") },
    };
  }

  Preview::Unsupported { reason: "対応していない種類です".into() }
}

/// アドレスバーに打たれた途中のパスから候補を出す。
///
/// 入力を「確定している親」と「打ちかけの断片」に割り、
/// 親の直下から断片で始まるものを返す。
#[tauri::command]
pub fn complete_path(input: String, show_hidden: Option<bool>) -> Vec<String> {
  let show_hidden = show_hidden.unwrap_or(false);
  let (parent, fragment) = split_for_completion(&input);

  let Ok(read) = std::fs::read_dir(&parent) else {
    return Vec::new();
  };

  let needle = fragment.to_lowercase();
  let mut hits: Vec<String> = read
    .flatten()
    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
    .filter(|e| {
      let name = e.file_name().to_string_lossy().to_string();
      if !show_hidden && is_hidden(&name, e.metadata().ok().as_ref()) {
        return false;
      }
      name.to_lowercase().starts_with(&needle)
    })
    .map(|e| e.path().to_string_lossy().to_string())
    .collect();

  hits.sort_by_key(|p| p.to_lowercase());
  hits.truncate(20); // 出しすぎると選ぶのが手間になる
  hits
}

/// 入力を「読みに行く親」と「前方一致に使う断片」に割る。
///
/// 末尾が区切り文字なら、そのディレクトリの中身を全部出す（断片は空）。
fn split_for_completion(input: &str) -> (String, String) {
  match input.rfind(['\\', '/']) {
    // 区切りが先頭付近（`C:\` など）の場合、親は区切りまで含める必要がある。
    Some(i) => (input[..=i].to_string(), input[i + 1..].to_string()),
    None => (input.to_string(), String::new()),
  }
}

/// 2つのパスが同じ実体を指しているか。
/// 正規化して比べることで、大文字小文字の違いや `.` を含む表記の揺れを吸収する。
fn is_same_entry(a: &Path, b: &Path) -> bool {
  match (a.canonicalize(), b.canonicalize()) {
    (Ok(a), Ok(b)) => a == b,
    _ => false,
  }
}

/// ファイル名として使えるか検査する。
///
/// 空や区切り文字入りをそのまま渡すと、意図しない場所に作られたり
/// 分かりにくいOSエラーになる。ここで弾いて理由を返す。
fn validate_name(name: &str) -> Result<(), String> {
  let trimmed = name.trim();
  if trimmed.is_empty() {
    return Err("名前が空です".into());
  }
  if trimmed == "." || trimmed == ".." {
    return Err("その名前は使えません".into());
  }
  if let Some(bad) = trimmed.chars().find(|c| r#"\/:*?"<>|"#.contains(*c)) {
    return Err(format!("名前に {bad} は使えません"));
  }
  Ok(())
}

/// Windows の `canonicalize` は `\\?\C:\...` を返す。
/// この形式は他アプリに渡すと解釈されないことがあるので、表示にもドラッグにも使えるよう剥がす。
pub(crate) fn strip_unc(p: &Path) -> String {
  let s = p.to_string_lossy().to_string();
  s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn strips_windows_unc_prefix() {
    assert_eq!(strip_unc(Path::new(r"\\?\C:\tmp")), r"C:\tmp");
  }

  #[test]
  fn leaves_plain_path_untouched() {
    assert_eq!(strip_unc(Path::new(r"C:\tmp")), r"C:\tmp");
  }

  #[test]
  fn lists_dirs_before_files_then_alphabetically() {
    let tmp = std::env::temp_dir().join("filer_list_dir_test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("b_dir")).unwrap();
    std::fs::create_dir_all(tmp.join("a_dir")).unwrap();
    std::fs::write(tmp.join("z_file.txt"), b"x").unwrap();
    std::fs::write(tmp.join("a_file.txt"), b"x").unwrap();

    let listing = list_dir(tmp.to_string_lossy().to_string(), None).unwrap();
    let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["a_dir", "b_dir", "a_file.txt", "z_file.txt"]);

    // canonicalize 後も UNC 前置きが残っていないこと。
    assert!(!listing.path.starts_with(r"\\?\"));

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn missing_dir_is_an_error_not_a_panic() {
    assert!(list_dir("Z:\\definitely\\not\\here".into(), None).is_err());
  }

  /// 各テストが専用の一時ディレクトリを持つようにする。
  fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("filer_test_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
  }

  fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
  }

  #[test]
  fn copies_a_file_without_touching_the_source() {
    let root = scratch("copy");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("a.txt"), b"hello").unwrap();

    accept_dropped(vec![s(&from.join("a.txt"))], s(&to), false).unwrap();

    assert!(from.join("a.txt").exists(), "コピーなので元が残るはず");
    assert_eq!(std::fs::read(to.join("a.txt")).unwrap(), b"hello");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn moves_a_file_and_removes_the_source() {
    let root = scratch("move");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("a.txt"), b"hello").unwrap();

    accept_dropped(vec![s(&from.join("a.txt"))], s(&to), true).unwrap();

    assert!(!from.join("a.txt").exists(), "移動なので元は消えるはず");
    assert_eq!(std::fs::read(to.join("a.txt")).unwrap(), b"hello");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// ファイラで既存ファイルが黙って上書きされるのが一番怖い。
  #[test]
  fn never_silently_overwrites_an_existing_file() {
    let root = scratch("collide");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("a.txt"), b"new").unwrap();
    std::fs::write(to.join("a.txt"), b"original").unwrap();

    accept_dropped(vec![s(&from.join("a.txt"))], s(&to), false).unwrap();

    assert_eq!(
      std::fs::read(to.join("a.txt")).unwrap(),
      b"original",
      "既存ファイルは残っていなければならない"
    );
    assert_eq!(std::fs::read(to.join("a (2).txt")).unwrap(), b"new");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn copying_preserves_the_modified_time() {
    let root = scratch("copy_mtime");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(from.join("d")).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
    for file in [from.join("a.txt"), from.join("d/b.txt")] {
      std::fs::write(&file, b"x").unwrap();
      std::fs::File::options().write(true).open(&file).unwrap().set_modified(old).unwrap();
    }

    accept_dropped(vec![s(&from.join("a.txt")), s(&from.join("d"))], s(&to), false).unwrap();

    for file in [to.join("a.txt"), to.join("d/b.txt")] {
      assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), old, "{} の日時が変わった", file.display());
    }
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn copies_directories_recursively() {
    let root = scratch("recurse");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(from.join("d/nested")).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("d/nested/deep.txt"), b"deep").unwrap();

    accept_dropped(vec![s(&from.join("d"))], s(&to), false).unwrap();

    assert_eq!(std::fs::read(to.join("d/nested/deep.txt")).unwrap(), b"deep");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 自分の子孫へ移そうとすると、再帰コピーが自分を食い続けて破滅する。
  #[test]
  fn refuses_to_move_a_directory_into_itself() {
    let root = scratch("selfmove");
    let outer = root.join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).unwrap();

    assert!(accept_dropped(vec![s(&outer)], s(&inner), true).is_err());
    assert!(outer.exists(), "拒否したので元は無傷であるべき");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 同じ場所に落とした時に自分自身を複製しない。
  #[test]
  fn dropping_into_the_same_directory_is_a_no_op() {
    let root = scratch("samedir");
    std::fs::write(root.join("a.txt"), b"x").unwrap();

    let done = accept_dropped(vec![s(&root.join("a.txt"))], s(&root), false).unwrap();

    assert!(done.is_empty());
    assert!(!root.join("a (2).txt").exists());
    let _ = std::fs::remove_dir_all(&root);
  }

  fn entry(name: &str, is_dir: bool, size: u64, modified: u128) -> Entry {
    Entry {
      name: name.into(),
      is_dir,
      size,
      modified,
      ext: Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default(),
      hidden: name.starts_with('.'),
    }
  }

  fn names(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
  }

  fn spec(key: SortKey, descending: bool) -> SortSpec {
    SortSpec { key, descending, dirs_first: true, show_hidden: false }
  }

  #[test]
  fn sorts_by_modified_newest_last_then_reversed() {
    let mut v = vec![
      entry("old.txt", false, 1, 100),
      entry("new.txt", false, 1, 300),
      entry("mid.txt", false, 1, 200),
    ];
    sort_entries(&mut v, spec(SortKey::Modified, false));
    assert_eq!(names(&v), vec!["old.txt", "mid.txt", "new.txt"]);

    sort_entries(&mut v, spec(SortKey::Modified, true));
    assert_eq!(names(&v), vec!["new.txt", "mid.txt", "old.txt"], "「さっきいじったやつ」を先頭に");
  }

  #[test]
  fn sorts_by_size() {
    let mut v = vec![
      entry("big.bin", false, 900, 0),
      entry("small.bin", false, 10, 0),
    ];
    sort_entries(&mut v, spec(SortKey::Size, true));
    assert_eq!(names(&v), vec!["big.bin", "small.bin"]);
  }

  #[test]
  fn directories_stay_first_regardless_of_key() {
    let mut v = vec![
      entry("huge.bin", false, 9_000, 999),
      entry("adir", true, 0, 1),
    ];
    for key in [SortKey::Name, SortKey::Size, SortKey::Modified, SortKey::Ext] {
      for desc in [false, true] {
        sort_entries(&mut v, spec(key, desc));
        assert!(v[0].is_dir, "key={:?} desc={desc} でディレクトリが先頭でない", key as u8);
      }
    }
  }

  #[test]
  fn dirs_first_can_be_turned_off() {
    let mut v = vec![entry("zdir", true, 0, 0), entry("afile.txt", false, 0, 0)];
    sort_entries(
      &mut v,
      SortSpec { key: SortKey::Name, descending: false, dirs_first: false, show_hidden: false },
    );
    assert_eq!(names(&v), vec!["afile.txt", "zdir"]);
  }

  /// 同値が並んだ時に順序がぶれると、更新のたびに行が入れ替わって目で追えなくなる。
  #[test]
  fn equal_keys_fall_back_to_name_for_a_stable_order() {
    let mut v = vec![
      entry("c.txt", false, 5, 42),
      entry("a.txt", false, 5, 42),
      entry("b.txt", false, 5, 42),
    ];
    sort_entries(&mut v, spec(SortKey::Size, false));
    assert_eq!(names(&v), vec!["a.txt", "b.txt", "c.txt"]);
  }

  #[test]
  fn descending_name_sort_reverses() {
    let mut v = vec![entry("a.txt", false, 0, 0), entry("b.txt", false, 0, 0)];
    sort_entries(&mut v, spec(SortKey::Name, true));
    assert_eq!(names(&v), vec!["b.txt", "a.txt"]);
  }

  #[test]
  fn hidden_files_are_excluded_unless_asked() {
    let root = scratch("hidden");
    std::fs::write(root.join("visible.txt"), b"x").unwrap();
    std::fs::write(root.join(".secret"), b"x").unwrap();

    let shown = list_dir(s(&root), None).unwrap();
    assert_eq!(names(&shown.entries), vec!["visible.txt"]);

    let all = list_dir(
      s(&root),
      Some(SortSpec { key: SortKey::Name, descending: false, dirs_first: true, show_hidden: true }),
    )
    .unwrap();
    assert_eq!(all.entries.len(), 2);
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn entries_carry_size_and_extension() {
    let root = scratch("meta");
    std::fs::write(root.join("data.TXT"), b"hello").unwrap();

    let listing = list_dir(s(&root), None).unwrap();
    let e = &listing.entries[0];
    assert_eq!(e.size, 5);
    assert_eq!(e.ext, "txt", "拡張子は小文字に揃える");
    assert!(e.modified > 0, "更新時刻が取れていること");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 実測用。`cargo test -- --ignored measure_list_dir --nocapture` で走らせる。
  /// ストリーミング化が必要かを、憶測でなく数字で判断するため。
  #[test]
  #[ignore]
  fn measure_list_dir() {
    for dir in [
      r"C:\Windows\WinSxS",
      r"C:\Windows\System32",
      r"C:\Users\hinat\AppData\Local\Temp",
    ] {
      if !Path::new(dir).is_dir() {
        continue;
      }
      let t = std::time::Instant::now();
      match list_dir(dir.to_string(), None) {
        Ok(l) => println!("{dir}: {} 件 / {:?}", l.entries.len(), t.elapsed()),
        Err(e) => println!("{dir}: 失敗 {e}"),
      }
    }
  }

  #[test]
  fn scan_total_counts_files_and_bytes_recursively() {
    let root = scratch("scan");
    std::fs::create_dir_all(root.join("d/nested")).unwrap();
    std::fs::write(root.join("a.bin"), vec![0u8; 100]).unwrap();
    std::fs::write(root.join("d/b.bin"), vec![0u8; 250]).unwrap();
    std::fs::write(root.join("d/nested/c.bin"), vec![0u8; 7]).unwrap();

    let (files, bytes) = scan_total(&[s(&root)]);
    assert_eq!(files, 3, "ディレクトリは数えずファイルだけ");
    assert_eq!(bytes, 357);
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn progress_reports_every_file() {
    let root = scratch("progress");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("a.bin"), vec![0u8; 10]).unwrap();
    std::fs::write(from.join("b.bin"), vec![0u8; 20]).unwrap();

    let never = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<(u64, u64)> = Vec::new();
    transfer(
      &[s(&from.join("a.bin")), s(&from.join("b.bin"))],
      &s(&to),
      false,
      &never,
      &mut |p, _| seen.push((p.files_done, p.bytes_done)),
    )
    .unwrap();

    let last = seen.last().copied().unwrap();
    assert_eq!(last, (2, 30), "最後は全件・全バイトに到達する");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 中断した時に中途半端なファイルが残ると、見た目は揃っているのに
  /// 中身が壊れている、という最悪の状態になる。
  #[test]
  fn cancelling_leaves_no_partial_file() {
    let root = scratch("cancel");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    // 1MB チャンクを複数回またぐ大きさにして、途中で止まる余地を作る。
    std::fs::write(from.join("big.bin"), vec![7u8; 5 * 1024 * 1024]).unwrap();

    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut chunks = 0;
    transfer(&[s(&from.join("big.bin"))], &s(&to), false, &cancel, &mut |_, _| {
      chunks += 1;
      // 最初のチャンクを書いた直後に中断させる。
      if chunks == 1 {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
      }
    })
    .unwrap();

    assert!(!to.join("big.bin").exists(), "書きかけは消されているべき");
    assert!(from.join("big.bin").exists(), "元は無傷であるべき");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// undo が元と先を突き合わせられるよう、ペアで返ること。
  #[test]
  fn transfer_returns_source_and_destination_pairs() {
    let root = scratch("pairs");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("a.txt"), b"x").unwrap();

    let never = std::sync::atomic::AtomicBool::new(false);
    let pairs = transfer(&[s(&from.join("a.txt"))], &s(&to), false, &never, &mut |_, _| {}).unwrap();

    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0, from.join("a.txt"));
    assert_eq!(pairs[0].1, to.join("a.txt"));
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 中断しても移動元を消してはいけない。消すとデータが消滅する。
  #[test]
  fn cancelling_a_move_keeps_the_source() {
    let root = scratch("cancel_move");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("big.bin"), vec![7u8; 5 * 1024 * 1024]).unwrap();

    let cancel = std::sync::atomic::AtomicBool::new(false);
    cancel.store(true, std::sync::atomic::Ordering::Relaxed); // 最初から中断状態

    transfer(&[s(&from.join("big.bin"))], &s(&to), true, &cancel, &mut |_, _| {}).unwrap();

    assert!(from.join("big.bin").exists(), "移動でも中断なら元は残る");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 中断された項目を完了ペアとして返すと、トレイから実際には動いていない項目が消え、
  /// undo にも存在しない転送が積まれる。
  #[test]
  fn cancelled_items_are_not_reported_as_completed() {
    let root = scratch("cancel_report");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("small.txt"), b"x").unwrap();
    std::fs::write(from.join("big.bin"), vec![7u8; 5 * 1024 * 1024]).unwrap();

    let cancel = std::sync::atomic::AtomicBool::new(false);
    let pairs = transfer(
      &[s(&from.join("small.txt")), s(&from.join("big.bin"))],
      &s(&to),
      // 移動だと同一ボリュームでは rename で一瞬に終わり、中断の余地が無い。
      false,
      &cancel,
      &mut |p, current| {
        if current.ends_with("big.bin") && p.files_done == 1 {
          cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
      },
    )
    .unwrap();

    assert_eq!(pairs.len(), 1, "完了したのは small.txt だけ");
    assert_eq!(pairs[0].0, from.join("small.txt"));
    assert!(!to.join("big.bin").exists(), "書きかけは残さない");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 同じドライブ内のフォルダ移動は rename で済ませる。
  /// コピーしていればファイルの実体（ファイルID）が変わるので、それで見分ける。
  #[cfg(windows)]
  #[test]
  fn moving_a_directory_on_the_same_volume_renames_instead_of_copying() {
    let root = scratch("move_dir_rename");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(from.join("d/nested")).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("d/nested/a.txt"), b"x").unwrap();
    let before = std::fs::metadata(from.join("d/nested/a.txt")).unwrap().modified().unwrap();

    let never = std::sync::atomic::AtomicBool::new(false);
    let mut bytes_seen = 0;
    let pairs = transfer(&[s(&from.join("d"))], &s(&to), true, &never, &mut |p, _| bytes_seen = p.bytes_done).unwrap();

    assert_eq!(pairs.len(), 1);
    assert!(!from.join("d").exists());
    assert_eq!(std::fs::read(to.join("d/nested/a.txt")).unwrap(), b"x");
    assert_eq!(bytes_seen, 0, "中身を1バイトも読み書きしていない");
    assert_eq!(std::fs::metadata(to.join("d/nested/a.txt")).unwrap().modified().unwrap(), before);
    let _ = std::fs::remove_dir_all(&root);
  }

  #[cfg(windows)]
  #[test]
  fn same_volume_compares_drive_prefixes_case_insensitively() {
    assert!(same_volume(Path::new(r"C:\a\b"), Path::new(r"c:\x")));
    assert!(!same_volume(Path::new(r"C:\a"), Path::new(r"D:\a")));
    assert!(!same_volume(Path::new(r"relative\a"), Path::new(r"C:\a")));
  }

  #[test]
  fn scan_total_skips_same_volume_moves() {
    let root = scratch("scan_skip_move");
    std::fs::write(root.join("a.bin"), vec![0u8; 100]).unwrap();
    let paths = [s(&root.join("a.bin"))];
    assert_eq!(scan_total_pub(&paths, &s(&root), false), (1, 100), "コピーは数える");
    #[cfg(windows)]
    assert_eq!(scan_total_pub(&paths, &s(&root), true), (0, 0), "同一ドライブの移動は数えない");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// フォルダを途中で止めたら、欠けたフォルダを転送先に残さない。
  #[test]
  fn cancelling_a_directory_copy_removes_the_partial_tree() {
    let root = scratch("cancel_dir");
    let (from, to) = (root.join("from"), root.join("to"));
    std::fs::create_dir_all(from.join("d")).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("d/big.bin"), vec![7u8; 5 * 1024 * 1024]).unwrap();

    let cancel = std::sync::atomic::AtomicBool::new(false);
    let pairs = transfer(&[s(&from.join("d"))], &s(&to), false, &cancel, &mut |_, _| {
      cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    })
    .unwrap();

    assert!(pairs.is_empty());
    assert!(!to.join("d").exists(), "書きかけのフォルダは消されているべき");
    let _ = std::fs::remove_dir_all(&root);
  }

  fn preview_kind(p: &Preview) -> &'static str {
    match p {
      Preview::Text { .. } => "text",
      Preview::Image => "image",
      Preview::Unsupported { .. } => "unsupported",
    }
  }

  #[test]
  fn preview_reads_small_text_file_in_full() {
    let root = scratch("preview_text");
    std::fs::write(root.join("a.txt"), "hello world").unwrap();

    let p = preview_entry(s(&root.join("a.txt")));
    assert_eq!(preview_kind(&p), "text");
    if let Preview::Text { text, truncated } = p {
      assert_eq!(text, "hello world");
      assert!(!truncated);
    }
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn preview_truncates_large_text_file() {
    let root = scratch("preview_big_text");
    std::fs::write(root.join("big.log"), vec![b'a'; TEXT_PREVIEW_CAP + 500]).unwrap();

    let p = preview_entry(s(&root.join("big.log")));
    if let Preview::Text { text, truncated } = p {
      assert_eq!(text.len(), TEXT_PREVIEW_CAP);
      assert!(truncated, "上限を超えたら truncated が立つはず");
    } else {
      panic!("text であるはず");
    }
    let _ = std::fs::remove_dir_all(&root);
  }

  /// バイナリを文字列化しようとして表示が壊れる、あるいは巨大バイナリを
  /// 丸ごと読んでしまう、を避ける。
  #[test]
  fn preview_refuses_binary_disguised_as_text_extension() {
    let root = scratch("preview_binary");
    let mut bytes = vec![1u8, 2, 3, 0, 4, 5]; // NUL を含む
    bytes.extend(vec![0u8; 100]);
    std::fs::write(root.join("data.log"), bytes).unwrap();

    let p = preview_entry(s(&root.join("data.log")));
    assert_eq!(preview_kind(&p), "unsupported");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn preview_recognizes_image_extensions_without_reading_bytes() {
    let root = scratch("preview_image");
    std::fs::write(root.join("photo.PNG"), b"not a real png but has right ext").unwrap();

    let p = preview_entry(s(&root.join("photo.PNG")));
    assert_eq!(preview_kind(&p), "image", "拡張子は大小文字を無視するはず");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn preview_unsupported_for_unknown_extension() {
    let root = scratch("preview_unknown");
    std::fs::write(root.join("archive.zip"), b"PK\x03\x04").unwrap();

    let p = preview_entry(s(&root.join("archive.zip")));
    assert_eq!(preview_kind(&p), "unsupported");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn preview_unsupported_for_a_directory() {
    let root = scratch("preview_dir");
    assert_eq!(preview_kind(&preview_entry(s(&root))), "unsupported");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn splits_input_for_completion() {
    // 途中まで打った状態: 親を読み、断片で絞る。
    assert_eq!(
      split_for_completion(r"C:\Users\hin"),
      (r"C:\Users\".to_string(), "hin".to_string())
    );
    // 区切りで終わっている: その中身を全部出す。
    assert_eq!(
      split_for_completion(r"C:\Users\"),
      (r"C:\Users\".to_string(), "".to_string())
    );
    // ドライブ直下。親に区切りを残さないと読めない。
    assert_eq!(split_for_completion(r"C:\"), (r"C:\".to_string(), "".to_string()));
    // 区切りが無い。
    assert_eq!(split_for_completion("hin"), ("hin".to_string(), "".to_string()));
    // スラッシュも扱える。
    assert_eq!(
      split_for_completion("/home/us"),
      ("/home/".to_string(), "us".to_string())
    );
  }

  #[test]
  fn completes_directories_by_prefix() {
    let root = scratch("complete");
    std::fs::create_dir_all(root.join("alpha")).unwrap();
    std::fs::create_dir_all(root.join("alpine")).unwrap();
    std::fs::create_dir_all(root.join("beta")).unwrap();
    std::fs::write(root.join("alpha.txt"), b"x").unwrap();

    let hits = complete_path(format!("{}\\alp", s(&root)), None);
    let names: Vec<String> = hits
      .iter()
      .map(|p| Path::new(p).file_name().unwrap().to_string_lossy().to_string())
      .collect();

    assert_eq!(names, vec!["alpha", "alpine"], "前方一致するディレクトリだけ");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn completion_of_a_missing_parent_is_empty_not_an_error() {
    assert!(complete_path("Z:\\nope\\x".into(), None).is_empty());
  }

  #[test]
  fn creates_a_folder_and_avoids_collisions() {
    let root = scratch("mkdir");
    let a = create_folder_impl(&s(&root), "new").unwrap();
    let b = create_folder_impl(&s(&root), "new").unwrap();

    assert!(a.is_dir());
    assert!(b.is_dir());
    assert_ne!(a, b, "衝突したら別名になるはず");
    assert!(b.ends_with("new (2)"));
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn rejects_names_that_would_escape_or_break() {
    let root = scratch("badname");
    for bad in ["", "   ", "..", "a/b", "a\\b", "a:b", "a*b", "a?b"] {
      assert!(
        create_folder_impl(&s(&root), bad).is_err(),
        "{bad:?} は拒否されるべき"
      );
    }
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn renames_a_file() {
    let root = scratch("rename");
    std::fs::write(root.join("before.txt"), b"x").unwrap();

    let after = rename_entry_impl(&s(&root.join("before.txt")), "after.txt").unwrap();

    assert!(!root.join("before.txt").exists());
    assert_eq!(std::fs::read(&after).unwrap(), b"x");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn rename_refuses_to_clobber_an_existing_file() {
    let root = scratch("rename_clash");
    std::fs::write(root.join("a.txt"), b"a").unwrap();
    std::fs::write(root.join("b.txt"), b"b").unwrap();

    assert!(rename_entry_impl(&s(&root.join("a.txt")), "b.txt").is_err());
    assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), b"b", "既存が残ること");
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 大文字小文字だけの変更は Windows で「既に存在する」と誤判定されやすい。
  #[test]
  fn rename_allows_case_only_change() {
    let root = scratch("rename_case");
    std::fs::write(root.join("name.txt"), b"x").unwrap();
    assert!(rename_entry_impl(&s(&root.join("name.txt")), "NAME.txt").is_ok());
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn trashing_nothing_is_not_an_error() {
    assert_eq!(trash_entries_impl(&[]).unwrap(), 0);
  }

  #[test]
  fn lists_only_subdirectories_sorted_by_name() {
    let root = scratch("subdirs");
    std::fs::create_dir_all(root.join("zeta")).unwrap();
    std::fs::create_dir_all(root.join("Alpha")).unwrap();
    std::fs::write(root.join("a_file.txt"), b"x").unwrap();

    let dirs = list_subdirs(s(&root), None).unwrap();
    let names: Vec<&str> = dirs.iter().map(|e| e.name.as_str()).collect();

    assert_eq!(names, vec!["Alpha", "zeta"], "ファイルは含めず、大小文字を無視して名前順");
    assert!(dirs.iter().all(|e| e.is_dir));
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 一覧とツリーで見えるものが食い違うと混乱する。
  #[test]
  fn subdirs_follow_the_same_hidden_setting_as_the_listing() {
    let root = scratch("subdirs_hidden");
    std::fs::create_dir_all(root.join("visible")).unwrap();
    std::fs::create_dir_all(root.join(".hidden")).unwrap();

    let shown = list_subdirs(s(&root), None).unwrap();
    assert_eq!(shown.len(), 1, "既定では隠しフォルダを出さない");

    let all = list_subdirs(s(&root), Some(true)).unwrap();
    assert_eq!(all.len(), 2);
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn subdirs_of_a_missing_dir_is_an_error_not_a_panic() {
    assert!(list_subdirs("Z:\\definitely\\not\\here".into(), None).is_err());
  }

  #[test]
  fn drives_includes_the_system_drive() {
    let found = drives();
    assert!(!found.is_empty(), "少なくとも1つはドライブがあるはず");
    assert!(found.iter().all(|d| Path::new(d).is_dir()));
  }

  #[test]
  fn rejects_a_destination_that_is_not_a_directory() {
    let root = scratch("baddest");
    std::fs::write(root.join("file.txt"), b"x").unwrap();
    assert!(accept_dropped(vec![], s(&root.join("file.txt")), false).is_err());
    let _ = std::fs::remove_dir_all(&root);
  }
}
