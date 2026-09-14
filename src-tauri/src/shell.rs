//! Windows シェルとの連携。「プログラムから開く」「プロパティ」と、
//! 他のアプリ（7-Zip や VS Code など）が足した項目を含む Windows 本来の右クリックメニュー。
//!
//! どれもウィンドウのメッセージループがあるスレッドで呼ぶ必要があるので、
//! メインスレッドへ回してから実行する。

use tauri::WebviewWindow;

#[tauri::command]
pub fn open_with(path: String) -> Result<(), String> {
  // Windows 標準の「プログラムから開く」ダイアログ。
  std::process::Command::new("rundll32.exe")
    .arg("shell32.dll,OpenAs_RunDLL")
    .arg(&path)
    .spawn()
    .map(|_| ())
    .map_err(|e| format!("「プログラムから開く」を表示できません: {e}"))
}

#[tauri::command]
pub fn show_properties(window: WebviewWindow, path: String) -> Result<(), String> {
  let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
  window
    .run_on_main_thread(move || imp::properties(hwnd, &path))
    .map_err(|e| e.to_string())
}

/// マウスカーソルの位置に Windows のメニューを出す。選ばれた項目はシェルが実行する。
#[tauri::command]
pub fn show_shell_menu(window: WebviewWindow, paths: Vec<String>) -> Result<(), String> {
  if paths.is_empty() {
    return Ok(());
  }
  let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
  window
    .run_on_main_thread(move || {
      if let Err(e) = imp::context_menu(hwnd, &paths) {
        log::warn!("shell menu: {e}");
      }
    })
    .map_err(|e| e.to_string())
}

#[cfg(windows)]
mod imp {
  use windows::core::{PCSTR, PCWSTR};
  use windows::Win32::Foundation::{HWND, POINT};
  use windows::Win32::System::Com::CoTaskMemFree;
  use windows::Win32::UI::Shell::Common::ITEMIDLIST;
  use windows::Win32::UI::Shell::{
    IContextMenu, IShellFolder, SHBindToParent, SHObjectProperties, SHParseDisplayName, CMF_NORMAL,
    CMINVOKECOMMANDINFO, SHOP_FILEPATH,
  };
  use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetCursorPos, TrackPopupMenuEx, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON,
  };

  fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
  }

  pub fn properties(hwnd: isize, path: &str) {
    let path = wide(path);
    unsafe {
      let _ = SHObjectProperties(Some(HWND(hwnd as _)), SHOP_FILEPATH, PCWSTR(path.as_ptr()), PCWSTR::null());
    }
  }

  /// 解放し忘れないよう、PIDL をまとめて持つ。
  struct Pidls(Vec<*mut ITEMIDLIST>);
  impl Drop for Pidls {
    fn drop(&mut self) {
      for p in &self.0 {
        unsafe { CoTaskMemFree(Some(*p as _)) };
      }
    }
  }

  pub fn context_menu(hwnd: isize, paths: &[String]) -> windows::core::Result<()> {
    let hwnd = HWND(hwnd as _);
    unsafe {
      let mut owned = Pidls(Vec::new());
      for path in paths {
        let wide = wide(path);
        let mut pidl = std::ptr::null_mut();
        SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)?;
        owned.0.push(pidl);
      }

      // 同じフォルダの項目として親に問い合わせる。別フォルダが混ざっていたら先頭のフォルダ分だけ扱う。
      let mut children = Vec::new();
      let mut parent: Option<IShellFolder> = None;
      for pidl in &owned.0 {
        let mut child = std::ptr::null_mut();
        let folder: IShellFolder = SHBindToParent(*pidl, Some(&mut child))?;
        if parent.is_none() {
          parent = Some(folder);
        }
        children.push(child as *const ITEMIDLIST);
      }
      let Some(parent) = parent else { return Ok(()) };
      let menu: IContextMenu = parent.GetUIObjectOf(hwnd, &children, None)?;

      let hmenu = CreatePopupMenu()?;
      const FIRST: u32 = 1;
      let result = (|| -> windows::core::Result<()> {
        menu.QueryContextMenu(hmenu, 0, FIRST, 0x7FFF, CMF_NORMAL).ok()?;
        let mut pt = POINT::default();
        GetCursorPos(&mut pt)?;
        let chosen = TrackPopupMenuEx(hmenu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, pt.x, pt.y, hwnd, None).0 as u32;
        if chosen >= FIRST {
          let info = CMINVOKECOMMANDINFO {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
            hwnd,
            // 項目番号をそのまま文字列ポインタの位置に入れるのが Win32 の約束（MAKEINTRESOURCE）。
            lpVerb: PCSTR((chosen - FIRST) as usize as *const u8),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
          };
          menu.InvokeCommand(&info)?;
        }
        Ok(())
      })();
      let _ = DestroyMenu(hmenu);
      result
    }
  }
}

#[cfg(not(windows))]
mod imp {
  pub fn properties(_: isize, _: &str) {}
  pub fn context_menu(_: isize, _: &[String]) -> Result<(), String> {
    Ok(())
  }
}
