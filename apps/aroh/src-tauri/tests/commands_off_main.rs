use std::path::Path;

const NOT_LOCKING: &[&str] = &["app_realtime_up"];

fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Finds every `#[tauri::command]` in `src` and returns the names of those Tauri would
/// run on the main thread: neither `async fn` nor `#[tauri::command(async)]`.
/// Output: `file: name` per main-thread command.
fn main_thread_commands() -> Vec<String> {
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    assert!(!files.is_empty(), "no sources found");
    let mut found = Vec::new();
    let mut commands = 0;
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            let attr = line.trim();
            if !attr.starts_with("#[tauri::command") {
                continue;
            }
            commands += 1;
            let signature = lines.by_ref().map(str::trim).find(|l| l.contains("fn ")).expect("command attribute without fn");
            let name = signature.split("fn ").nth(1).unwrap().split(['(', '<']).next().unwrap().trim().to_string();
            let off_main = signature.contains("async fn") || attr.contains("async");
            if !off_main && !NOT_LOCKING.contains(&name.as_str()) {
                found.push(format!("{}: {name}", file.file_name().unwrap().to_string_lossy()));
            }
        }
    }
    assert!(commands > 40, "expected the app's commands, found {commands}");
    found
}

#[test]
fn no_command_runs_on_the_main_thread() {
    let found = main_thread_commands();
    assert!(found.is_empty(), "commands Tauri runs on the main thread (B90), add `async` or `#[tauri::command(async)]`: {found:#?}");
}
