use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{Profile, ProfileStore};
use crate::engine::{Engine, EngineState};
use crate::keys::{self, KeyEvent};
use crate::osc::{self, OscMessage, OscSender};
use crate::padkontrol::{self, ControlEvent};
#[cfg(all(windows, feature = "windows-midi-services"))]
use crate::winmidi;

const CLIENT_NAME: &str = "totalKONTROL";
const PADKONTROL_HINT: &str = "padkontrol";
const DISPLAY_TEXT: &str = "TK ";
pub const MESSAGE_EVENT: &str = "midi-message";
pub const ENGINE_EVENT: &str = "engine-state";

/// Names of the virtual ports the app creates. On Windows, a single
/// bidirectional device carries both directions.
const VIRTUAL_OUTPUT_NAME: &str = "totalKONTROL";
#[cfg(not(windows))]
const VIRTUAL_FEEDBACK_NAME: &str = "totalKONTROL feedback";
#[cfg(windows)]
const VIRTUAL_FEEDBACK_NAME: &str = VIRTUAL_OUTPUT_NAME;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiPort {
    pub name: String,
    pub is_pad_kontrol: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiPorts {
    pub inputs: Vec<MidiPort>,
    pub outputs: Vec<MidiPort>,
    /// True when the app can create its own ports: macOS and Linux, or
    /// Windows with Windows MIDI Services.
    pub virtual_supported: bool,
    /// Why virtual ports are not offered, when there is a specific reason.
    pub virtual_note: Option<String>,
    pub virtual_output_name: &'static str,
    pub virtual_feedback_name: &'static str,
}

/// How long MIDI port enumeration may take. A frozen Windows MIDI service
/// makes WinMM calls hang forever.
const ENUMERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);
pub fn midi_service_frozen() -> String {
    t!("core.midi.serviceFrozen")
}

fn to_port(name: String) -> MidiPort {
    let is_pad_kontrol = name.to_lowercase().contains(PADKONTROL_HINT);
    MidiPort { name, is_pad_kontrol }
}

/// Whether the app can create virtual ports, and why not when it matters.
fn virtual_support() -> (bool, Option<String>) {
    #[cfg(all(windows, feature = "windows-midi-services"))]
    return match winmidi::availability() {
        winmidi::Availability::Ready => (true, None),
        winmidi::Availability::NeedsWindowsUpdate => (false, Some(winmidi::needs_windows_update())),
        winmidi::Availability::Unavailable => (false, None),
    };
    #[cfg(not(all(windows, feature = "windows-midi-services")))]
    return (cfg!(unix), None);
}

/// Lists the MIDI ports, giving up if the system MIDI service hangs.
pub fn list_ports() -> Result<MidiPorts, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(enumerate_ports());
    });
    rx.recv_timeout(ENUMERATION_TIMEOUT)
        .unwrap_or_else(|_| Err(midi_service_frozen()))
}

/// Names of the MIDI inputs, or `None` if the system MIDI service hangs.
fn input_port_names() -> Option<Vec<String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let names = MidiInput::new(CLIENT_NAME).map(|midi_in| {
            midi_in
                .ports()
                .iter()
                .filter_map(|p| midi_in.port_name(p).ok())
                .collect()
        });
        let _ = tx.send(names);
    });
    rx.recv_timeout(ENUMERATION_TIMEOUT).ok()?.ok()
}

const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
/// When the MIDI service hangs, look again much later rather than piling up
/// stuck threads.
const FROZEN_BACKOFF: std::time::Duration = std::time::Duration::from_secs(30);
/// Time for a padKONTROL just plugged back in to finish starting up.
const SETTLE_TIME: std::time::Duration = std::time::Duration::from_secs(1);
/// How long the padKONTROL has to acknowledge a heartbeat.
const HEARTBEAT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

/// Watches the padKONTROL: when its input port disappears or, in native
/// mode, it stops answering the heartbeat (unplugged), the connection is
/// dropped and its settings kept; when it comes back, the app reconnects with
/// them and reloads the profile.
pub fn spawn_watchdog(app: AppHandle) {
    std::thread::spawn(move || {
        let mut delay = WATCH_INTERVAL;
        loop {
            std::thread::sleep(delay);
            delay = WATCH_INTERVAL;
            let connection = app.state::<PadConnection>();
            let Some(input) = connection.watched_input() else {
                continue;
            };
            let Some(names) = input_port_names() else {
                delay = FROZEN_BACKOFF;
                continue;
            };
            let present = names.contains(&input);
            let connected = connection.info().is_some();
            if connected && (!present || connection.responds() == Some(false)) {
                connection.lose();
                let _ = app.emit(CONNECTION_EVENT, connection.state());
            } else if !connected && present && connection.waiting() {
                std::thread::sleep(SETTLE_TIME);
                let Some(remembered) = connection.remembered.lock().unwrap().clone() else {
                    continue;
                };
                let profile = remembered
                    .profile
                    .and_then(|file| app.state::<ProfileStore>().load(&file).ok().map(|p| (file, p)));
                // A failed attempt keeps waiting: the next round tries again.
                if connection.connect(app.clone(), remembered.options, profile).is_ok() {
                    // Ports still listed but nobody behind them.
                    if connection.responds() == Some(false) {
                        connection.lose();
                    } else {
                        let _ = app.emit(CONNECTION_EVENT, connection.state());
                    }
                }
            }
        }
    });
}

fn enumerate_ports() -> Result<MidiPorts, String> {
    let midi_in = MidiInput::new(CLIENT_NAME).map_err(|e| e.to_string())?;
    let midi_out = MidiOutput::new(CLIENT_NAME).map_err(|e| e.to_string())?;

    let inputs = midi_in
        .ports()
        .iter()
        .filter_map(|p| midi_in.port_name(p).ok())
        .map(to_port)
        .collect();
    let outputs = midi_out
        .ports()
        .iter()
        .filter_map(|p| midi_out.port_name(p).ok())
        .map(to_port)
        .collect();

    let (virtual_supported, virtual_note) = virtual_support();
    Ok(MidiPorts {
        inputs,
        outputs,
        virtual_supported,
        virtual_note,
        virtual_output_name: VIRTUAL_OUTPUT_NAME,
        virtual_feedback_name: VIRTUAL_FEEDBACK_NAME,
    })
}

/// Direction of a message shown in the monitor.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Received from the padKONTROL.
    In,
    /// Sent to the DAW.
    Out,
    /// Received from the DAW, for the LEDs.
    Feedback,
    /// Sent to the OSC destination.
    Osc,
    /// Received on the OSC listening port.
    OscIn,
    /// Keyboard shortcut typed by a `key` action.
    Key,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiMessage {
    /// Time, in milliseconds since the Unix epoch.
    pub received_at: u64,
    pub direction: Direction,
    pub hex: String,
    pub description: String,
    pub event: Option<ControlEvent>,
    /// Encoder position kept by the engine, for encoder events
    /// it handled. The live view shows it so it matches the display.
    pub encoder_position: Option<i16>,
}

impl MidiMessage {
    fn new(direction: Direction, bytes: &[u8]) -> Self {
        Self {
            received_at: now_ms(),
            direction,
            hex: padkontrol::to_hex(bytes),
            description: padkontrol::describe(bytes),
            event: if matches!(direction, Direction::In) {
                padkontrol::decode(bytes)
            } else {
                None
            },
            encoder_position: None,
        }
    }

    fn key(event: &KeyEvent) -> Self {
        Self {
            received_at: now_ms(),
            direction: Direction::Key,
            hex: String::new(),
            description: event.describe(),
            event: None,
            encoder_position: None,
        }
    }

    fn osc(direction: Direction, message: &OscMessage) -> Self {
        Self {
            received_at: now_ms(),
            direction,
            hex: String::new(),
            description: message.describe(),
            event: None,
            encoder_position: None,
        }
    }
}

/// OSC destination picked in the UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OscOptions {
    pub host: String,
    pub port: u16,
    /// Address prefix of the mirror messages, e.g. `/totalkontrol`.
    pub prefix: String,
    /// Send every control to `<prefix>/pad/3`, `<prefix>/knob/1`…
    pub mirror: bool,
    /// UDP port to listen on for LED, display and bank messages.
    #[serde(default)]
    pub listen_port: Option<u16>,
}

/// OSC side of a connection.
struct OscOutput {
    sender: OscSender,
    prefix: String,
    mirror: bool,
}

/// Port picked in the UI: an existing port, or a virtual port.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PortChoice {
    Port(String),
    Virtual,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOptions {
    pub input: String,
    pub output: Option<String>,
    pub native_mode: bool,
    pub daw_output: Option<PortChoice>,
    pub feedback_input: Option<PortChoice>,
    #[serde(default)]
    pub osc: Option<OscOptions>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub input: String,
    pub output: Option<String>,
    pub native_mode: bool,
    pub daw_output: Option<String>,
    pub feedback_input: Option<String>,
    /// OSC destination, as `host:port`.
    pub osc: Option<String>,
    /// UDP port the app listens on for OSC.
    pub osc_listen: Option<u16>,
}

/// Where messages for the DAW go.
enum DawOutput {
    Midir(MidiOutputConnection),
    #[cfg(all(windows, feature = "windows-midi-services"))]
    Windows(Arc<winmidi::VirtualDevice>),
}

impl DawOutput {
    fn send(&mut self, message: &[u8]) -> bool {
        match self {
            DawOutput::Midir(out) => out.send(message).is_ok(),
            #[cfg(all(windows, feature = "windows-midi-services"))]
            DawOutput::Windows(device) => device.send(message),
        }
    }
}

/// Where messages from the DAW come from. Dropping it stops listening.
enum FeedbackInput {
    Midir(#[allow(dead_code)] MidiInputConnection<()>),
    #[cfg(all(windows, feature = "windows-midi-services"))]
    Windows(#[allow(dead_code)] winmidi::Listener),
}

/// State shared by the MIDI input callbacks and the commands.
#[derive(Default)]
struct Shared {
    engine: Option<Engine>,
    pad_out: Option<MidiOutputConnection>,
    daw_out: Option<DawOutput>,
    osc: Option<OscOutput>,
    /// Reporting setup acknowledgements received, for the heartbeat.
    acks: u32,
}

impl Shared {
    /// Sends the engine's OSC messages, plus the mirror of `event` when the
    /// mirror is on, and returns them for the monitor.
    fn send_osc(
        &mut self,
        messages: &[OscMessage],
        event: Option<&ControlEvent>,
        encoder_position: Option<i16>,
        bank: Option<usize>,
    ) -> Vec<MidiMessage> {
        let Some(out) = self.osc.as_ref() else { return vec![] };
        let mut all = messages.to_vec();
        if out.mirror {
            all.extend(event.map(|e| osc::mirror(&out.prefix, e, encoder_position)));
            all.extend(bank.map(|b| osc::mirror_bank(&out.prefix, b)));
        }
        all.iter()
            .filter(|m| out.sender.send(m))
            .map(|m| MidiMessage::osc(Direction::Osc, m))
            .collect()
    }

    fn send_pad(&mut self, messages: &[Vec<u8>]) {
        if let Some(out) = self.pad_out.as_mut() {
            for message in messages {
                let _ = out.send(message);
            }
        }
    }

    /// Sends to the DAW and returns the messages for the monitor.
    fn send_daw(&mut self, messages: &[Vec<u8>]) -> Vec<MidiMessage> {
        let Some(out) = self.daw_out.as_mut() else {
            return vec![];
        };
        messages
            .iter()
            .filter(|m| out.send(m))
            .map(|m| MidiMessage::new(Direction::Out, m))
            .collect()
    }
}

struct Connection {
    _pad_in: MidiInputConnection<()>,
    _feedback_in: Option<FeedbackInput>,
    shared: Arc<Mutex<Shared>>,
    info: ConnectionInfo,
}

impl Connection {
    fn close(self) {
        keys::release_all();
        let mut shared = self.shared.lock().unwrap();
        if self.info.native_mode {
            // Without this message the padKONTROL stays silent until it is power-cycled.
            shared.send_pad(&[padkontrol::exit_native_mode()]);
        }
    }
}

/// What a lost connection reconnects with.
#[derive(Clone)]
struct Remembered {
    options: ConnectOptions,
    profile: Option<String>,
}

/// Connection to the padKONTROL and the DAW, shared by the Tauri commands.
#[derive(Default)]
pub struct PadConnection {
    current: Mutex<Option<Connection>>,
    /// Settings of the last connection, until the user disconnects.
    remembered: Mutex<Option<Remembered>>,
    /// The padKONTROL went away: reconnect as soon as it is back.
    waiting: std::sync::atomic::AtomicBool,
}

/// Sent to the UI whenever the connection is lost or comes back by itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionState {
    pub connection: Option<ConnectionInfo>,
    pub waiting: bool,
    pub engine: Option<EngineState>,
}

pub const CONNECTION_EVENT: &str = "connection-state";

impl PadConnection {
    pub fn waiting(&self) -> bool {
        self.waiting.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn state(&self) -> ConnectionState {
        ConnectionState {
            connection: self.info(),
            waiting: self.waiting(),
            engine: self.engine_state(),
        }
    }

    /// Input port to watch: the connected one, or the one being waited for.
    fn watched_input(&self) -> Option<String> {
        self.info().map(|info| info.input).or_else(|| {
            self.waiting()
                .then(|| {
                    self.remembered
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|r| r.options.input.clone())
                })
                .flatten()
        })
    }

    /// Whether the padKONTROL answers a heartbeat, in native mode. With the
    /// Korg driver on Windows, an unplugged padKONTROL stays in the port list
    /// and the open ports go silent, so the list alone does not tell.
    fn responds(&self) -> Option<bool> {
        let shared = {
            let current = self.current.lock().unwrap();
            let connection = current.as_ref()?;
            if !connection.info.native_mode {
                return None;
            }
            connection.shared.clone()
        };
        // Two tries, so that one lost message does not drop the connection.
        for _ in 0..2 {
            let before = {
                let mut shared = shared.lock().unwrap();
                shared.send_pad(&[padkontrol::report_all_controls()]);
                shared.acks
            };
            let deadline = std::time::Instant::now() + HEARTBEAT_TIMEOUT;
            while std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
                if shared.lock().unwrap().acks != before {
                    return Some(true);
                }
            }
        }
        Some(false)
    }

    /// The padKONTROL went away: drop the connection, keep its settings.
    fn lose(&self) {
        self.close_current();
        self.waiting.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn info(&self) -> Option<ConnectionInfo> {
        self.current.lock().unwrap().as_ref().map(|c| c.info.clone())
    }

    pub fn engine_state(&self) -> Option<EngineState> {
        let current = self.current.lock().unwrap();
        let shared = current.as_ref()?.shared.lock().unwrap();
        shared.engine.as_ref().map(Engine::state)
    }

    pub fn connect(
        &self,
        app: AppHandle,
        options: ConnectOptions,
        profile: Option<(String, Profile)>,
    ) -> Result<ConnectionInfo, String> {
        if options.native_mode && options.output.is_none() {
            return Err(t!("core.connection.nativeModeNeedsOutput"));
        }
        check_no_loop(&options)?;
        self.close_current();
        let remembered = Remembered {
            options: options.clone(),
            profile: profile.as_ref().map(|(file, _)| file.clone()),
        };

        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut pad_out = options.output.as_deref().map(open_output).transpose()?;
        let (daw_out, daw_name) = match &options.daw_output {
            Some(choice) => {
                let (output, name) = open_daw_output(choice)?;
                (Some(output), Some(name))
            }
            None => (None, None),
        };

        let osc_out = options.osc.as_ref().map(open_osc).transpose()?;
        let osc_name = osc_out.as_ref().map(|o| o.sender.target().to_string());
        let osc_listener = match &options.osc {
            Some(OscOptions {
                listen_port: Some(port),
                prefix,
                ..
            }) => Some((bind_osc_listener(*port)?, osc::normalize_prefix(prefix))),
            _ => None,
        };
        let osc_listen = osc_listener
            .as_ref()
            .and_then(|(socket, _)| socket.local_addr().ok())
            .map(|a| a.port());

        let pad_in = open_input(&options.input, pad_input_handler(app.clone(), shared.clone()))?;
        let (feedback_in, feedback_name) = match &options.feedback_input {
            Some(choice) => {
                let (input, name) = open_feedback_input(choice, feedback_handler(app.clone(), shared.clone()))?;
                (Some(input), Some(name))
            }
            None => (None, None),
        };

        // The engine only runs in native mode: outside of it, the padKONTROL
        // already sends its own MIDI messages.
        let mut engine = profile
            .filter(|_| options.native_mode)
            .map(|(file, profile)| Engine::new(profile).with_file(file));

        if options.native_mode {
            let out = pad_out.as_mut().expect("output port checked above");
            let mut init = padkontrol::enter_native_mode(DISPLAY_TEXT);
            if let Some(engine) = &mut engine {
                init.extend(engine.refresh());
            }
            for message in init {
                out.send(&message)
                    .map_err(|e| t!("core.connection.nativeModeSendFailed", error = e))?;
            }
        }

        if let Some(engine) = &engine {
            let _ = app.emit(ENGINE_EVENT, Some(engine.state()));
        }
        *shared.lock().unwrap() = Shared {
            engine,
            pad_out,
            daw_out,
            osc: osc_out,
            acks: 0,
        };
        // A profile can also be picked later, while connected.
        if options.native_mode {
            spawn_engine_ticker(app.clone(), Arc::downgrade(&shared));
        }
        if let Some((socket, prefix)) = osc_listener {
            spawn_osc_listener(app.clone(), Arc::downgrade(&shared), socket, prefix);
        }

        let info = ConnectionInfo {
            input: options.input,
            output: options.output,
            native_mode: options.native_mode,
            daw_output: daw_name,
            feedback_input: feedback_name,
            osc: osc_name,
            osc_listen,
        };
        *self.current.lock().unwrap() = Some(Connection {
            _pad_in: pad_in,
            _feedback_in: feedback_in,
            shared,
            info: info.clone(),
        });
        *self.remembered.lock().unwrap() = Some(remembered);
        self.waiting.store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(info)
    }

    /// Disconnects on the user's request: nothing reconnects by itself.
    pub fn disconnect(&self) {
        self.close_current();
        *self.remembered.lock().unwrap() = None;
        self.waiting.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    fn close_current(&self) {
        if let Some(connection) = self.current.lock().unwrap().take() {
            connection.close();
        }
    }

    /// Replaces the active profile without dropping the connection. The same
    /// file, edited, keeps its current bank.
    pub fn set_profile(&self, file: &str, profile: Profile) -> Result<EngineState, String> {
        if let Some(remembered) = self.remembered.lock().unwrap().as_mut() {
            remembered.profile = Some(file.to_string());
        }
        // Keys held by the previous actions would stay down.
        keys::release_all();
        self.with_engine(|shared| {
            match shared.engine.as_mut() {
                Some(engine) if engine.file() == file => engine.reload(profile),
                _ => shared.engine = Some(Engine::new(profile).with_file(file)),
            }
            let engine = shared.engine.as_mut().expect("engine set just above");
            let state = engine.state();
            let refresh = engine.refresh();
            shared.send_pad(&refresh);
            Ok(state)
        })
    }

    /// File of the profile the engine runs, if any.
    pub fn active_file(&self) -> Option<String> {
        self.engine_state().map(|state| state.file)
    }

    pub fn select_bank(&self, bank: usize) -> Result<EngineState, String> {
        self.with_engine(|shared| {
            let engine = shared
                .engine
                .as_mut()
                .ok_or_else(|| t!("core.connection.noActiveProfile"))?;
            let out = engine.select_bank(bank);
            let state = engine.state();
            shared.send_pad(&out.pad);
            if out.bank_changed {
                shared.send_osc(&[], None, None, Some(state.bank));
            }
            Ok(state)
        })
    }

    fn with_engine<T>(&self, f: impl FnOnce(&mut Shared) -> Result<T, String>) -> Result<T, String> {
        let current = self.current.lock().unwrap();
        let connection = current.as_ref().ok_or_else(|| t!("core.connection.notConnected"))?;
        if !connection.info.native_mode {
            return Err(t!("core.connection.engineNeedsNativeMode"));
        }
        let mut shared = connection.shared.lock().unwrap();
        f(&mut shared)
    }
}

/// Refuses a feedback port that would read back what the app sends to the
/// DAW: a loopMIDI port returns everything written to it, and the virtual
/// output shows up as an input under the same name.
fn check_no_loop(options: &ConnectOptions) -> Result<(), String> {
    let looping = match (&options.daw_output, &options.feedback_input) {
        (Some(PortChoice::Port(out)), Some(PortChoice::Port(feedback))) => out == feedback,
        (Some(PortChoice::Virtual), Some(PortChoice::Port(feedback))) => feedback == VIRTUAL_OUTPUT_NAME,
        _ => false,
    };
    if looping {
        return Err(t!("core.connection.feedbackLoop"));
    }
    Ok(())
}

pub type Handler = Box<dyn FnMut(&[u8]) + Send + 'static>;

/// Engine clock: rolls, flam notes and the display need a tick this often.
const ENGINE_TICK: std::time::Duration = std::time::Duration::from_millis(5);

/// Ticks the engine: plays rolls and flam notes on time, and brings the bank
/// back on the display after a value. Stops on its own once the connection
/// is gone.
fn spawn_engine_ticker(app: AppHandle, shared: std::sync::Weak<Mutex<Shared>>) {
    std::thread::spawn(move || {
        while let Some(shared) = shared.upgrade() {
            let sent = {
                let mut shared = shared.lock().unwrap();
                match shared.engine.as_mut() {
                    Some(engine) => {
                        let out = engine.tick(std::time::Instant::now());
                        shared.send_pad(&out.pad);
                        shared.send_daw(&out.daw)
                    }
                    None => vec![],
                }
            };
            drop(shared);
            for message in sent {
                let _ = app.emit(MESSAGE_EVENT, message);
            }
            std::thread::sleep(ENGINE_TICK);
        }
    });
}

/// How often the OSC listener checks that its connection is still there.
const OSC_LISTEN_POLL: std::time::Duration = std::time::Duration::from_millis(100);

fn bind_osc_listener(port: u16) -> Result<std::net::UdpSocket, String> {
    // On a quick reconnect, the previous listener can hold the port for one
    // more poll before it notices and stops.
    let mut attempt = 0;
    let socket = loop {
        match std::net::UdpSocket::bind(("0.0.0.0", port)) {
            Ok(socket) => break socket,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && attempt < 5 => {
                attempt += 1;
                std::thread::sleep(OSC_LISTEN_POLL);
            }
            Err(e) => {
                return Err(t!("core.osc.listenFailed", port, error = e));
            }
        }
    };
    socket
        .set_read_timeout(Some(OSC_LISTEN_POLL))
        .map_err(|e| e.to_string())?;
    Ok(socket)
}

/// Receives OSC for the engine (LEDs, display, bank) and the monitor. Stops,
/// freeing the port, once the connection is gone.
fn spawn_osc_listener(
    app: AppHandle,
    shared: std::sync::Weak<Mutex<Shared>>,
    socket: std::net::UdpSocket,
    prefix: String,
) {
    std::thread::spawn(move || {
        let mut buffer = vec![0u8; 65_536];
        loop {
            let received = socket.recv_from(&mut buffer);
            let Some(shared) = shared.upgrade() else { break };
            let Ok((size, _)) = received else { continue };
            for message in osc::decode(&buffer[..size]) {
                let mut state = None;
                {
                    let mut shared = shared.lock().unwrap();
                    if let Some(engine) = shared.engine.as_mut() {
                        let out = engine.osc_feedback(&message, &prefix);
                        if out.bank_changed {
                            state = Some(engine.state());
                        }
                        shared.send_pad(&out.pad);
                    }
                }
                let _ = app.emit(MESSAGE_EVENT, MidiMessage::osc(Direction::OscIn, &message));
                if let Some(state) = state {
                    let _ = app.emit(ENGINE_EVENT, Some(state));
                }
            }
        }
    });
}

/// Messages from the padKONTROL: monitor, then engine, then DAW, OSC and LEDs.
fn pad_input_handler(app: AppHandle, shared: Arc<Mutex<Shared>>) -> Handler {
    Box::new(move |bytes| {
        // Heartbeat answers would flood the monitor.
        if padkontrol::is_report_ack(bytes) {
            let mut shared = shared.lock().unwrap();
            shared.acks = shared.acks.wrapping_add(1);
            return;
        }
        let mut incoming = MidiMessage::new(Direction::In, bytes);
        let mut sent = vec![];
        let mut state = None;
        if let Some(event) = &incoming.event {
            let mut shared = shared.lock().unwrap();
            let out = shared.engine.as_mut().map(|engine| {
                let out = engine.handle(event);
                if out.bank_changed {
                    state = Some(engine.state());
                }
                out
            });
            let (osc_messages, encoder_position) = match &out {
                Some(out) => {
                    incoming.encoder_position = out.encoder_position;
                    shared.send_pad(&out.pad);
                    sent = shared.send_daw(&out.daw);
                    keys::send(&out.keys);
                    sent.extend(out.keys.iter().map(MidiMessage::key));
                    (out.osc.as_slice(), out.encoder_position)
                }
                None => (&[][..], None),
            };
            // The mirror runs with or without a profile.
            let bank = state.as_ref().map(|s: &EngineState| s.bank);
            sent.extend(shared.send_osc(osc_messages, Some(event), encoder_position, bank));
        }
        let _ = app.emit(MESSAGE_EVENT, incoming);
        for message in sent {
            let _ = app.emit(MESSAGE_EVENT, message);
        }
        if let Some(state) = state {
            let _ = app.emit(ENGINE_EVENT, Some(state));
        }
    })
}

/// Messages sent back by the DAW: LEDs in `follow` mode.
fn feedback_handler(app: AppHandle, shared: Arc<Mutex<Shared>>) -> Handler {
    Box::new(move |bytes| {
        {
            let mut shared = shared.lock().unwrap();
            if let Some(engine) = shared.engine.as_mut() {
                let leds = engine.feedback(bytes);
                shared.send_pad(&leds);
            }
        }
        // Clock pulses and active sensing would flood the monitor.
        if !matches!(bytes, [0xF8] | [0xFE]) {
            let _ = app.emit(MESSAGE_EVENT, MidiMessage::new(Direction::Feedback, bytes));
        }
    })
}

fn new_input() -> Result<MidiInput, String> {
    let mut midi_in = MidiInput::new(CLIENT_NAME).map_err(|e| e.to_string())?;
    // Native mode runs entirely over SysEx, which midir ignores by default.
    midi_in.ignore(Ignore::None);
    Ok(midi_in)
}

fn open_input(name: &str, mut handler: Handler) -> Result<MidiInputConnection<()>, String> {
    let midi_in = new_input()?;
    let port = midi_in
        .ports()
        .into_iter()
        .find(|p| midi_in.port_name(p).is_ok_and(|n| n == name))
        .ok_or_else(|| t!("core.midi.inputNotFound", name))?;
    midi_in
        .connect(&port, "totalKONTROL-in", move |_, bytes, _| handler(bytes), ())
        .map_err(|e| open_failed(name, e))
}

fn open_failed(name: &str, e: impl std::fmt::Display) -> String {
    t!("core.midi.openFailed", error = e, name)
}

#[cfg(unix)]
fn virtual_port_failed(e: impl std::fmt::Display) -> String {
    t!("core.midi.virtualPortFailed", error = e)
}

fn open_osc(options: &OscOptions) -> Result<OscOutput, String> {
    let host = options.host.trim();
    let sender = OscSender::open(host, options.port)
        .map_err(|e| t!("core.osc.unreachable", port = options.port, error = e, host))?;
    let prefix = osc::normalize_prefix(&options.prefix);
    Ok(OscOutput {
        sender,
        prefix,
        mirror: options.mirror,
    })
}

fn open_output(name: &str) -> Result<MidiOutputConnection, String> {
    let midi_out = MidiOutput::new(CLIENT_NAME).map_err(|e| e.to_string())?;
    let port = midi_out
        .ports()
        .into_iter()
        .find(|p| midi_out.port_name(p).is_ok_and(|n| n == name))
        .ok_or_else(|| t!("core.midi.outputNotFound", name))?;
    midi_out
        .connect(&port, "totalKONTROL-out")
        .map_err(|e| open_failed(name, e))
}

fn open_daw_output(choice: &PortChoice) -> Result<(DawOutput, String), String> {
    match choice {
        PortChoice::Port(name) => Ok((DawOutput::Midir(open_output(name)?), name.clone())),
        PortChoice::Virtual => virtual_output().map(|out| (out, VIRTUAL_OUTPUT_NAME.to_string())),
    }
}

fn open_feedback_input(choice: &PortChoice, handler: Handler) -> Result<(FeedbackInput, String), String> {
    match choice {
        PortChoice::Port(name) => Ok((FeedbackInput::Midir(open_input(name, handler)?), name.clone())),
        PortChoice::Virtual => virtual_input(handler).map(|input| (input, VIRTUAL_FEEDBACK_NAME.to_string())),
    }
}

#[cfg(unix)]
fn virtual_output() -> Result<DawOutput, String> {
    use midir::os::unix::VirtualOutput;
    MidiOutput::new(CLIENT_NAME)
        .map_err(|e| e.to_string())?
        .create_virtual(VIRTUAL_OUTPUT_NAME)
        .map(DawOutput::Midir)
        .map_err(virtual_port_failed)
}

#[cfg(unix)]
fn virtual_input(mut handler: Handler) -> Result<FeedbackInput, String> {
    use midir::os::unix::VirtualInput;
    new_input()?
        .create_virtual(VIRTUAL_FEEDBACK_NAME, move |_, bytes, _| handler(bytes), ())
        .map(FeedbackInput::Midir)
        .map_err(virtual_port_failed)
}

#[cfg(all(windows, feature = "windows-midi-services"))]
fn windows_device() -> Result<Arc<winmidi::VirtualDevice>, String> {
    match winmidi::availability() {
        winmidi::Availability::Ready => winmidi::device(),
        winmidi::Availability::NeedsWindowsUpdate => Err(winmidi::needs_windows_update()),
        winmidi::Availability::Unavailable => Err(t!("core.midi.windowsMidiUnavailable")),
    }
}

#[cfg(all(windows, feature = "windows-midi-services"))]
fn virtual_output() -> Result<DawOutput, String> {
    windows_device().map(DawOutput::Windows)
}

#[cfg(all(windows, feature = "windows-midi-services"))]
fn virtual_input(handler: Handler) -> Result<FeedbackInput, String> {
    Ok(FeedbackInput::Windows(windows_device()?.listen(handler)))
}

#[cfg(all(windows, not(feature = "windows-midi-services")))]
fn virtual_output() -> Result<DawOutput, String> {
    Err(virtual_unsupported())
}

#[cfg(all(windows, not(feature = "windows-midi-services")))]
fn virtual_input(_handler: Handler) -> Result<FeedbackInput, String> {
    Err(virtual_unsupported())
}

#[cfg(all(windows, not(feature = "windows-midi-services")))]
fn virtual_unsupported() -> String {
    t!("core.midi.virtualUnsupported")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(daw: Option<PortChoice>, feedback: Option<PortChoice>) -> ConnectOptions {
        ConnectOptions {
            input: "in".into(),
            output: None,
            native_mode: false,
            daw_output: daw,
            feedback_input: feedback,
            osc: None,
        }
    }

    #[test]
    fn refuses_feedback_ports_that_loop() {
        let port = |name: &str| Some(PortChoice::Port(name.into()));
        assert!(check_no_loop(&options(port("loop"), port("loop"))).is_err());
        assert!(check_no_loop(&options(Some(PortChoice::Virtual), port(VIRTUAL_OUTPUT_NAME))).is_err());
        assert!(check_no_loop(&options(port("totalKONTROL"), port("totalKONTROL feedback"))).is_ok());
        assert!(check_no_loop(&options(Some(PortChoice::Virtual), Some(PortChoice::Virtual))).is_ok());
        assert!(check_no_loop(&options(None, port("x"))).is_ok());
    }

    #[test]
    fn osc_listener_receives_and_frees_its_port() {
        let socket = bind_osc_listener(0).unwrap();
        let port = socket.local_addr().unwrap().port();
        let sender = OscSender::open("127.0.0.1", port).unwrap();
        assert!(sender.send(&OscMessage::new("/tk/bank", vec![osc::OscArg::Int(1)])));
        let mut buffer = [0u8; 512];
        let (size, _) = socket.recv_from(&mut buffer).unwrap();
        assert_eq!(osc::decode(&buffer[..size])[0].describe(), "/tk/bank 1");

        // A second listener on a taken port reports it.
        assert!(bind_osc_listener(port).is_err());
        drop(socket);
        assert!(bind_osc_listener(port).is_ok());
    }

    /// Diagnostic tool: `cargo test print_ports -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_ports() {
        println!("{:#?}", super::list_ports().unwrap());
    }
}
