//! Android foreground service that keeps the process alive for the duration of
//! a sync, so a swipe-away mid-sync doesn't lose the run. Gated behind the
//! per-device "Background sync (Android)" opt-in; a no-op on desktop.

use tauri::{AppHandle, Manager, Runtime};

/// How long the service may live before it self-destructs, as a backstop for a
/// stop call that never lands (e.g. the Activity was destroyed by a swipe, so
/// the JNI round-trip can't reach it). Comfortably longer than any real note
/// sync, short enough that a leaked notification doesn't linger for long.
pub const WATCHDOG_MS: u64 = 3 * 60 * 1000;

/// Reads the "Background sync (Android)" opt-in straight from settings.json —
/// the same file the frontend's settings store persists to.
fn background_sync_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Ok(base) = app.path().app_data_dir() else {
        return false;
    };
    let Ok(raw) = std::fs::read_to_string(base.join("settings.json")) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| {
            v.get("backgroundSyncAndroid")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false)
}

#[cfg(target_os = "android")]
mod imp {
    use serde::Serialize;
    use tauri::{
        plugin::{Builder, PluginApi, PluginHandle, TauriPlugin},
        AppHandle, Manager, Runtime,
    };

    /// Must equal the `package` of SyncServicePlugin.kt.
    const PLUGIN_IDENTIFIER: &str = "com.mh968.note_manager";
    /// Must equal the Kotlin class simple name.
    const PLUGIN_CLASS: &str = "SyncServicePlugin";
    /// Routing key for Rust -> Kotlin commands.
    const PLUGIN_NAME: &str = "sync-service";

    pub struct SyncService<R: Runtime>(PluginHandle<R>);

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct StartArgs {
        timeout_ms: u64,
    }

    pub fn init<R: Runtime>() -> TauriPlugin<R> {
        Builder::new(PLUGIN_NAME)
            .setup(|app, api: PluginApi<R, ()>| {
                let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, PLUGIN_CLASS)?;
                app.manage(SyncService(handle));
                Ok(())
            })
            .build()
    }

    pub fn start<R: Runtime>(app: &AppHandle<R>, timeout_ms: u64) -> Result<(), String> {
        app.state::<SyncService<R>>()
            .0
            .run_mobile_plugin::<()>("startSync", StartArgs { timeout_ms })
            .map_err(|e| format!("failed to start sync foreground service: {e}"))
    }

    pub fn stop<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
        app.state::<SyncService<R>>()
            .0
            .run_mobile_plugin::<()>("stopSync", ())
            .map_err(|e| format!("failed to stop sync foreground service: {e}"))
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use tauri::{
        plugin::{Builder, TauriPlugin},
        AppHandle, Runtime,
    };

    pub fn init<R: Runtime>() -> TauriPlugin<R> {
        Builder::new("sync-service").build()
    }

    pub fn start<R: Runtime>(_app: &AppHandle<R>, _timeout_ms: u64) -> Result<(), String> {
        Ok(())
    }

    pub fn stop<R: Runtime>(_app: &AppHandle<R>) -> Result<(), String> {
        Ok(())
    }
}

pub use imp::init;

/// RAII: starts the foreground service now (only when the Android opt-in is on)
/// and stops it on drop — covering the normal return, `?`, and panic. Nothing
/// happens on desktop or when the option is off.
pub struct SyncServiceGuard<R: Runtime> {
    app: AppHandle<R>,
    active: bool,
}

impl<R: Runtime> SyncServiceGuard<R> {
    pub fn start(app: &AppHandle<R>) -> Self {
        if !background_sync_enabled(app) {
            return Self {
                app: app.clone(),
                active: false,
            };
        }
        let active = match imp::start(app, WATCHDOG_MS) {
            Ok(()) => true,
            // Most likely Android's background-start restriction (the app wasn't
            // in the foreground). The sync still runs, just unprotected.
            Err(e) => {
                eprintln!("{e}");
                false
            }
        };
        Self {
            app: app.clone(),
            active,
        }
    }
}

impl<R: Runtime> Drop for SyncServiceGuard<R> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let app = self.app.clone();
        // Detached on purpose: the stop round-trip is a JNI call routed through
        // the Activity-bound event loop, which can stall or panic if the task
        // was swiped away mid-sync. The service's own watchdog is the backstop.
        std::thread::spawn(move || {
            let _ = imp::stop(&app);
        });
    }
}
