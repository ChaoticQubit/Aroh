use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_aroh_calendar);

/// Error payload every platform rejects with, so the guest code sees one shape.
/// `code` is one of `unsupported`, `calendar_denied` or `bad_args`.
/// Messages are safe to show.
#[cfg(not(target_os = "ios"))]
#[derive(Debug, serde::Serialize)]
struct CalendarError {
    code: &'static str,
    message: &'static str,
}

#[cfg(not(target_os = "ios"))]
const UNSUPPORTED: CalendarError = CalendarError {
    code: "unsupported",
    message: "Device calendars are only read on iPhone for now.",
};

/// Desktop and Android stand-in for the Swift `events`: always rejects with `unsupported`.
/// The Swift side reads occurrences between `from` and `to` after asking for access.
/// Never writes.
#[cfg(not(target_os = "ios"))]
#[tauri::command]
fn events() -> Result<(), CalendarError> {
    Err(UNSUPPORTED)
}

/// Desktop and Android stand-in for the Swift `seed`: always rejects with `unsupported`.
/// The Swift side writes test events only in simulator debug builds.
/// Release builds reject it everywhere.
#[cfg(not(target_os = "ios"))]
#[tauri::command]
fn seed() -> Result<(), CalendarError> {
    Err(UNSUPPORTED)
}

/// Builds the `aroh-calendar` plugin. On iOS `events` is served by the Swift
/// `CalendarPlugin` (EventKit, read only); on every other target both commands
/// reject with code `unsupported`.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    let builder = Builder::new("aroh-calendar");
    #[cfg(target_os = "ios")]
    let builder = builder.setup(|_app, api| {
        api.register_ios_plugin(init_plugin_aroh_calendar)?;
        Ok(())
    });
    #[cfg(not(target_os = "ios"))]
    let builder = builder.invoke_handler(tauri::generate_handler![events, seed]);
    builder.build()
}
