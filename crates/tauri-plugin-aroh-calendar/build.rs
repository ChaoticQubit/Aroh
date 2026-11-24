const COMMANDS: &[&str] = &["events", "seed"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
