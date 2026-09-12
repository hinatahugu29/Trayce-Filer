#![windows_subsystem = "windows"]

use chrono::{DateTime, Local};
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::SystemTime;

slint::include_modules!();

// HSL (色相: 0..360, 彩度: 0..1, 輝度: 0..1) から RGB を計算
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r_prime, g_prime, b_prime) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r_prime + m) * 255.0).round() as u8,
        ((g_prime + m) * 255.0).round() as u8,
        ((b_prime + m) * 255.0).round() as u8,
    )
}

// ★思想2: パスから安定した固有色相（カラーバンド）を作る
// 元の api.ts (pathHue) と完全に同一のアルゴリズム (h = (h * 31 + char) % 360)
fn path_hue_color(path: &str) -> Color {
    let mut h: u32 = 0;
    for b in path.bytes() {
        h = (h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
    }
    let (r, g, b) = hsl_to_rgb(h as f32, 0.75, 0.55);
    Color::from_rgb_u8(r, g, b)
}

// ★思想3: パスを「末尾のフォルダ名（大見出し）」と「その手前（文脈）」に割る
fn split_path(path: &str) -> (String, String) {
    let normalized = path.trim_end_matches(['\\', '/']);
    if let Some(idx) = normalized.rfind(['\\', '/']) {
        let lead = &normalized[..idx];
        let tail = &normalized[idx + 1..];
        (
            if tail.is_empty() { normalized.to_string() } else { tail.to_string() },
            lead.to_string(),
        )
    } else {
        (normalized.to_string(), String::new())
    }
}

// ファイルサイズを読みやすく整形 (B, KB, MB, GB)
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// システム日時を文字列に変換
fn format_time(time: SystemTime) -> String {
    let dt: DateTime<Local> = time.into();
    dt.format("%Y/%m/%d %H:%M").to_string()
}

// 拡張子に応じた絵文字アイコン
fn get_icon(is_dir: bool, name: &str) -> &'static str {
    if is_dir {
        return "📁";
    }
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" => "🖼️",
        "mp4" | "mkv" | "avi" | "mov" | "webm" => "🎬",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => "🎵",
        "zip" | "7z" | "rar" | "tar" | "gz" => "📦",
        "rs" | "ts" | "js" | "py" | "html" | "css" | "json" | "toml" | "svelte" => "💻",
        "txt" | "md" | "doc" | "docx" | "pdf" => "📄",
        "exe" | "msi" | "bat" | "cmd" | "ps1" => "⚙️",
        _ => "📄",
    }
}

// ディレクトリ読込（リアルタイムフィルタリング対応）
fn read_directory(path: &str, filter: &str) -> (Vec<FileItem>, Result<(), String>) {
    let p = Path::new(path);
    let read_res = fs::read_dir(p);

    let entries = match read_res {
        Ok(e) => e,
        Err(err) => return (Vec::new(), Err(err.to_string())),
    };

    let filter_lower = filter.trim().to_lowercase();
    let mut dirs_list = Vec::new();
    let mut files_list = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();

        if !filter_lower.is_empty() && !name.to_lowercase().contains(&filter_lower) {
            continue;
        }

        let metadata = entry.metadata().ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);

        let size_str = if is_dir {
            String::new()
        } else {
            metadata.as_ref().map(|m| format_size(m.len())).unwrap_or_default()
        };

        let date_str = metadata
            .and_then(|m| m.modified().ok())
            .map(format_time)
            .unwrap_or_default();

        let icon = get_icon(is_dir, &name);

        let item = FileItem {
            name: SharedString::from(name),
            is_dir,
            size: SharedString::from(size_str),
            date: SharedString::from(date_str),
            icon: SharedString::from(icon),
        };

        if is_dir {
            dirs_list.push(item);
        } else {
            files_list.push(item);
        }
    }

    dirs_list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files_list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    dirs_list.extend(files_list);
    (dirs_list, Ok(()))
}

// 利用可能ドライブの検出
fn detect_drives() -> Vec<SharedString> {
    let mut drives = Vec::new();
    for letter in b'A'..=b'Z' {
        let path = format!("{}:\\", letter as char);
        if Path::new(&path).exists() {
            drives.push(SharedString::from(path));
        }
    }
    if drives.is_empty() {
        drives.push(SharedString::from("C:\\"));
    }
    drives
}

// 新規フォルダー作成
fn create_new_folder(current_dir: &str) -> Result<String, String> {
    let base_name = "新しいフォルダー";
    let mut candidate = Path::new(current_dir).join(base_name);
    let mut idx = 1;

    while candidate.exists() {
        candidate = Path::new(current_dir).join(format!("{} ({})", base_name, idx));
        idx += 1;
    }

    fs::create_dir(&candidate).map_err(|e| e.to_string())?;
    Ok(candidate.file_name().unwrap().to_string_lossy().to_string())
}

// 安全なゴミ箱削除
fn trash_item(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

// 再帰的ディレクトリコピー
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// 重複回避されたユニークな貼り付け先パスの生成
fn unique_destination_path(dir: &Path, original_name: &str) -> PathBuf {
    let mut dest = dir.join(original_name);
    if !dest.exists() {
        return dest;
    }
    let p = Path::new(original_name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(original_name);
    let ext = p.extension().and_then(|e| e.to_str());

    let mut idx = 1;
    loop {
        let candidate_name = if let Some(e) = ext {
            format!("{} - コピー ({}) .{}", stem, idx, e)
        } else {
            format!("{} - コピー ({})", stem, idx)
        };
        dest = dir.join(candidate_name);
        if !dest.exists() {
            return dest;
        }
        idx += 1;
    }
}

// OS クリップボードへテキストを設定
fn set_clipboard_text(text: &str) {
    if let Ok(mut child) = Command::new("clip").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

// 内部クリップボードアイテム
#[derive(Clone, Debug)]
struct ClipboardItem {
    path: PathBuf,
    is_cut: bool,
}

// ペイン状態管理（戻る/進む履歴スタック内包）
struct PaneState {
    back_stack: Vec<String>,
    forward_stack: Vec<String>,
}

impl PaneState {
    fn new() -> Self {
        Self {
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
        }
    }

    fn push_history(&mut self, current_path: &str) {
        if self.back_stack.last().map(|s| s.as_str()) != Some(current_path) {
            self.back_stack.push(current_path.to_string());
        }
        self.forward_stack.clear();
    }

    fn can_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    fn can_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }

    fn go_back(&mut self, current_path: &str) -> Option<String> {
        if let Some(target) = self.back_stack.pop() {
            self.forward_stack.push(current_path.to_string());
            Some(target)
        } else {
            None
        }
    }

    fn go_forward(&mut self, current_path: &str) -> Option<String> {
        if let Some(target) = self.forward_stack.pop() {
            self.back_stack.push(current_path.to_string());
            Some(target)
        } else {
            None
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start_time = SystemTime::now();
    let app = AppWindow::new()?;

    // ドライブ一覧
    let drives = detect_drives();
    app.set_drives(ModelRc::from(Rc::new(VecModel::from(drives))));

    // 初期パス
    let left_initial = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "C:\\".to_string());
    let right_initial = dirs::document_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| left_initial.clone());

    // 内部状態
    let left_state = Rc::new(RefCell::new(PaneState::new()));
    let right_state = Rc::new(RefCell::new(PaneState::new()));
    let recent_history: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(vec![
        left_initial.clone(),
        right_initial.clone(),
    ]));
    let internal_clipboard: Rc<RefCell<Option<ClipboardItem>>> = Rc::new(RefCell::new(None));

    // 最近の履歴を更新するヘルパー
    let add_to_recent_history = {
        let app_weak = app.as_weak();
        let recent_history = recent_history.clone();
        move |path: &str| {
            let mut hist = recent_history.borrow_mut();
            hist.retain(|p| p != path);
            hist.insert(0, path.to_string());
            if hist.len() > 30 {
                hist.truncate(30);
            }
            if let Some(app) = app_weak.upgrade() {
                let items: Vec<SharedString> = hist.iter().map(|s| SharedString::from(s.as_str())).collect();
                app.set_recent_history(ModelRc::from(Rc::new(VecModel::from(items))));
            }
        }
    };

    // 左ペイン更新ヘルパー
    let update_left_pane = {
        let app_weak = app.as_weak();
        let left_state = left_state.clone();
        move |path: &str, filter: &str| {
            if let Some(app) = app_weak.upgrade() {
                let (tail, lead) = split_path(path);
                let hue = path_hue_color(path);
                app.set_left_path(SharedString::from(path));
                app.set_left_tail(SharedString::from(tail));
                app.set_left_lead(SharedString::from(lead));
                app.set_left_hue_color(hue);

                let state = left_state.borrow();
                app.set_left_can_back(state.can_back());
                app.set_left_can_forward(state.can_forward());

                let (items, _) = read_directory(path, filter);
                app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
            }
        }
    };

    // 右ペイン更新ヘルパー
    let update_right_pane = {
        let app_weak = app.as_weak();
        let right_state = right_state.clone();
        move |path: &str, filter: &str| {
            if let Some(app) = app_weak.upgrade() {
                let (tail, lead) = split_path(path);
                let hue = path_hue_color(path);
                app.set_right_path(SharedString::from(path));
                app.set_right_tail(SharedString::from(tail));
                app.set_right_lead(SharedString::from(lead));
                app.set_right_hue_color(hue);

                let state = right_state.borrow();
                app.set_right_can_back(state.can_back());
                app.set_right_can_forward(state.can_forward());

                let (items, _) = read_directory(path, filter);
                app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
            }
        }
    };

    // 初期化実行
    update_left_pane(&left_initial, "");
    update_right_pane(&right_initial, "");
    {
        let hist = recent_history.borrow();
        let items: Vec<SharedString> = hist.iter().map(|s| SharedString::from(s.as_str())).collect();
        app.set_recent_history(ModelRc::from(Rc::new(VecModel::from(items))));
    }

    let elapsed = start_time.elapsed().unwrap_or_default();
    app.set_status_text(SharedString::from(format!(
        "⚡ GPUネイティブ描画エンジン稼働中 · 初期化: {:.1}ms",
        elapsed.as_secs_f64() * 1000.0
    )));

    // ドライブ選択
    {
        let app_weak = app.as_weak();
        let left_state = left_state.clone();
        let right_state = right_state.clone();
        let add_to_recent = add_to_recent_history.clone();
        app.on_drive_selected(move |drive| {
            if let Some(app) = app_weak.upgrade() {
                let drive_str = drive.to_string();
                add_to_recent(&drive_str);
                if app.get_active_pane() == 0 {
                    let cur = app.get_left_path().to_string();
                    left_state.borrow_mut().push_history(&cur);
                    let (tail, lead) = split_path(&drive_str);
                    let hue = path_hue_color(&drive_str);
                    app.set_left_path(SharedString::from(&drive_str));
                    app.set_left_tail(SharedString::from(tail));
                    app.set_left_lead(SharedString::from(lead));
                    app.set_left_hue_color(hue);
                    app.set_left_can_back(left_state.borrow().can_back());
                    app.set_left_can_forward(left_state.borrow().can_forward());
                    let (items, _) = read_directory(&drive_str, &app.get_left_filter());
                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_left_selected(-1);
                } else {
                    let cur = app.get_right_path().to_string();
                    right_state.borrow_mut().push_history(&cur);
                    let (tail, lead) = split_path(&drive_str);
                    let hue = path_hue_color(&drive_str);
                    app.set_right_path(SharedString::from(&drive_str));
                    app.set_right_tail(SharedString::from(tail));
                    app.set_right_lead(SharedString::from(lead));
                    app.set_right_hue_color(hue);
                    app.set_right_can_back(right_state.borrow().can_back());
                    app.set_right_can_forward(right_state.borrow().can_forward());
                    let (items, _) = read_directory(&drive_str, &app.get_right_filter());
                    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_right_selected(-1);
                }
            }
        });
    }

    // 2ペイン分割切り替え
    {
        let app_weak = app.as_weak();
        app.on_toggle_split(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_split_mode();
                app.set_split_mode(!cur);
            }
        });
    }

    // アクティブペイン切り替え
    {
        let app_weak = app.as_weak();
        app.on_switch_active_pane(move |p| {
            if let Some(app) = app_weak.upgrade() {
                app.set_active_pane(p);
            }
        });
    }

    // 履歴オーバーレイ開閉
    {
        let app_weak = app.as_weak();
        app.on_toggle_history(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_show_history();
                app.set_show_history(!cur);
            }
        });
    }

    // 履歴アイテム選択
    {
        let app_weak = app.as_weak();
        let left_state = left_state.clone();
        let right_state = right_state.clone();
        let add_to_recent = add_to_recent_history.clone();
        app.on_select_history_item(move |path_item| {
            if let Some(app) = app_weak.upgrade() {
                let target_path = path_item.to_string();
                if Path::new(&target_path).exists() {
                    add_to_recent(&target_path);
                    if app.get_active_pane() == 0 {
                        let cur = app.get_left_path().to_string();
                        left_state.borrow_mut().push_history(&cur);
                        let (tail, lead) = split_path(&target_path);
                        let hue = path_hue_color(&target_path);
                        app.set_left_path(SharedString::from(&target_path));
                        app.set_left_tail(SharedString::from(tail));
                        app.set_left_lead(SharedString::from(lead));
                        app.set_left_hue_color(hue);
                        app.set_left_can_back(left_state.borrow().can_back());
                        app.set_left_can_forward(left_state.borrow().can_forward());
                        let (items, _) = read_directory(&target_path, "");
                        app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                        app.set_left_selected(-1);
                    } else {
                        let cur = app.get_right_path().to_string();
                        right_state.borrow_mut().push_history(&cur);
                        let (tail, lead) = split_path(&target_path);
                        let hue = path_hue_color(&target_path);
                        app.set_right_path(SharedString::from(&target_path));
                        app.set_right_tail(SharedString::from(tail));
                        app.set_right_lead(SharedString::from(lead));
                        app.set_right_hue_color(hue);
                        app.set_right_can_back(right_state.borrow().can_back());
                        app.set_right_can_forward(right_state.borrow().can_forward());
                        let (items, _) = read_directory(&target_path, "");
                        app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                        app.set_right_selected(-1);
                    }
                }
                app.set_show_history(false);
            }
        });
    }

    // 履歴消去
    {
        let app_weak = app.as_weak();
        let recent_history = recent_history.clone();
        app.on_clear_recent_history(move || {
            recent_history.borrow_mut().clear();
            if let Some(app) = app_weak.upgrade() {
                let empty_items: Vec<SharedString> = Vec::new();
                app.set_recent_history(ModelRc::from(Rc::new(VecModel::from(empty_items))));
                app.set_status_text(SharedString::from("履歴を消去しました"));
            }
        });
    }

    // 左ペイン: 戻る
    {
        let app_weak = app.as_weak();
        let left_state = left_state.clone();
        app.on_left_navigate_back(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_left_path().to_string();
                if let Some(target) = left_state.borrow_mut().go_back(&cur) {
                    let (tail, lead) = split_path(&target);
                    let hue = path_hue_color(&target);
                    app.set_left_path(SharedString::from(&target));
                    app.set_left_tail(SharedString::from(tail));
                    app.set_left_lead(SharedString::from(lead));
                    app.set_left_hue_color(hue);
                    app.set_left_can_back(left_state.borrow().can_back());
                    app.set_left_can_forward(left_state.borrow().can_forward());
                    let (items, _) = read_directory(&target, &app.get_left_filter());
                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_left_selected(0);
                }
            }
        });
    }

    // 左ペイン: 進む
    {
        let app_weak = app.as_weak();
        let left_state = left_state.clone();
        app.on_left_navigate_forward(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_left_path().to_string();
                if let Some(target) = left_state.borrow_mut().go_forward(&cur) {
                    let (tail, lead) = split_path(&target);
                    let hue = path_hue_color(&target);
                    app.set_left_path(SharedString::from(&target));
                    app.set_left_tail(SharedString::from(tail));
                    app.set_left_lead(SharedString::from(lead));
                    app.set_left_hue_color(hue);
                    app.set_left_can_back(left_state.borrow().can_back());
                    app.set_left_can_forward(left_state.borrow().can_forward());
                    let (items, _) = read_directory(&target, &app.get_left_filter());
                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_left_selected(0);
                }
            }
        });
    }

    // 左ペイン: 親フォルダへ戻る
    {
        let app_weak = app.as_weak();
        let left_state = left_state.clone();
        let add_to_recent = add_to_recent_history.clone();
        app.on_left_navigate_up(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_left_path().to_string();
                if let Some(parent) = Path::new(&cur).parent() {
                    let parent_str = parent.to_string_lossy().to_string();
                    let final_path = if parent_str.len() == 2 && parent_str.ends_with(':') {
                        format!("{}\\", parent_str)
                    } else {
                        parent_str
                    };
                    left_state.borrow_mut().push_history(&cur);
                    add_to_recent(&final_path);
                    let (tail, lead) = split_path(&final_path);
                    let hue = path_hue_color(&final_path);
                    app.set_left_path(SharedString::from(&final_path));
                    app.set_left_tail(SharedString::from(tail));
                    app.set_left_lead(SharedString::from(lead));
                    app.set_left_hue_color(hue);
                    app.set_left_can_back(left_state.borrow().can_back());
                    app.set_left_can_forward(left_state.borrow().can_forward());
                    let (items, _) = read_directory(&final_path, &app.get_left_filter());
                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_left_selected(-1);
                }
            }
        });
    }

    // 左ペイン: 更新
    {
        let app_weak = app.as_weak();
        app.on_left_refresh(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_left_path().to_string();
                let (items, _) = read_directory(&cur, &app.get_left_filter());
                app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
            }
        });
    }

    // 左ペイン: 新規フォルダー作成
    {
        let app_weak = app.as_weak();
        app.on_left_new_folder(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_left_path().to_string();
                if let Ok(new_name) = create_new_folder(&cur) {
                    let (items, _) = read_directory(&cur, &app.get_left_filter());
                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_status_text(SharedString::from(format!("フォルダー「{}」を作成しました", new_name)));
                }
            }
        });
    }

    // 左ペイン: ゴミ箱へ送る
    {
        let app_weak = app.as_weak();
        app.on_left_trash_selected(move || {
            if let Some(app) = app_weak.upgrade() {
                let sel = app.get_left_selected();
                let files = app.get_left_files();
                if sel >= 0 && (sel as usize) < files.row_count() {
                    if let Some(item) = files.row_data(sel as usize) {
                        let cur = app.get_left_path().to_string();
                        let target = Path::new(&cur).join(item.name.as_str());
                        if let Ok(()) = trash_item(&target) {
                            let (items, _) = read_directory(&cur, &app.get_left_filter());
                            app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_left_selected(-1);
                            app.set_status_text(SharedString::from(format!("「{}」をゴミ箱へ送りました", item.name)));
                        }
                    }
                }
            }
        });
    }

    // 左ペイン: フィルター変更
    {
        let app_weak = app.as_weak();
        app.on_left_filter_changed(move |text| {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_left_path().to_string();
                let (items, _) = read_directory(&cur, text.as_str());
                app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                app.set_left_selected(-1);
            }
        });
    }

    // 左ペイン: クリック選択
    {
        let app_weak = app.as_weak();
        app.on_left_item_clicked(move |idx| {
            if let Some(app) = app_weak.upgrade() {
                app.set_left_selected(idx);
            }
        });
    }

    // 左ペイン: ダブルクリック
    {
        let app_weak = app.as_weak();
        let left_state = left_state.clone();
        let add_to_recent = add_to_recent_history.clone();
        app.on_left_item_double_clicked(move |idx| {
            if let Some(app) = app_weak.upgrade() {
                let files = app.get_left_files();
                if let Some(item) = files.row_data(idx as usize) {
                    let cur = app.get_left_path().to_string();
                    let target = Path::new(&cur).join(item.name.as_str());

                    if item.is_dir {
                        let next_str = target.to_string_lossy().to_string();
                        left_state.borrow_mut().push_history(&cur);
                        add_to_recent(&next_str);
                        let (tail, lead) = split_path(&next_str);
                        let hue = path_hue_color(&next_str);
                        app.set_left_path(SharedString::from(&next_str));
                        app.set_left_tail(SharedString::from(tail));
                        app.set_left_lead(SharedString::from(lead));
                        app.set_left_hue_color(hue);
                        app.set_left_can_back(left_state.borrow().can_back());
                        app.set_left_can_forward(left_state.borrow().can_forward());
                        app.set_left_filter(SharedString::from(""));
                        let (items, _) = read_directory(&next_str, "");
                        app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                        app.set_left_selected(-1);
                    } else {
                        let _ = std::process::Command::new("cmd")
                            .args(["/c", "start", "", &target.to_string_lossy()])
                            .spawn();
                    }
                }
            }
        });
    }

    // 右ペイン: 戻る
    {
        let app_weak = app.as_weak();
        let right_state = right_state.clone();
        app.on_right_navigate_back(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_right_path().to_string();
                if let Some(target) = right_state.borrow_mut().go_back(&cur) {
                    let (tail, lead) = split_path(&target);
                    let hue = path_hue_color(&target);
                    app.set_right_path(SharedString::from(&target));
                    app.set_right_tail(SharedString::from(tail));
                    app.set_right_lead(SharedString::from(lead));
                    app.set_right_hue_color(hue);
                    app.set_right_can_back(right_state.borrow().can_back());
                    app.set_right_can_forward(right_state.borrow().can_forward());
                    let (items, _) = read_directory(&target, &app.get_right_filter());
                    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_right_selected(0);
                }
            }
        });
    }

    // 右ペイン: 進む
    {
        let app_weak = app.as_weak();
        let right_state = right_state.clone();
        app.on_right_navigate_forward(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_right_path().to_string();
                if let Some(target) = right_state.borrow_mut().go_forward(&cur) {
                    let (tail, lead) = split_path(&target);
                    let hue = path_hue_color(&target);
                    app.set_right_path(SharedString::from(&target));
                    app.set_right_tail(SharedString::from(tail));
                    app.set_right_lead(SharedString::from(lead));
                    app.set_right_hue_color(hue);
                    app.set_right_can_back(right_state.borrow().can_back());
                    app.set_right_can_forward(right_state.borrow().can_forward());
                    let (items, _) = read_directory(&target, &app.get_right_filter());
                    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_right_selected(0);
                }
            }
        });
    }

    // 右ペイン: 親フォルダへ戻る
    {
        let app_weak = app.as_weak();
        let right_state = right_state.clone();
        let add_to_recent = add_to_recent_history.clone();
        app.on_right_navigate_up(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_right_path().to_string();
                if let Some(parent) = Path::new(&cur).parent() {
                    let parent_str = parent.to_string_lossy().to_string();
                    let final_path = if parent_str.len() == 2 && parent_str.ends_with(':') {
                        format!("{}\\", parent_str)
                    } else {
                        parent_str
                    };
                    right_state.borrow_mut().push_history(&cur);
                    add_to_recent(&final_path);
                    let (tail, lead) = split_path(&final_path);
                    let hue = path_hue_color(&final_path);
                    app.set_right_path(SharedString::from(&final_path));
                    app.set_right_tail(SharedString::from(tail));
                    app.set_right_lead(SharedString::from(lead));
                    app.set_right_hue_color(hue);
                    app.set_right_can_back(right_state.borrow().can_back());
                    app.set_right_can_forward(right_state.borrow().can_forward());
                    let (items, _) = read_directory(&final_path, &app.get_right_filter());
                    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_right_selected(-1);
                }
            }
        });
    }

    // 右ペイン: 更新
    {
        let app_weak = app.as_weak();
        app.on_right_refresh(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_right_path().to_string();
                let (items, _) = read_directory(&cur, &app.get_right_filter());
                app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
            }
        });
    }

    // 右ペイン: 新規フォルダー作成
    {
        let app_weak = app.as_weak();
        app.on_right_new_folder(move || {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_right_path().to_string();
                if let Ok(new_name) = create_new_folder(&cur) {
                    let (items, _) = read_directory(&cur, &app.get_right_filter());
                    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_status_text(SharedString::from(format!("フォルダー「{}」を作成しました", new_name)));
                }
            }
        });
    }

    // 右ペイン: ゴミ箱へ送る
    {
        let app_weak = app.as_weak();
        app.on_right_trash_selected(move || {
            if let Some(app) = app_weak.upgrade() {
                let sel = app.get_right_selected();
                let files = app.get_right_files();
                if sel >= 0 && (sel as usize) < files.row_count() {
                    if let Some(item) = files.row_data(sel as usize) {
                        let cur = app.get_right_path().to_string();
                        let target = Path::new(&cur).join(item.name.as_str());
                        if let Ok(()) = trash_item(&target) {
                            let (items, _) = read_directory(&cur, &app.get_right_filter());
                            app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_right_selected(-1);
                            app.set_status_text(SharedString::from(format!("「{}」をゴミ箱へ送りました", item.name)));
                        }
                    }
                }
            }
        });
    }

    // 右ペイン: フィルター変更
    {
        let app_weak = app.as_weak();
        app.on_right_filter_changed(move |text| {
            if let Some(app) = app_weak.upgrade() {
                let cur = app.get_right_path().to_string();
                let (items, _) = read_directory(&cur, text.as_str());
                app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                app.set_right_selected(-1);
            }
        });
    }

    // 右ペイン: クリック選択
    {
        let app_weak = app.as_weak();
        app.on_right_item_clicked(move |idx| {
            if let Some(app) = app_weak.upgrade() {
                app.set_right_selected(idx);
            }
        });
    }

    // 右ペイン: ダブルクリック
    {
        let app_weak = app.as_weak();
        let right_state = right_state.clone();
        let add_to_recent = add_to_recent_history.clone();
        app.on_right_item_double_clicked(move |idx| {
            if let Some(app) = app_weak.upgrade() {
                let files = app.get_right_files();
                if let Some(item) = files.row_data(idx as usize) {
                    let cur = app.get_right_path().to_string();
                    let target = Path::new(&cur).join(item.name.as_str());

                    if item.is_dir {
                        let next_str = target.to_string_lossy().to_string();
                        right_state.borrow_mut().push_history(&cur);
                        add_to_recent(&next_str);
                        let (tail, lead) = split_path(&next_str);
                        let hue = path_hue_color(&next_str);
                        app.set_right_path(SharedString::from(&next_str));
                        app.set_right_tail(SharedString::from(tail));
                        app.set_right_lead(SharedString::from(lead));
                        app.set_right_hue_color(hue);
                        app.set_right_can_back(right_state.borrow().can_back());
                        app.set_right_can_forward(right_state.borrow().can_forward());
                        app.set_right_filter(SharedString::from(""));
                        let (items, _) = read_directory(&next_str, "");
                        app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                        app.set_right_selected(-1);
                    } else {
                        let _ = std::process::Command::new("cmd")
                            .args(["/c", "start", "", &target.to_string_lossy()])
                            .spawn();
                    }
                }
            }
        });
    }

    // ★包括的キーボードショートカット＆ナビゲーション総合ハンドラ
    {
        let app_weak = app.as_weak();
        let left_state = left_state.clone();
        let right_state = right_state.clone();
        let add_to_recent = add_to_recent_history.clone();
        let clipboard = internal_clipboard.clone();

        app.on_handle_key(move |key_str, ctrl, alt, shift| -> bool {
            if let Some(app) = app_weak.upgrade() {
                let is_left = app.get_active_pane() == 0;
                let key = key_str.as_str();

                // 1. Ctrl + H: 履歴オーバーレイ開閉
                if ctrl && (key == "h" || key == "H") {
                    let cur = app.get_show_history();
                    app.set_show_history(!cur);
                    return true;
                }

                // 2. Alt + ←: 履歴戻る
                if alt && (key == "\u{F702}" || key == "Left" || key == "ArrowLeft") {
                    if is_left {
                        let cur = app.get_left_path().to_string();
                        if let Some(target) = left_state.borrow_mut().go_back(&cur) {
                            let (tail, lead) = split_path(&target);
                            let hue = path_hue_color(&target);
                            app.set_left_path(SharedString::from(&target));
                            app.set_left_tail(SharedString::from(tail));
                            app.set_left_lead(SharedString::from(lead));
                            app.set_left_hue_color(hue);
                            app.set_left_can_back(left_state.borrow().can_back());
                            app.set_left_can_forward(left_state.borrow().can_forward());
                            let (items, _) = read_directory(&target, &app.get_left_filter());
                            app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_left_selected(0);
                        }
                    } else {
                        let cur = app.get_right_path().to_string();
                        if let Some(target) = right_state.borrow_mut().go_back(&cur) {
                            let (tail, lead) = split_path(&target);
                            let hue = path_hue_color(&target);
                            app.set_right_path(SharedString::from(&target));
                            app.set_right_tail(SharedString::from(tail));
                            app.set_right_lead(SharedString::from(lead));
                            app.set_right_hue_color(hue);
                            app.set_right_can_back(right_state.borrow().can_back());
                            app.set_right_can_forward(right_state.borrow().can_forward());
                            let (items, _) = read_directory(&target, &app.get_right_filter());
                            app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_right_selected(0);
                        }
                    }
                    return true;
                }

                // 3. Alt + →: 履歴進む
                if alt && (key == "\u{F703}" || key == "Right" || key == "ArrowRight") {
                    if is_left {
                        let cur = app.get_left_path().to_string();
                        if let Some(target) = left_state.borrow_mut().go_forward(&cur) {
                            let (tail, lead) = split_path(&target);
                            let hue = path_hue_color(&target);
                            app.set_left_path(SharedString::from(&target));
                            app.set_left_tail(SharedString::from(tail));
                            app.set_left_lead(SharedString::from(lead));
                            app.set_left_hue_color(hue);
                            app.set_left_can_back(left_state.borrow().can_back());
                            app.set_left_can_forward(left_state.borrow().can_forward());
                            let (items, _) = read_directory(&target, &app.get_left_filter());
                            app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_left_selected(0);
                        }
                    } else {
                        let cur = app.get_right_path().to_string();
                        if let Some(target) = right_state.borrow_mut().go_forward(&cur) {
                            let (tail, lead) = split_path(&target);
                            let hue = path_hue_color(&target);
                            app.set_right_path(SharedString::from(&target));
                            app.set_right_tail(SharedString::from(tail));
                            app.set_right_lead(SharedString::from(lead));
                            app.set_right_hue_color(hue);
                            app.set_right_can_back(right_state.borrow().can_back());
                            app.set_right_can_forward(right_state.borrow().can_forward());
                            let (items, _) = read_directory(&target, &app.get_right_filter());
                            app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_right_selected(0);
                        }
                    }
                    return true;
                }

                // 4. Ctrl + Shift + N: 新規フォルダー作成
                if ctrl && shift && (key == "n" || key == "N") {
                    let cur = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };
                    if let Ok(new_name) = create_new_folder(&cur) {
                        let filter = if is_left { app.get_left_filter() } else { app.get_right_filter() };
                        let (items, _) = read_directory(&cur, filter.as_str());
                        if is_left {
                            app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                        } else {
                            app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                        }
                        app.set_status_text(SharedString::from(format!("フォルダー「{}」を作成しました", new_name)));
                    }
                    return true;
                }

                // 5. Ctrl + Shift + C: フルパスをクリップボードにコピー
                if ctrl && shift && (key == "c" || key == "C") {
                    let files = if is_left { app.get_left_files() } else { app.get_right_files() };
                    let sel = if is_left { app.get_left_selected() } else { app.get_right_selected() };
                    let cur = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };

                    let target_path = if sel >= 0 && (sel as usize) < files.row_count() {
                        if let Some(item) = files.row_data(sel as usize) {
                            Path::new(&cur).join(item.name.as_str()).to_string_lossy().to_string()
                        } else {
                            cur
                        }
                    } else {
                        cur
                    };

                    set_clipboard_text(&target_path);
                    app.set_status_text(SharedString::from(format!("フルパスをコピーしました: {}", target_path)));
                    return true;
                }

                // 6. Ctrl + C: コピー
                if ctrl && !shift && (key == "c" || key == "C") {
                    let files = if is_left { app.get_left_files() } else { app.get_right_files() };
                    let sel = if is_left { app.get_left_selected() } else { app.get_right_selected() };
                    let cur = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };

                    if sel >= 0 && (sel as usize) < files.row_count() {
                        if let Some(item) = files.row_data(sel as usize) {
                            let target = Path::new(&cur).join(item.name.as_str());
                            *clipboard.borrow_mut() = Some(ClipboardItem {
                                path: target,
                                is_cut: false,
                            });
                            app.set_status_text(SharedString::from(format!("「{}」をコピーしました（Ctrl+V で貼り付け）", item.name)));
                            return true;
                        }
                    }
                }

                // 7. Ctrl + X: 切り取り
                if ctrl && (key == "x" || key == "X") {
                    let files = if is_left { app.get_left_files() } else { app.get_right_files() };
                    let sel = if is_left { app.get_left_selected() } else { app.get_right_selected() };
                    let cur = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };

                    if sel >= 0 && (sel as usize) < files.row_count() {
                        if let Some(item) = files.row_data(sel as usize) {
                            let target = Path::new(&cur).join(item.name.as_str());
                            *clipboard.borrow_mut() = Some(ClipboardItem {
                                path: target,
                                is_cut: true,
                            });
                            app.set_status_text(SharedString::from(format!("「{}」を切り取りました（Ctrl+V で移動）", item.name)));
                            return true;
                        }
                    }
                }

                // 8. Ctrl + V: 貼り付け
                if ctrl && (key == "v" || key == "V") {
                    let cur_dir = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };
                    let cur_path = Path::new(&cur_dir);

                    let clip_item = clipboard.borrow().clone();
                    if let Some(item) = clip_item {
                        if item.path.exists() {
                            let file_name = item.path.file_name().unwrap().to_string_lossy().to_string();
                            let dest = unique_destination_path(cur_path, &file_name);

                            let res = if item.is_cut {
                                fs::rename(&item.path, &dest)
                            } else if item.path.is_dir() {
                                copy_dir_all(&item.path, &dest)
                            } else {
                                fs::copy(&item.path, &dest).map(|_| ())
                            };

                            match res {
                                Ok(()) => {
                                    if item.is_cut {
                                        *clipboard.borrow_mut() = None;
                                    }
                                    let filter = if is_left { app.get_left_filter() } else { app.get_right_filter() };
                                    let (items, _) = read_directory(&cur_dir, filter.as_str());
                                    if is_left {
                                        app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                                    } else {
                                        app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                                    }
                                    app.set_status_text(SharedString::from(format!("「{}」を貼り付けました", file_name)));
                                }
                                Err(e) => {
                                    app.set_status_text(SharedString::from(format!("貼り付けに失敗しました: {}", e)));
                                }
                            }
                            return true;
                        }
                    }
                }

                // 9. Tab キー: ペイン切り替え (0 ⇄ 1)
                if key == "\t" || key == "Tab" {
                    app.set_active_pane(if is_left { 1 } else { 0 });
                    return true;
                }

                let files = if is_left { app.get_left_files() } else { app.get_right_files() };
                let count = files.row_count() as i32;
                let current_sel = if is_left { app.get_left_selected() } else { app.get_right_selected() };

                // 10. ↑ キー: カーソル上移動
                if key == "\u{F700}" || key == "Up" || key == "ArrowUp" {
                    let new_sel = if current_sel <= 0 { 0 } else { current_sel - 1 };
                    if is_left { app.set_left_selected(new_sel); } else { app.set_right_selected(new_sel); }
                    return true;
                }

                // 11. ↓ キー: カーソル下移動
                if key == "\u{F701}" || key == "Down" || key == "ArrowDown" {
                    let new_sel = if current_sel < 0 { 0 } else if current_sel >= count - 1 { count - 1 } else { current_sel + 1 };
                    if is_left { app.set_left_selected(new_sel); } else { app.set_right_selected(new_sel); }
                    return true;
                }

                // 12. Enter キー: フォルダ潜入 / ファイル実行
                if key == "\n" || key == "\r" || key == "Return" || key == "Enter" {
                    if current_sel >= 0 && (current_sel as usize) < files.row_count() {
                        if let Some(item) = files.row_data(current_sel as usize) {
                            let cur_path = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };
                            let target = Path::new(&cur_path).join(item.name.as_str());

                            if item.is_dir {
                                let next_str = target.to_string_lossy().to_string();
                                add_to_recent(&next_str);
                                let (tail, lead) = split_path(&next_str);
                                let hue = path_hue_color(&next_str);
                                if is_left {
                                    left_state.borrow_mut().push_history(&cur_path);
                                    app.set_left_path(SharedString::from(&next_str));
                                    app.set_left_tail(SharedString::from(tail));
                                    app.set_left_lead(SharedString::from(lead));
                                    app.set_left_hue_color(hue);
                                    app.set_left_can_back(left_state.borrow().can_back());
                                    app.set_left_can_forward(left_state.borrow().can_forward());
                                    app.set_left_filter(SharedString::from(""));
                                    let (items, _) = read_directory(&next_str, "");
                                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                                    app.set_left_selected(0);
                                } else {
                                    right_state.borrow_mut().push_history(&cur_path);
                                    app.set_right_path(SharedString::from(&next_str));
                                    app.set_right_tail(SharedString::from(tail));
                                    app.set_right_lead(SharedString::from(lead));
                                    app.set_right_hue_color(hue);
                                    app.set_right_can_back(right_state.borrow().can_back());
                                    app.set_right_can_forward(right_state.borrow().can_forward());
                                    app.set_right_filter(SharedString::from(""));
                                    let (items, _) = read_directory(&next_str, "");
                                    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                                    app.set_right_selected(0);
                                }
                            } else {
                                let _ = std::process::Command::new("cmd")
                                    .args(["/c", "start", "", &target.to_string_lossy()])
                                    .spawn();
                            }
                            return true;
                        }
                    }
                }

                // 13. Delete キー: ゴミ箱へ送る
                if key == "\u{7F}" || key == "\u{F728}" || key == "Delete" || key == "Del" {
                    let sel = if is_left { app.get_left_selected() } else { app.get_right_selected() };
                    if sel >= 0 && (sel as usize) < files.row_count() {
                        if let Some(item) = files.row_data(sel as usize) {
                            let cur_path = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };
                            let target = Path::new(&cur_path).join(item.name.as_str());
                            if let Ok(()) = trash_item(&target) {
                                let filter = if is_left { app.get_left_filter() } else { app.get_right_filter() };
                                let (items, _) = read_directory(&cur_path, filter.as_str());
                                let new_count = items.len() as i32;
                                if is_left {
                                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                                    let next_sel = if sel >= new_count { new_count - 1 } else { sel };
                                    app.set_left_selected(next_sel);
                                } else {
                                    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                                    let next_sel = if sel >= new_count { new_count - 1 } else { sel };
                                    app.set_right_selected(next_sel);
                                }
                                app.set_status_text(SharedString::from(format!("「{}」をゴミ箱へ送りました", item.name)));
                            }
                        }
                    }
                    return true;
                }

                // 14. Backspace キー: 親フォルダへ戻る
                if key == "\u{8}" || key == "BackSpace" || key == "Backspace" {
                    let cur_path = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };
                    if let Some(parent) = Path::new(&cur_path).parent() {
                        let parent_str = parent.to_string_lossy().to_string();
                        let final_path = if parent_str.len() == 2 && parent_str.ends_with(':') {
                            format!("{}\\", parent_str)
                        } else {
                            parent_str
                        };
                        add_to_recent(&final_path);
                        let (tail, lead) = split_path(&final_path);
                        let hue = path_hue_color(&final_path);
                        if is_left {
                            left_state.borrow_mut().push_history(&cur_path);
                            app.set_left_path(SharedString::from(&final_path));
                            app.set_left_tail(SharedString::from(tail));
                            app.set_left_lead(SharedString::from(lead));
                            app.set_left_hue_color(hue);
                            app.set_left_can_back(left_state.borrow().can_back());
                            app.set_left_can_forward(left_state.borrow().can_forward());
                            let (items, _) = read_directory(&final_path, &app.get_left_filter());
                            app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_left_selected(0);
                        } else {
                            right_state.borrow_mut().push_history(&cur_path);
                            app.set_right_path(SharedString::from(&final_path));
                            app.set_right_tail(SharedString::from(tail));
                            app.set_right_lead(SharedString::from(lead));
                            app.set_right_hue_color(hue);
                            app.set_right_can_back(right_state.borrow().can_back());
                            app.set_right_can_forward(right_state.borrow().can_forward());
                            let (items, _) = read_directory(&final_path, &app.get_right_filter());
                            app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_right_selected(0);
                        }
                        return true;
                    }
                }

                // 15. F5: 更新
                if key == "\u{F708}" || key == "F5" {
                    let cur_path = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };
                    let filter = if is_left { app.get_left_filter() } else { app.get_right_filter() };
                    let (items, _) = read_directory(&cur_path, filter.as_str());
                    if is_left {
                        app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    } else {
                        app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    }
                    return true;
                }
            }
            false
        });
    }

    app.run()?;
    Ok(())
}
