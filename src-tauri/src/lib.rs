#[macro_use]
mod i18n;
mod config;
mod engine;
mod keys;
mod midi;
mod osc;
mod padkontrol;
mod tray;
#[cfg(all(windows, feature = "windows-midi-services"))]
mod winmidi;

use config::{Profile, ProfileStore, ProfileSummary};
use engine::EngineState;
use midi::{ConnectOptions, ConnectionInfo, PadConnection};
use tauri::{AppHandle, Manager, RunEvent, State};

/// App identifier before 0.1.0 was packaged; it ended in `.app`, which
/// clashes with macOS bundle names.
const LEGACY_IDENTIFIER: &str = "pro.totalkontrol.app";

fn load(store: &ProfileStore, file: &str) -> Result<Profile, String> {
    store.load(file).map_err(|errors| {
        let errors = errors.join("\n");
        t!("core.app.invalidProfile", file, errors)
    })
}

/// Runs blocking MIDI work off the main thread. Opening ports, and on Windows
/// waiting for the MIDI service, can take seconds and would freeze the window.
/// Gives up after `MIDI_TIMEOUT` if the system MIDI service hangs, and while
/// work is stuck there, refuses more at once (see `midi::with_timeout`).
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        midi::with_timeout(MIDI_TIMEOUT, f).unwrap_or_else(|| Err(midi::midi_service_frozen()))
    })
    .await
    .map_err(|e| e.to_string())?
}

const MIDI_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Language of the messages produced by the Rust core, sent by the UI.
#[tauri::command]
fn set_locale(app: AppHandle, lang: i18n::Lang) {
    i18n::set(lang);
    tray::relabel(&app);
}

#[tauri::command]
async fn list_midi_ports() -> Result<midi::MidiPorts, String> {
    blocking(midi::list_ports).await
}

#[tauri::command]
async fn connect_pad(
    app: AppHandle,
    options: ConnectOptions,
    profile: Option<String>,
) -> Result<ConnectionInfo, String> {
    blocking(move || {
        let profile = profile
            .map(|file| load(&app.state::<ProfileStore>(), &file).map(|profile| (file, profile)))
            .transpose()?;
        app.state::<PadConnection>().connect(&app, options, profile)
    })
    .await
}

// Commands touching the connection run on a thread pool, never on the main
// thread: they can wait for a connection being opened or closed.

#[tauri::command(async)]
fn disconnect_pad(app: AppHandle, connection: State<PadConnection>) {
    connection.disconnect(&app);
}

/// All notes off, in the DAW and on the keyboard.
#[tauri::command(async)]
fn panic(connection: State<PadConnection>) -> Result<(), String> {
    connection.panic()
}

#[tauri::command(async)]
fn connection_status(connection: State<PadConnection>) -> midi::ConnectionState {
    connection.state()
}

#[tauri::command(async)]
fn engine_state(connection: State<PadConnection>) -> Option<EngineState> {
    connection.engine_state()
}

#[tauri::command(async)]
fn set_profile(
    connection: State<PadConnection>,
    store: State<ProfileStore>,
    file: String,
) -> Result<EngineState, String> {
    connection.set_profile(&file, load(&store, &file)?)
}

#[tauri::command(async)]
fn select_bank(connection: State<PadConnection>, bank: usize) -> Result<EngineState, String> {
    connection.select_bank(bank)
}

#[tauri::command]
fn list_profiles(store: State<ProfileStore>) -> Result<Vec<ProfileSummary>, String> {
    store.list()
}

#[tauri::command]
fn load_profile(store: State<ProfileStore>, file: String) -> Result<Profile, Vec<String>> {
    store.load(&file)
}

#[tauri::command]
fn profiles_dir(store: State<ProfileStore>) -> String {
    store.dir().display().to_string()
}

/// Errors of a profile being edited, without saving it.
#[tauri::command]
fn validate_profile(profile: serde_json::Value) -> Vec<String> {
    config::parse_value(profile).err().unwrap_or_default()
}

/// Saves a profile. When it is the one running, the engine picks up the
/// changes right away and the new engine state is returned.
#[tauri::command(async)]
fn save_profile(
    connection: State<PadConnection>,
    store: State<ProfileStore>,
    file: String,
    profile: serde_json::Value,
) -> Result<Option<EngineState>, Vec<String>> {
    let profile = config::parse_value(profile)?;
    store.save(&file, profile.clone())?;
    if connection.active_file().as_deref() == Some(file.as_str()) {
        return connection.set_profile(&file, profile).map(Some).map_err(|e| vec![e]);
    }
    Ok(None)
}

#[tauri::command]
fn create_profile(store: State<ProfileStore>, name: String) -> Result<String, String> {
    store.create(&name)
}

#[tauri::command]
fn duplicate_profile(store: State<ProfileStore>, file: String) -> Result<String, String> {
    store.duplicate(&file)
}

#[tauri::command(async)]
fn delete_profile(connection: State<PadConnection>, store: State<ProfileStore>, file: String) -> Result<(), String> {
    if connection.active_file().as_deref() == Some(file.as_str()) {
        return Err(t!("core.app.profileInUse"));
    }
    store.delete(&file)
}

/// Release builds abort on panic, skipping `RunEvent::Exit`: hands the
/// padKONTROL back and ends the notes first, or the padKONTROL would stay
/// silent until power-cycled and the DAW keep sounding.
fn release_on_panic(app: AppHandle) {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        app.state::<PadConnection>().release_on_panic();
        report(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    i18n::init_from_system();
    tauri::Builder::default()
        .manage(PadConnection::default())
        .setup(|app| {
            let dir = app.path().app_config_dir()?.join("profiles");
            // Profiles saved before the identifier change live under the old one.
            let legacy = app.path().config_dir()?.join(LEGACY_IDENTIFIER).join("profiles");
            let (store, copied) = ProfileStore::open_migrating(dir, &legacy)?;
            if copied > 0 {
                println!("{copied} profil(s) repris de {}", legacy.display());
            }
            app.manage(store);
            release_on_panic(app.handle().clone());
            tray::setup(app.handle());
            midi::spawn_watchdog(app.handle().clone());
            Ok(())
        })
        .on_window_event(tray::on_window_event)
        .invoke_handler(tauri::generate_handler![
            set_locale,
            list_midi_ports,
            connect_pad,
            disconnect_pad,
            panic,
            connection_status,
            engine_state,
            set_profile,
            select_bank,
            list_profiles,
            load_profile,
            profiles_dir,
            validate_profile,
            save_profile,
            create_profile,
            duplicate_profile,
            delete_profile
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Returns the padKONTROL to its normal mode when the app closes.
            if let RunEvent::Exit = event {
                app.state::<PadConnection>().shut_down(app);
            }
        });
}
