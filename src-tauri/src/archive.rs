use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

/// 選択されたものを1つの ZIP にまとめる。
///
/// 出力先は選択の親ディレクトリ。名前は
/// - 1件だけならその名前 + `.zip`
/// - 複数ならその親フォルダ名 + `.zip`
/// 既存と衝突したら `name (2).zip` のように退避する（黙って上書きしない）。
#[tauri::command]
pub fn compress_to_zip(paths: Vec<String>) -> Result<String, String> {
  if paths.is_empty() {
    return Err("圧縮するものが選ばれていません".into());
  }

  let first = PathBuf::from(&paths[0]);
  let parent = first
    .parent()
    .ok_or("親ディレクトリを特定できません")?
    .to_path_buf();

  let base_name = if paths.len() == 1 {
    first
      .file_stem()
      .map(|s| s.to_string_lossy().to_string())
      .unwrap_or_else(|| "archive".into())
  } else {
    parent
      .file_name()
      .map(|s| s.to_string_lossy().to_string())
      .unwrap_or_else(|| "archive".into())
  };

  let target = unique_zip_path(&parent, &base_name);
  let file = std::fs::File::create(&target).map_err(|e| format!("作成できません: {e}"))?;
  let mut zip = zip::ZipWriter::new(file);
  let options: zip::write::FileOptions<'_, ()> =
    zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

  for p in &paths {
    let src = Path::new(p);
    let Some(name) = src.file_name() else { continue };
    let name = name.to_string_lossy().to_string();

    if src.is_dir() {
      add_dir(&mut zip, src, &name, options).map_err(|e| format!("{p}: {e}"))?;
    } else {
      zip.start_file(&name, options).map_err(|e| format!("{p}: {e}"))?;
      let bytes = std::fs::read(src).map_err(|e| format!("{p}: {e}"))?;
      zip.write_all(&bytes).map_err(|e| format!("{p}: {e}"))?;
    }
  }

  zip.finish().map_err(|e| format!("書き出しに失敗: {e}"))?;
  Ok(target.to_string_lossy().to_string())
}

fn add_dir<W: Write + Seek>(
  zip: &mut zip::ZipWriter<W>,
  dir: &Path,
  prefix: &str,
  options: zip::write::FileOptions<'_, ()>,
) -> std::io::Result<()> {
  // 空フォルダも構造として残す。展開した時に消えていると驚く。
  zip.add_directory(format!("{prefix}/"), options)?;

  for entry in std::fs::read_dir(dir)? {
    let entry = entry?;
    let name = entry.file_name().to_string_lossy().to_string();
    let inner = format!("{prefix}/{name}");

    if entry.file_type()?.is_dir() {
      add_dir(zip, &entry.path(), &inner, options)?;
    } else {
      zip.start_file(&inner, options)?;
      let bytes = std::fs::read(entry.path())?;
      zip.write_all(&bytes)?;
    }
  }
  Ok(())
}

/// ZIP を同じ場所のフォルダへ展開する。戻り値は展開先フォルダ。
#[tauri::command]
pub fn extract_zip(path: String) -> Result<String, String> {
  let src = Path::new(&path);
  let parent = src.parent().ok_or("親ディレクトリを特定できません")?;
  let stem = src
    .file_stem()
    .map(|s| s.to_string_lossy().to_string())
    .unwrap_or_else(|| "extracted".into());

  let dest = unique_dir_path(parent, &stem);
  std::fs::create_dir_all(&dest).map_err(|e| format!("作成できません: {e}"))?;

  let file = std::fs::File::open(src).map_err(|e| format!("開けません: {e}"))?;
  let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("ZIP として読めません: {e}"))?;

  for i in 0..zip.len() {
    let mut entry = zip.by_index(i).map_err(|e| format!("読み取り失敗: {e}"))?;

    // `../` を含むエントリで展開先の外へ書き出される（Zip Slip）のを防ぐ。
    // 悪意ある ZIP でなくても、作りの悪いツールが生成することがある。
    let Some(rel) = entry.enclosed_name() else {
      return Err(format!("安全でないパスが含まれています: {}", entry.name()));
    };
    let out = dest.join(rel);

    if entry.is_dir() {
      std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
      continue;
    }
    if let Some(p) = out.parent() {
      std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }

    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).map_err(|e| format!("読み取り失敗: {e}"))?;
    std::fs::write(&out, buf).map_err(|e| format!("{}: {e}", out.display()))?;
  }

  Ok(dest.to_string_lossy().to_string())
}

fn unique_zip_path(dir: &Path, stem: &str) -> PathBuf {
  let first = dir.join(format!("{stem}.zip"));
  if !first.exists() {
    return first;
  }
  for n in 2..10_000 {
    let candidate = dir.join(format!("{stem} ({n}).zip"));
    if !candidate.exists() {
      return candidate;
    }
  }
  first
}

fn unique_dir_path(dir: &Path, stem: &str) -> PathBuf {
  let first = dir.join(stem);
  if !first.exists() {
    return first;
  }
  for n in 2..10_000 {
    let candidate = dir.join(format!("{stem} ({n})"));
    if !candidate.exists() {
      return candidate;
    }
  }
  first
}

#[cfg(test)]
mod tests {
  use super::*;

  fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("filer_zip_test_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
  }
  fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
  }

  #[test]
  fn compresses_a_single_file_and_extracts_it_back() {
    let root = scratch("roundtrip");
    std::fs::write(root.join("hello.txt"), b"contents").unwrap();

    let zip_path = compress_to_zip(vec![s(&root.join("hello.txt"))]).unwrap();
    assert!(zip_path.ends_with("hello.zip"));

    // 展開してから中身が一致すること。
    let out = extract_zip(zip_path).unwrap();
    let restored = Path::new(&out).join("hello.txt");
    assert_eq!(std::fs::read(restored).unwrap(), b"contents");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn compresses_a_directory_recursively() {
    let root = scratch("dir");
    std::fs::create_dir_all(root.join("src/nested")).unwrap();
    std::fs::write(root.join("src/a.txt"), b"a").unwrap();
    std::fs::write(root.join("src/nested/b.txt"), b"b").unwrap();

    let zip_path = compress_to_zip(vec![s(&root.join("src"))]).unwrap();
    let out = extract_zip(zip_path).unwrap();

    assert_eq!(std::fs::read(Path::new(&out).join("src/a.txt")).unwrap(), b"a");
    assert_eq!(
      std::fs::read(Path::new(&out).join("src/nested/b.txt")).unwrap(),
      b"b"
    );
    let _ = std::fs::remove_dir_all(&root);
  }

  /// 既存を黙って上書きしないこと。ファイラで一番怖いのがそれ。
  #[test]
  fn does_not_overwrite_an_existing_zip() {
    let root = scratch("collide");
    std::fs::write(root.join("data.txt"), b"x").unwrap();
    std::fs::write(root.join("data.zip"), b"existing archive").unwrap();

    let created = compress_to_zip(vec![s(&root.join("data.txt"))]).unwrap();

    assert!(created.ends_with("data (2).zip"));
    assert_eq!(
      std::fs::read(root.join("data.zip")).unwrap(),
      b"existing archive",
      "既存はそのまま残るべき"
    );
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn extract_does_not_collide_with_existing_folder() {
    let root = scratch("extract_collide");
    std::fs::write(root.join("pack.txt"), b"x").unwrap();
    let zip_path = compress_to_zip(vec![s(&root.join("pack.txt"))]).unwrap();
    std::fs::create_dir_all(root.join("pack")).unwrap();
    std::fs::write(root.join("pack/keep.txt"), b"important").unwrap();

    let out = extract_zip(zip_path).unwrap();

    assert!(out.ends_with("pack (2)"), "既存フォルダを避けるべき");
    assert!(root.join("pack/keep.txt").exists(), "既存の中身は無事であるべき");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn compressing_nothing_is_an_error() {
    assert!(compress_to_zip(vec![]).is_err());
  }
}
