fn main() {
    println!("cargo:rerun-if-env-changed=MUSIC_CHAPTER_EDITOR_CONTACT");
    tauri_build::build()
}
