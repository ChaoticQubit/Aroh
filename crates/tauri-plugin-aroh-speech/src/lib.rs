use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_aroh_speech);

/// Error payload every platform rejects with, so the guest code sees one shape.
/// `code` is one of `unsupported`, `mic_denied`, `speech_denied`, `on_device_unavailable`,
/// `recognizer_unavailable`, `busy`, `not_started` or `audio_failed`.
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

/// Builds the `aroh-speech` plugin. On iOS the `start` and `stop` commands are
/// served by the Swift `SpeechPlugin` (on-device `SFSpeechRecognizer`); on every
/// other target they reject with code `unsupported`.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    let builder = Builder::new("aroh-speech");
    #[cfg(target_os = "ios")]
    let builder = builder.setup(|_app, api| {
        api.register_ios_plugin(init_plugin_aroh_speech)?;
        Ok(())
    });
    #[cfg(not(target_os = "ios"))]
    let builder = builder.invoke_handler(tauri::generate_handler![start, stop]);
    builder.build()
}
