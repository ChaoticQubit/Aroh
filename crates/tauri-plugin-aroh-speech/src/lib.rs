use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_aroh_speech);

/// Error payload every platform rejects with, so the guest code sees one shape.
/// `code` is one of `unsupported`, `mic_denied`, `speech_denied`, `on_device_unavailable`,
/// `recognizer_unavailable`, `busy`, `not_started`, `audio_failed` or `superseded` (a newer start took over).
#[cfg(not(target_os = "ios"))]
#[derive(Debug, serde::Serialize)]
struct SpeechError {
    code: &'static str,
    message: &'static str,
}

#[cfg(not(target_os = "ios"))]
const UNSUPPORTED: SpeechError = SpeechError {
    code: "unsupported",
    message: "On-device voice input is only available on iPhone for now.",
};

/// Desktop and Android stand-in for the Swift `start`: always rejects with `unsupported`.
#[cfg(not(target_os = "ios"))]
#[tauri::command]
fn start() -> Result<(), SpeechError> {
    Err(UNSUPPORTED)
}

/// Desktop and Android stand-in for the Swift `stop`: always rejects with `unsupported`.
#[cfg(not(target_os = "ios"))]
#[tauri::command]
fn stop() -> Result<String, SpeechError> {
    Err(UNSUPPORTED)
}

/// Desktop and Android stand-in for the Swift `open_settings`: always rejects with `unsupported`.
#[cfg(not(target_os = "ios"))]
#[tauri::command]
fn open_settings() -> Result<(), SpeechError> {
    Err(UNSUPPORTED)
}

/// Builds the `aroh-speech` plugin. On iOS `start`, `stop` and `open_settings` are
/// served by the Swift `SpeechPlugin` (on-device `SFSpeechRecognizer`, partial words as
/// `partial` events); on every other target they reject with code `unsupported`.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    let builder = Builder::new("aroh-speech");
    #[cfg(target_os = "ios")]
    let builder = builder.setup(|_app, api| {
        api.register_ios_plugin(init_plugin_aroh_speech)?;
        Ok(())
    });
    #[cfg(not(target_os = "ios"))]
    let builder = builder.invoke_handler(tauri::generate_handler![start, stop, open_settings]);
    builder.build()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_default_permission_lets_the_app_hear_partial_words_and_open_settings() {
        let default = include_str!("../permissions/default.toml");
        for permission in ["allow-start", "allow-stop", "allow-open-settings", "allow-register-listener", "allow-remove-listener"] {
            assert!(default.contains(&format!("\"{permission}\"")), "{permission} missing");
        }
    }
}
