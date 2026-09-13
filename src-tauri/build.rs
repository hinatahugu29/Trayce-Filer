fn main() {
  // Tauri embeds these files into the Windows executable and default window icon.
  // Track them explicitly so changing only the artwork rebuilds that resource.
  println!("cargo:rerun-if-changed=icons/icon.ico");
  println!("cargo:rerun-if-changed=icons/icon.png");
  tauri_build::build()
}
