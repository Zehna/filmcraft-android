//! FilmCraft on Android — M1 minimal shell.
//!
//! GameActivity + eframe/wgpu, reusing the desktop engine and egui UI untouched: the same
//! `filmcraft_engine::Session` and `filmcraft_ui_egui::FilmcraftApp` the desktop shell builds.
//! What the desktop shell gets from the OS, this crate takes from the activity:
//!
//! | Desktop (`apps/filmcraft`) | Android (this crate, M1) |
//! |---|
//! | `std::env::args()` launch flags | [`winit::platform::android::activity::AndroidApp`] |
//! | data dir from `default_data_dir()` | [`AndroidApp::internal_data_path`] |
//! | rfd file dialogs | not wired yet (M2: SAF bridge; hooks stay at their defaults) |
//! | cpal audio output | not wired yet (M2: AAudio; `app.audio` stays `None`, playback uses the wall clock) |
//! | VideoToolbox hardware decode | not wired yet (M2: MediaCodec in `crates/platform`) |
//! | desktop-sized UI | the desktop layout as-is (M3: phone workspace) |
//!
//! Deliberately minimal: the M1 gate is only that the APK installs, the app opens, the UI
//! renders and the demo project loads. SAF import/save, IME/keyboard, the phone workspace,
//! audio playback and MediaCodec are M2/M3 work and are not stubbed in disguise here — the
//! corresponding hooks are simply left at `HostHooks::default()`.
//!
//! Provisional identity (NOT final): package `com.taras.filmcraft`, working label "FilmCraft".
//! The ArtCraft name and logos are not used anywhere in this shell.
#![cfg(target_os = "android")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

use filmcraft_engine::Session;
use filmcraft_engine::autosave::AutosaveConfig;
use filmcraft_ui_egui::FilmcraftApp;
use serde_json::json;
use winit::platform::android::activity::AndroidApp;

/// Android entry point, called by GameActivity (manifest `android.app.lib_name=main`).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info));
    log::info!("filmcraft-android: android_main entered (M1 minimal shell)");
    // OS hardware video decoders in front of our own (none registered on Android yet; the
    // pure-Rust decoders stay the fallback, exactly as on a desktop without hardware support).
    let hardware = filmcraft_platform::register();
    log::info!("filmcraft-android: hardware decoding: {hardware:?}");
    // Application-private storage; used for auto-save, crash logs and preferences.
    let data_dir = app.internal_data_path().map(|p| p.to_path_buf());
    match &data_dir {
        Some(dir) => log::info!("filmcraft-android: data dir {}", dir.display()),
        None => log::warn!("filmcraft-android: no internal data path; auto-save disabled"),
    }
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    let started = eframe::run_native(
        "FilmCraft",
        options,
        Box::new(move |cc| {
            filmcraft_ui_egui::install_ime_reset_callbacks(&cc.egui_ctx);
            // Panics anywhere go to <data dir>/Logs/crash-<day>.log with a backtrace, as on desktop.
            let log_dir = data_dir.as_ref().map(|d| d.join("Logs"));
            filmcraft_ui_egui::crash::install(log_dir);
            let mut session = Session::default();
            if let Some(dir) = data_dir.clone() {
                if let Err(e) = session.start_autosave(AutosaveConfig::new(dir)) {
                    log::warn!("filmcraft-android: auto-save unavailable: {e}");
                }
            }
            // M1 gate: open the demo project so the timeline and monitors have something to show.
            match session.execute("file.openDemoProject", json!({})) {
                Ok(_) => log::info!("filmcraft-android: demo project opened"),
                Err(e) => log::warn!("filmcraft-android: demo project failed to open: {e}"),
            }
            let mut filmcraft_app = FilmcraftApp::new(session);
            if let Some(rs) = cc.wgpu_render_state.clone() {
                filmcraft_app.set_wgpu(rs);
            }
            Ok(Box::new(filmcraft_app))
        }),
    );
    if let Err(e) = started {
        log::error!("filmcraft-android: eframe exited with error: {e}");
    }
}
