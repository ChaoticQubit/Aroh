use serde::de::DeserializeOwned;
use serde::Serialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_aroh_ios);

/// Rust handle to the Swift `ArohPlugin`: HealthKit (`health_authorize`, `health_fetch`,
/// `health_seed` in debug builds), remote notification registration (`push_register`), the
/// next tapped run or proposal push (`push_tap_next`, held until one arrives), cancelling
/// every pending reminder (`reminders_cancel`), location (`location_*`) and the iCloud
/// Keychain diary key (`diary_key_get`, `diary_key_put`, `diary_key_delete`) and attached image
/// text (`image_ocr` with Apple Vision, `image_copy` for a downscaled JPEG).
/// The webview cannot call it; only the app's Rust commands do.
pub struct Ios<R: Runtime> {
    #[cfg(target_os = "ios")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "ios"))]
    _runtime: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> Ios<R> {
    /// Runs Swift command `command` with `args` and decodes its answer into `T`.
    /// Errors with the Swift rejection message, or `unsupported` off iOS.
    /// Blocks only the calling task, never the main thread.
    pub async fn call<T: DeserializeOwned>(&self, command: &str, args: impl Serialize) -> Result<T, String> {
        #[cfg(target_os = "ios")]
        return self.handle.run_mobile_plugin_async(command, args).await.map_err(|e| e.to_string());
        #[cfg(not(target_os = "ios"))]
        {
            let _ = (command, args);
            Err("unsupported: Apple Health and push are only available on iPhone".into())
        }
    }
}

/// Access to the [`Ios`] handle from any Tauri manager (app, handle, window).
/// Panics if the plugin was not registered with [`init`].
pub trait IosExt<R: Runtime> {
    fn ios(&self) -> &Ios<R>;
}

impl<R: Runtime, M: Manager<R>> IosExt<R> for M {
    fn ios(&self) -> &Ios<R> {
        self.state::<Ios<R>>().inner()
    }
}

/// Builds the `aroh-ios` plugin. On iOS it registers the Swift `ArohPlugin` and manages
/// its handle; elsewhere it manages a stand-in whose calls reject with `unsupported`.
/// It exposes no webview commands.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("aroh-ios")
        .setup(|app, _api| {
            #[cfg(target_os = "ios")]
            let ios = Ios { handle: _api.register_ios_plugin(init_plugin_aroh_ios)? };
            #[cfg(not(target_os = "ios"))]
            let ios: Ios<R> = Ios { _runtime: std::marker::PhantomData };
            app.manage(ios);
            Ok(())
        })
        .build()
}
