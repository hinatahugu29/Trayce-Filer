#![windows_subsystem = "windows"]

use chrono::{DateTime, Local};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::time::SystemTime;

slint::include_modules!();

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

// 拡張子に応じたアイコン
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

// 指定ディレクトリのファイル一覧を取得して Slint 用の FileItem リストに変換
fn read_directory(path_str: &str) -> (Vec<FileItem>, Result<(), String>) {
    let path = Path::new(path_str);
    let mut dirs_list: Vec<FileItem> = Vec::new();
    let mut files_list: Vec<FileItem> = Vec::new();

    let read_res = fs::read_dir(path);
    if let Err(e) = read_res {
        return (Vec::new(), Err(e.to_string()));
    }

    for entry in read_res.unwrap().flatten() {
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

        let name = entry.file_name().to_string_lossy().to_string();
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

    // ディレクトリ名順、ファイル名順でソート
    dirs_list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files_list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    dirs_list.extend(files_list);
    (dirs_list, Ok(()))
}

// Windows の利用可能ドライブを検出
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start_time = SystemTime::now();

    // UIウィンドウの生成
    let app = AppWindow::new()?;

    // ドライブ一覧の初期化
    let drives = detect_drives();
    let drives_model = Rc::new(VecModel::from(drives));
    app.set_drives(ModelRc::from(drives_model));

    // 初期パスの設定（ユーザーのホームディレクトリ、または C:\）
    let initial_path = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "C:\\".to_string());

    // 左ペイン初期化
    app.set_left_path(SharedString::from(&initial_path));
    let (left_items, _) = read_directory(&initial_path);
    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(left_items))));

    // 右ペイン初期化（ドキュメントまたは初期パス）
    let initial_right = dirs::document_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| initial_path.clone());
    app.set_right_path(SharedString::from(&initial_right));
    let (right_items, _) = read_directory(&initial_right);
    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(right_items))));

    let elapsed = start_time.elapsed().unwrap_or_default();
    app.set_status_text(SharedString::from(format!(
        "⚡ GPUネイティブ描画エンジン起動完了 (DirectX / WGPU) · 初期化: {:.1}ms",
        elapsed.as_secs_f64() * 1000.0
    )));

    // ドライブ選択イベント
    let app_weak = app.as_weak();
    app.on_drive_selected(move |drive| {
        if let Some(app) = app_weak.upgrade() {
            let path = drive.to_string();
            app.set_left_path(SharedString::from(&path));
            let (items, _) = read_directory(&path);
            app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
            app.set_left_selected(-1);
        }
    });

    // 2ペイン分割トグルイベント
    let app_weak = app.as_weak();
    app.on_toggle_split(move || {
        if let Some(app) = app_weak.upgrade() {
            let current = app.get_split_mode();
            app.set_split_mode(!current);
        }
    });

    // 左ペイン: 親ディレクトリへ移動
    let app_weak = app.as_weak();
    app.on_left_navigate_up(move || {
        if let Some(app) = app_weak.upgrade() {
            let current = app.get_left_path().to_string();
            if let Some(parent) = Path::new(&current).parent() {
                let parent_str = parent.to_string_lossy().to_string();
                let final_path = if parent_str.len() == 2 && parent_str.ends_with(':') {
                    format!("{}\\", parent_str)
                } else {
                    parent_str
                };
                app.set_left_path(SharedString::from(&final_path));
                let (items, _) = read_directory(&final_path);
                app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                app.set_left_selected(-1);
            }
        }
    });

    // 左ペイン: 更新
    let app_weak = app.as_weak();
    app.on_left_refresh(move || {
        if let Some(app) = app_weak.upgrade() {
            let path = app.get_left_path().to_string();
            let (items, _) = read_directory(&path);
            app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
        }
    });

    // 左ペイン: 項目選択
    let app_weak = app.as_weak();
    app.on_left_item_clicked(move |idx| {
        if let Some(app) = app_weak.upgrade() {
            app.set_left_selected(idx);
        }
    });

    // 左ペイン: ダブルクリック（フォルダ潜行またはファイル実行）
    let app_weak = app.as_weak();
    app.on_left_item_double_clicked(move |idx| {
        if let Some(app) = app_weak.upgrade() {
            let files = app.get_left_files();
            if let Some(item) = files.row_data(idx as usize) {
                let current_dir = app.get_left_path().to_string();
                let target_path = Path::new(&current_dir).join(item.name.as_str());

                if item.is_dir {
                    let next_str = target_path.to_string_lossy().to_string();
                    app.set_left_path(SharedString::from(&next_str));
                    let (items, _) = read_directory(&next_str);
                    app.set_left_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_left_selected(-1);
                } else {
                    // Windows でファイルを既定アプリで起動
                    let _ = std::process::Command::new("cmd")
                        .args(["/c", "start", "", &target_path.to_string_lossy()])
                        .spawn();
                }
            }
        }
    });

    // 右ペイン: 親ディレクトリへ移動
    let app_weak = app.as_weak();
    app.on_right_navigate_up(move || {
        if let Some(app) = app_weak.upgrade() {
            let current = app.get_right_path().to_string();
            if let Some(parent) = Path::new(&current).parent() {
                let parent_str = parent.to_string_lossy().to_string();
                let final_path = if parent_str.len() == 2 && parent_str.ends_with(':') {
                    format!("{}\\", parent_str)
                } else {
                    parent_str
                };
                app.set_right_path(SharedString::from(&final_path));
                let (items, _) = read_directory(&final_path);
                app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                app.set_right_selected(-1);
            }
        }
    });

    // 右ペイン: 更新
    let app_weak = app.as_weak();
    app.on_right_refresh(move || {
        if let Some(app) = app_weak.upgrade() {
            let path = app.get_right_path().to_string();
            let (items, _) = read_directory(&path);
            app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
        }
    });

    // 右ペイン: 項目選択
    let app_weak = app.as_weak();
    app.on_right_item_clicked(move |idx| {
        if let Some(app) = app_weak.upgrade() {
            app.set_right_selected(idx);
        }
    });

    // 右ペイン: ダブルクリック
    let app_weak = app.as_weak();
    app.on_right_item_double_clicked(move |idx| {
        if let Some(app) = app_weak.upgrade() {
            let files = app.get_right_files();
            if let Some(item) = files.row_data(idx as usize) {
                let current_dir = app.get_right_path().to_string();
                let target_path = Path::new(&current_dir).join(item.name.as_str());

                if item.is_dir {
                    let next_str = target_path.to_string_lossy().to_string();
                    app.set_right_path(SharedString::from(&next_str));
                    let (items, _) = read_directory(&next_str);
                    app.set_right_files(ModelRc::from(Rc::new(VecModel::from(items))));
                    app.set_right_selected(-1);
                } else {
                    let _ = std::process::Command::new("cmd")
                        .args(["/c", "start", "", &target_path.to_string_lossy()])
                        .spawn();
                }
            }
        }
    });

    // アプリケーション実行ループ
    app.run()?;
    Ok(())
}
