const COMMANDS: &[&str] = &["start", "stop", "open_settings", "register_listener", "remove_listener"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
