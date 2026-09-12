#![windows_subsystem = "windows"]

use chrono::{DateTime, Local};
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::fs;
use std::path::Path;
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
        "zip" | "rar" | "7z" | "tar" | "gz" => "📦",
        "txt" | "md" | "json" | "rs" | "ts" | "js" | "html" | "css" => "📄",
        "mp3" | "wav" | "flac" | "m4a" => "🎵",
        "mp4" | "mkv" | "avi" | "mov" | "webm" => "🎬",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" => "📑",
        "exe" | "msi" | "bat" | "cmd" | "ps1" => "⚙️",
        _ => "📄",
    }
}

// ディレクトリ一覧取得
fn read_directory(path_str: &str, filter: &str) -> (Vec<FileItem>, Result<(), String>) {
    let path = Path::new(path_str);
    let mut dirs_list: Vec<FileItem> = Vec::new();
    let mut files_list: Vec<FileItem> = Vec::new();

    let read_res = fs::read_dir(path);
    if let Err(e) = read_res {
        return (Vec::new(), Err(e.to_string()));
    }

    let filter_lower = filter.to_lowercase();

    for entry in read_res.unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();

        // フィルター適用
        if !filter_lower.is_empty() && !name.to_lowercase().contains(&filter_lower) {
            continue;
        }

        let meta = entry.metadata().ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size_str = if is_dir {
            String::new()
        } else {
            format_size(meta.as_ref().map(|m| m.len()).unwrap_or(0))
        };

        let date_str = meta
            .as_ref()
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

// ドライブ検出
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

    // 左ペイン状態の反映ヘルパー
    let update_left_pane = {
        let app_weak = app.as_weak();
        move |path: &str, filter: &str| {
            if let Some(app) = app_weak.upgrade() {
                let (tail, lead) = split_path(path);
                let hue = path_hue_color(path);
                app.set_left_path(SharedString::from(path));
                app.set_left_tail(SharedString::from(tail));
                app.set_left_lead(SharedString::from(lead));
                app.set_left_hue_color(hue);

                let (items, _) = read_directory(path, filter);
                app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
            }
        }
    };

    // 右ペイン状態の反映ヘルパー
    let update_right_pane = {
        let app_weak = app.as_weak();
        move |path: &str, filter: &str| {
            if let Some(app) = app_weak.upgrade() {
                let (tail, lead) = split_path(path);
                let hue = path_hue_color(path);
                app.set_right_path(SharedString::from(path));
                app.set_right_tail(SharedString::from(tail));
                app.set_right_lead(SharedString::from(lead));
                app.set_right_hue_color(hue);

                let (items, _) = read_directory(path, filter);
                app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
            }
        }
    };

    // 初期表示設定
    update_left_pane(&left_initial, "");
    update_right_pane(&right_initial, "");

    let elapsed = start_time.elapsed().unwrap_or_default();
    app.set_status_text(SharedString::from(format!(
        "⚡ GPUネイティブ描画エンジン稼働中 · 初期化: {:.1}ms",
        elapsed.as_secs_f64() * 1000.0
    )));

    // ドライブ選択
    {
        let app_weak = app.as_weak();
        app.on_drive_selected(move |drive| {
            if let Some(app) = app_weak.upgrade() {
                let drive_str = drive.to_string();
                if app.get_active_pane() == 0 {
                    let (tail, lead) = split_path(&drive_str);
                    let hue = path_hue_color(&drive_str);
                    app.set_left_path(SharedString::from(&drive_str));
                    app.set_left_tail(SharedString::from(tail));
                    app.set_left_lead(SharedString::from(lead));
                    app.set_left_hue_color(hue);
                    let (items, _) = read_directory(&drive_str, &app.get_left_filter());
                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_left_selected(-1);
                } else {
                    let (tail, lead) = split_path(&drive_str);
                    let hue = path_hue_color(&drive_str);
                    app.set_right_path(SharedString::from(&drive_str));
                    app.set_right_tail(SharedString::from(tail));
                    app.set_right_lead(SharedString::from(lead));
                    app.set_right_hue_color(hue);
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

    // 左ペイン: 上へ戻る
    {
        let app_weak = app.as_weak();
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
                    let (tail, lead) = split_path(&final_path);
                    let hue = path_hue_color(&final_path);
                    app.set_left_path(SharedString::from(&final_path));
                    app.set_left_tail(SharedString::from(tail));
                    app.set_left_lead(SharedString::from(lead));
                    app.set_left_hue_color(hue);
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

    // 左ペイン: ダブルクリック（フォルダ潜行またはファイル起動）
    {
        let app_weak = app.as_weak();
        app.on_left_item_double_clicked(move |idx| {
            if let Some(app) = app_weak.upgrade() {
                let files = app.get_left_files();
                if let Some(item) = files.row_data(idx as usize) {
                    let cur = app.get_left_path().to_string();
                    let target = Path::new(&cur).join(item.name.as_str());

                    if item.is_dir {
                        let next_str = target.to_string_lossy().to_string();
                        let (tail, lead) = split_path(&next_str);
                        let hue = path_hue_color(&next_str);
                        app.set_left_path(SharedString::from(&next_str));
                        app.set_left_tail(SharedString::from(tail));
                        app.set_left_lead(SharedString::from(lead));
                        app.set_left_hue_color(hue);
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

    // 右ペイン: 上へ戻る
    {
        let app_weak = app.as_weak();
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
                    let (tail, lead) = split_path(&final_path);
                    let hue = path_hue_color(&final_path);
                    app.set_right_path(SharedString::from(&final_path));
                    app.set_right_tail(SharedString::from(tail));
                    app.set_right_lead(SharedString::from(lead));
                    app.set_right_hue_color(hue);
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
        app.on_right_item_double_clicked(move |idx| {
            if let Some(app) = app_weak.upgrade() {
                let files = app.get_right_files();
                if let Some(item) = files.row_data(idx as usize) {
                    let cur = app.get_right_path().to_string();
                    let target = Path::new(&cur).join(item.name.as_str());

                    if item.is_dir {
                        let next_str = target.to_string_lossy().to_string();
                        let (tail, lead) = split_path(&next_str);
                        let hue = path_hue_color(&next_str);
                        app.set_right_path(SharedString::from(&next_str));
                        app.set_right_tail(SharedString::from(tail));
                        app.set_right_lead(SharedString::from(lead));
                        app.set_right_hue_color(hue);
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

    // ★思想1: キーボード総合ハンドラ（マウスを持たずに完結する操作）
    {
        let app_weak = app.as_weak();
        app.on_handle_key(move |key_str| {
            if let Some(app) = app_weak.upgrade() {
                let is_left = app.get_active_pane() == 0;
                let key = key_str.as_str();

                // Tab キー: ペイン切り替え (0 ⇄ 1)
                if key == "\t" || key == "Tab" {
                    app.set_active_pane(if is_left { 1 } else { 0 });
                    return true;
                }

                let files = if is_left { app.get_left_files() } else { app.get_right_files() };
                let count = files.row_count() as i32;
                let current_sel = if is_left { app.get_left_selected() } else { app.get_right_selected() };

                // ↑ キー: カーソル上移動
                if key == "\u{F700}" || key == "Up" || key == "ArrowUp" {
                    let new_sel = if current_sel <= 0 { 0 } else { current_sel - 1 };
                    if is_left { app.set_left_selected(new_sel); } else { app.set_right_selected(new_sel); }
                    return true;
                }

                // ↓ キー: カーソル下移動
                if key == "\u{F701}" || key == "Down" || key == "ArrowDown" {
                    let new_sel = if current_sel < 0 { 0 } else if current_sel >= count - 1 { count - 1 } else { current_sel + 1 };
                    if is_left { app.set_left_selected(new_sel); } else { app.set_right_selected(new_sel); }
                    return true;
                }

                // Enter キー: 潜る / 実行
                if key == "\n" || key == "\r" || key == "Return" || key == "Enter" {
                    if current_sel >= 0 && (current_sel as usize) < files.row_count() {
                        if let Some(item) = files.row_data(current_sel as usize) {
                            let cur_path = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };
                            let target = Path::new(&cur_path).join(item.name.as_str());

                            if item.is_dir {
                                let next_str = target.to_string_lossy().to_string();
                                let (tail, lead) = split_path(&next_str);
                                let hue = path_hue_color(&next_str);
                                if is_left {
                                    app.set_left_path(SharedString::from(&next_str));
                                    app.set_left_tail(SharedString::from(tail));
                                    app.set_left_lead(SharedString::from(lead));
                                    app.set_left_hue_color(hue);
                                    app.set_left_filter(SharedString::from(""));
                                    let (items, _) = read_directory(&next_str, "");
                                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                                    app.set_left_selected(0);
                                } else {
                                    app.set_right_path(SharedString::from(&next_str));
                                    app.set_right_tail(SharedString::from(tail));
                                    app.set_right_lead(SharedString::from(lead));
                                    app.set_right_hue_color(hue);
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

                // Delete キー: ゴミ箱へ安全削除
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

                // Backspace キー: 親フォルダへ戻る
                if key == "\u{8}" || key == "BackSpace" || key == "Backspace" {
                    let cur_path = if is_left { app.get_left_path().to_string() } else { app.get_right_path().to_string() };
                    if let Some(parent) = Path::new(&cur_path).parent() {
                        let parent_str = parent.to_string_lossy().to_string();
                        let final_path = if parent_str.len() == 2 && parent_str.ends_with(':') {
                            format!("{}\\", parent_str)
                        } else {
                            parent_str
                        };
                        let (tail, lead) = split_path(&final_path);
                        let hue = path_hue_color(&final_path);
                        if is_left {
                            app.set_left_path(SharedString::from(&final_path));
                            app.set_left_tail(SharedString::from(tail));
                            app.set_left_lead(SharedString::from(lead));
                            app.set_left_hue_color(hue);
                            let (items, _) = read_directory(&final_path, &app.get_left_filter());
                            app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_left_selected(0);
                        } else {
                            app.set_right_path(SharedString::from(&final_path));
                            app.set_right_tail(SharedString::from(tail));
                            app.set_right_lead(SharedString::from(lead));
                            app.set_right_hue_color(hue);
                            let (items, _) = read_directory(&final_path, &app.get_right_filter());
                            app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                            app.set_right_selected(0);
                        }
                        return true;
                    }
                }

                // F5: 更新
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
