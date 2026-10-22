#[macro_use]
mod i18n;
mod config;
mod diag;
mod engine;
mod keys;
mod midi;
mod osc;
mod padkontrol;
mod scenes;
mod tray;
#[cfg(all(windows, feature = "windows-midi-services"))]
mod winmidi;

use config::{Profile, ProfileStore, ProfileSummary};
use engine::EngineState;
use midi::{ConnectOptions, ConnectionInfo, PadConnection};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};

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
    blocking_for(MIDI_TIMEOUT, f).await
}

async fn blocking_for<T: Send + 'static>(
    timeout: std::time::Duration,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        midi::with_timeout(timeout, f).unwrap_or_else(|| Err(midi::midi_service_frozen()))
    })
    .await
    .map_err(|e| e.to_string())?
}

const MIDI_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Scene work: up to 48 exchanges (a restore reads the 16 scenes for a
/// backup, then loads and writes them) of `midi::EXCHANGE_TIMEOUT` each.
const SCENES_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

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

#[tauri::command]
fn list_presets() -> Vec<config::PresetSummary> {
    config::presets()
}

#[tauri::command(async)]
fn create_from_preset(store: State<ProfileStore>, id: String) -> Result<String, String> {
    store.create_from_preset(&id)
}

/// Picks a profile file anywhere and adds it to the store. `None` when the
/// user cancels. The dialog runs from here: the page needs no file access.
#[tauri::command(async)]
fn import_profile(app: AppHandle, store: State<ProfileStore>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app.dialog().file().add_filter("JSON", &["json"]).blocking_pick_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    store.import(&path).map(Some)
}

/// Saves a copy of a profile where the user picks. `None` when cancelled.
#[tauri::command(async)]
fn export_profile(app: AppHandle, store: State<ProfileStore>, file: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(&file)
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    store.export(&file, &path)?;
    Ok(Some(path.display().to_string()))
}

/// Saves what helps understand a problem: versions, MIDI ports, the
/// connection and the end of the log. `None` when cancelled.
#[tauri::command(async)]
fn export_diagnostics(app: AppHandle) -> Result<Option<String>, String> {
    use std::fmt::Write;
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("Text", &["txt"])
        .set_file_name(format!("totalkontrol-diagnostics-{}.txt", diag::file_stamp()))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let mut text = String::new();
    let _ = writeln!(text, "totalKONTROL {}", app.package_info().version);
    let _ = writeln!(text, "System: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    let _ = writeln!(text, "Date: {}\n", diag::now());
    match midi::list_ports() {
        Ok(ports) => {
            let names = |list: &[midi::MidiPort]| list.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
            let _ = writeln!(text, "MIDI inputs: {}", names(&ports.inputs));
            let _ = writeln!(text, "MIDI outputs: {}", names(&ports.outputs));
        }
        Err(error) => {
            let _ = writeln!(text, "MIDI ports: {error}");
        }
    }
    let state = app.state::<PadConnection>().state();
    let _ = writeln!(text, "Connection: {:?}", state.connection);
    let _ = writeln!(text, "Waiting: {}\n\nLog:\n{}", state.waiting, diag::tail(64 * 1024));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(Some(path.display().to_string()))
}

/// Opens the releases page, from the update notice.
#[tauri::command(async)]
fn open_releases(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url("https://github.com/PoUpA/totalKONTROL/releases/latest", None::<&str>)
        .map_err(|e| e.to_string())
}

/// Shows the profiles folder in the file manager.
#[tauri::command(async)]
fn open_profiles_dir(app: AppHandle, store: State<ProfileStore>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(store.dir().display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Scenes last read from the padKONTROL, for a backup or a new profile.
#[derive(Default)]
struct ReadScenes(std::sync::Mutex<Vec<scenes::Scene>>);

/// Reads the padKONTROL's 16 scenes and returns them as profile banks.
#[tauri::command]
async fn read_scenes(app: AppHandle) -> Result<Vec<config::Bank>, String> {
    blocking_for(SCENES_TIMEOUT, move || {
        let read = app.state::<PadConnection>().read_scenes()?;
        let banks = (1..)
            .zip(&read)
            .map(|(n, scene)| scenes::to_bank(&scene.data, t!("core.scenes.bankName", n)))
            .collect();
        *app.state::<ReadScenes>().0.lock().unwrap() = read;
        Ok(banks)
    })
    .await
}

/// A new profile with one bank per scene read; returns its file name.
#[tauri::command(async)]
fn scenes_to_profile(read: State<ReadScenes>, store: State<ProfileStore>, name: String) -> Result<String, String> {
    let read = read.0.lock().unwrap();
    if read.is_empty() {
        return Err(t!("core.scenes.readFirst"));
    }
    let banks = (1..)
        .zip(read.iter())
        .map(|(n, scene)| {
            let mut bank = scenes::to_bank(&scene.data, t!("core.scenes.bankName", n));
            bank.display = Some(format!("S{n:02}"));
            bank
        })
        .collect();
    store.create_with_banks(&name, banks)
}

/// Saves the scenes read to a backup file; returns its name.
#[tauri::command(async)]
fn backup_scenes(read: State<ReadScenes>, backups: State<scenes::Backups>) -> Result<String, String> {
    let read = read.0.lock().unwrap();
    if read.len() != usize::from(scenes::SCENE_COUNT) {
        return Err(t!("core.scenes.readFirst"));
    }
    backups.save(&read)
}

#[tauri::command(async)]
fn list_scene_backups(backups: State<scenes::Backups>) -> Vec<String> {
    backups.list()
}

#[tauri::command(async)]
fn scene_backups_dir(backups: State<scenes::Backups>) -> String {
    backups.dir().display().to_string()
}

/// Stores a backup's 16 scenes into the padKONTROL, for good, after saving
/// the scenes it replaces to a new backup; returns that backup's name.
#[tauri::command]
async fn restore_scenes(app: AppHandle, file: String) -> Result<String, String> {
    blocking_for(SCENES_TIMEOUT, move || {
        let backups = app.state::<scenes::Backups>();
        let backup = backups.load(&file)?;
        app.state::<PadConnection>()
            .restore_scenes(&backup, |replaced| backups.save(replaced))
    })
    .await
}

/// Writes a bank of a saved profile into one of the padKONTROL's scenes;
/// returns what the scene could not take.
#[tauri::command]
async fn write_bank_to_scene(
    app: AppHandle,
    file: String,
    bank: usize,
    scene: u8,
) -> Result<Vec<scenes::Left>, String> {
    blocking_for(SCENES_TIMEOUT, move || {
        let profile = load(&app.state::<ProfileStore>(), &file)?;
        let bank = profile
            .banks
            .get(bank)
            .ok_or_else(|| t!("core.validation.missingBank", bank))?;
        if scene >= scenes::SCENE_COUNT {
            return Err(t!("core.scenes.noSuchScene", scene));
        }
        app.state::<PadConnection>().write_bank_to_scene(bank, scene)
    })
    .await
}

/// Quits, from the window once it agreed (see `tray::request_quit`). Exits
/// through RunEvent::Exit, which returns the padKONTROL to normal mode.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn cancel_quit() {
    tray::cancel_quit();
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
        diag::log(format!("panic: {info}"));
        app.state::<PadConnection>().release_on_panic();
        report(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    i18n::init_from_system();
    tauri::Builder::default()
        .manage(PadConnection::default())
        // Used from the Rust side only: the page has no dialog or opener permission.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?.join("profiles");
            // Profiles saved before the identifier change live under the old one.
            let legacy = app.path().config_dir()?.join(LEGACY_IDENTIFIER).join("profiles");
            let (store, copied) = ProfileStore::open_migrating(dir, &legacy)?;
            if copied > 0 {
                println!("{copied} profil(s) repris de {}", legacy.display());
            }
            let window = app.handle().clone();
            diag::init(app.path().app_log_dir()?, move |problem| {
                let _ = window.emit("problem", problem);
            });
            diag::log(format!("totalKONTROL {} started", app.package_info().version));
            app.manage(store);
            app.manage(scenes::Backups::new(app.path().app_config_dir()?.join("scene-backups")));
            app.manage(ReadScenes::default());
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
            read_scenes,
            scenes_to_profile,
            backup_scenes,
            list_scene_backups,
            scene_backups_dir,
            restore_scenes,
            write_bank_to_scene,
            panic,
            quit,
            cancel_quit,
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
            delete_profile,
            list_presets,
            create_from_preset,
            import_profile,
            export_profile,
            open_profiles_dir,
            export_diagnostics,
            open_releases
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
