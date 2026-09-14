//! 二重起動の防止。
//!
//! 2つ目のプロセスが立つと、窓一覧（レジストリ）が別々になるうえ、同じ state.json を
//! 互いに上書きしてお気に入りや履歴が片方の内容で消える。ホットキーも取り合いになる。
//!
//! 仕組みは標準ライブラリだけで組む。
//! - ロックファイルを排他ロックできたプロセスが「最初の1つ」。OS がプロセス終了時に
//!   ロックを外すので、落ちた後に残骸で起動できなくなることは無い。
//! - 最初の1つは 127.0.0.1 の空きポートで待ち受け、その番号を別ファイルに書く
//!   （Windows の排他ロック中はロックファイル自体を他から読めないため分ける）。
//! - 2つ目はその番号へ `focus` を送り、既存の窓を前面に出してもらって終了する。

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 開発ビルドは別の名前にする。配布版を常用しながら `tauri dev` で開発しても、
/// 互いを「既に起動中」と見なして起動できなくなる事態を避ける。
/// （保存先の state.json は同じなので、同時に使うと上書きし合う点は変わらない。）
const NAME: &str = if cfg!(debug_assertions) { "trayce-filer-instance-dev" } else { "trayce-filer-instance" };
const FOCUS: &[u8] = b"focus";

/// 最初の1つであり続ける間、保持しておく。落とすとロックが外れる。
pub struct InstanceGuard {
  _lock: File,
  listener: TcpListener,
}

pub enum Acquire {
  /// このプロセスが最初の1つ。`None` はロック自体が使えなかった場合で、
  /// 起動を止めるより二重起動を許す方がましなので、そのまま起動する。
  First(Option<InstanceGuard>),
  /// 既に動いているプロセスがあり、前面化を頼んだ。このプロセスは終了してよい。
  Second,
}

pub fn acquire() -> Acquire {
  acquire_in(&std::env::temp_dir())
}

fn paths(dir: &Path) -> (PathBuf, PathBuf) {
  (dir.join(format!("{NAME}.lock")), dir.join(format!("{NAME}.port")))
}

fn acquire_in(dir: &Path) -> Acquire {
  let (lock_path, port_path) = paths(dir);
  let Ok(lock) = OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path) else {
    return Acquire::First(None);
  };
  match lock.try_lock() {
    Ok(()) => {
      let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else { return Acquire::First(None) };
      let Ok(port) = listener.local_addr().map(|addr| addr.port()) else { return Acquire::First(None) };
      if std::fs::write(&port_path, port.to_string()).is_err() {
        return Acquire::First(None);
      }
      Acquire::First(Some(InstanceGuard { _lock: lock, listener }))
    }
    Err(TryLockError::WouldBlock) => {
      // 連絡が付かなくても（相手が起動途中など）終了する。
      // 黙って2つ目を立てると state.json を壊しうる。
      if !notify_existing(&port_path) {
        log::warn!("既に起動中のファイラへ前面化を依頼できませんでした");
      }
      Acquire::Second
    }
    Err(TryLockError::Error(_)) => Acquire::First(None),
  }
}

fn notify_existing(port_path: &Path) -> bool {
  let Some(port) = std::fs::read_to_string(port_path).ok().and_then(|text| text.trim().parse::<u16>().ok()) else {
    return false;
  };
  let Ok(mut stream) = TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_millis(500)) else {
    return false;
  };
  stream.write_all(FOCUS).is_ok()
}

/// 後から起動されたプロセスからの前面化依頼を待ち受ける。
pub fn serve(guard: InstanceGuard, on_focus: impl Fn() + Send + 'static) {
  std::thread::spawn(move || {
    let InstanceGuard { _lock, listener } = guard;
    // ロックはこのスレッドが持ち続ける（プロセスが生きている間、スレッドも生きている）。
    let _lock = _lock;
    for stream in listener.incoming() {
      let Ok(mut stream) = stream else { continue };
      let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
      let mut buf = [0u8; 16];
      let Ok(n) = stream.read(&mut buf) else { continue };
      // 同じ PC の他のプロセスも接続はできる。決まった合図以外は無視する。
      if &buf[..n] == FOCUS {
        on_focus();
      }
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::mpsc::channel;

  fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("filer_instance_test_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
  }

  #[test]
  fn second_acquire_asks_the_first_to_focus() {
    let dir = scratch("focus");
    let Acquire::First(Some(guard)) = acquire_in(&dir) else { panic!("最初の1つになれるはず") };
    let (tx, rx) = channel();
    serve(guard, move || tx.send(()).unwrap());

    assert!(matches!(acquire_in(&dir), Acquire::Second), "2つ目は起動しない");
    assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok(), "最初の1つに前面化が届く");
  }

  #[test]
  fn the_lock_is_released_when_the_guard_is_dropped() {
    let dir = scratch("release");
    let Acquire::First(Some(guard)) = acquire_in(&dir) else { panic!() };
    drop(guard);
    assert!(matches!(acquire_in(&dir), Acquire::First(Some(_))), "終了後は再び起動できる");
  }
}
