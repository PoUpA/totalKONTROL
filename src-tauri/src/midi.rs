use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{Bank, Profile, ProfileStore};
use crate::engine::{Engine, EngineState, Output};
use crate::keys::{self, KeyEvent};
use crate::osc::{self, OscMessage, OscSender};
use crate::padkontrol::{self, ControlEvent};
use crate::scenes;
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
    with_timeout(ENUMERATION_TIMEOUT, enumerate_ports).unwrap_or_else(|| Err(midi_service_frozen()))
}

/// Names of the MIDI inputs, or `None` if the system MIDI service hangs.
fn input_port_names() -> Option<Vec<String>> {
    with_timeout(ENUMERATION_TIMEOUT, || {
        MidiInput::new(CLIENT_NAME).map(|midi_in| {
            midi_in
                .ports()
                .iter()
                .filter_map(|p| midi_in.port_name(p).ok())
                .collect()
        })
    })?
    .ok()
}

/// MIDI work that outlived its timeout and still runs: most likely stuck in
/// a frozen system MIDI service, for good.
static STUCK: Mutex<Vec<Arc<AtomicBool>>> = Mutex::new(Vec::new());

/// Runs MIDI work on its own thread and gives up after `timeout`, as the
/// system MIDI service can hang. While earlier work is still stuck, new work
/// is refused at once (`None`) instead of piling up threads stuck too.
pub fn with_timeout<T: Send + 'static>(
    timeout: std::time::Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    {
        let mut stuck = STUCK.lock().unwrap_or_else(|e| e.into_inner());
        stuck.retain(|done| !done.load(Ordering::SeqCst));
        if !stuck.is_empty() {
            return None;
        }
    }
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
        finished.store(true, Ordering::SeqCst);
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => Some(result),
        Err(_) => {
            STUCK.lock().unwrap_or_else(|e| e.into_inner()).push(done);
            None
        }
    }
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
/// them and reloads the profile. Each decision is dropped if the user
/// connected or disconnected in the meantime (see `PadConnection::generation`).
pub fn spawn_watchdog(app: AppHandle) {
    std::thread::spawn(move || {
        let mut delay = WATCH_INTERVAL;
        loop {
            std::thread::sleep(delay);
            delay = WATCH_INTERVAL;
            let connection = app.state::<PadConnection>();
            let generation = connection.generation();
            let Some(input) = connection.watched_input() else {
                continue;
            };
            let Some(names) = input_port_names() else {
                delay = FROZEN_BACKOFF;
                continue;
            };
            let present = names.contains(&input);
            if connection.info().is_some() {
                if !present || connection.responds() == Some(false) {
                    connection.lose_if(&app, &input, generation);
                }
            } else if present && connection.waiting() {
                std::thread::sleep(SETTLE_TIME);
                connection.reconnect_if(&app, generation);
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
    /// Accept OSC from other devices (a tablet…), not only from this computer.
    #[serde(default)]
    pub listen_network: bool,
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
    /// Global MIDI channel of the padKONTROL (1 to 16), in native mode.
    pub global_channel: Option<u8>,
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

/// How long the padKONTROL has to answer a data dump or a request.
const EXCHANGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
/// How long connecting and disconnecting wait for an operation under way.
const OPERATION_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
/// How long the padKONTROL has to answer native mode on, telling its global
/// MIDI channel.
const CHANNEL_DETECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
/// How long closing waits for an output to send what is queued.
const CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
/// The same, when the app is going down after a panic.
const PANIC_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(300);

enum WriterCommand {
    Send(Vec<u8>),
    Flush(mpsc::Sender<()>),
}

/// Sends MIDI from its own thread. Callbacks and locks only queue messages:
/// on Windows a SysEx message is sent synchronously and waits for the driver,
/// which would otherwise hold up the inputs, the engine ticker and the
/// heartbeat. The port closes when the writer is closed or dropped.
struct Writer {
    tx: mpsc::Sender<WriterCommand>,
    closed: mpsc::Receiver<()>,
}

/// What a writer sends to.
trait Sink: Send + 'static {
    fn send(&mut self, message: &[u8]);
    /// Called once the queue is empty, after a burst of messages.
    fn idle(&mut self) {}
}

/// A sink that only sends, through `send`.
fn sink<F: FnMut(&[u8]) + Send + 'static>(send: F) -> impl Sink {
    struct FnSink<F>(F);
    impl<F: FnMut(&[u8]) + Send + 'static> Sink for FnSink<F> {
        fn send(&mut self, message: &[u8]) {
            (self.0)(message)
        }
    }
    FnSink(send)
}

impl Writer {
    fn spawn(mut sink: impl Sink) -> Self {
        let (tx, rx) = mpsc::channel();
        let (closed_tx, closed) = mpsc::channel();
        std::thread::spawn(move || {
            sink.idle();
            while let Ok(command) = rx.recv() {
                let mut next = Some(command);
                while let Some(command) = next {
                    match command {
                        WriterCommand::Send(message) => sink.send(&message),
                        WriterCommand::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                    next = rx.try_recv().ok();
                }
                sink.idle();
            }
            // Closes the port before telling.
            drop(sink);
            let _ = closed_tx.send(());
        });
        Self { tx, closed }
    }

    fn send(&self, message: Vec<u8>) {
        let _ = self.tx.send(WriterCommand::Send(message));
    }

    fn flush(&self, timeout: std::time::Duration) -> bool {
        flush(&self.tx, timeout)
    }

    /// Sends what is queued and closes the port, waiting at most `timeout`:
    /// a port reopened right after must be free. Returns what tells when a
    /// port still closing is done.
    fn close(self, timeout: std::time::Duration) -> Option<mpsc::Receiver<()>> {
        let Writer { tx, closed } = self;
        drop(tx);
        match closed.recv_timeout(timeout) {
            Err(mpsc::RecvTimeoutError::Timeout) => Some(closed),
            _ => None,
        }
    }
}

/// The padKONTROL output: readdresses native messages to its global channel,
/// and tells the UI which LEDs are lit.
struct PadSink {
    out: MidiOutputConnection,
    channel: u8,
    app: AppHandle,
    lit: std::collections::BTreeSet<LedId>,
    changed: bool,
}

/// An LED, as the UI names it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LedId {
    Pad { pad: u8 },
    Button { button: crate::config::ButtonId },
}

pub const LEDS_EVENT: &str = "leds";

impl PadSink {
    fn observe(&mut self, message: &[u8]) {
        let Some((control, on)) = padkontrol::decode_led(message) else {
            return;
        };
        let id = match control {
            padkontrol::Control::Pad(pad) => LedId::Pad { pad },
            padkontrol::Control::Button(button) => LedId::Button { button },
            padkontrol::Control::XyTouch => return,
        };
        self.changed |= if on { self.lit.insert(id) } else { self.lit.remove(&id) };
    }
}

impl Sink for PadSink {
    fn send(&mut self, message: &[u8]) {
        self.observe(message);
        let mut message = message.to_vec();
        padkontrol::for_channel(&mut message, self.channel);
        let _ = self.out.send(&message);
    }

    fn idle(&mut self) {
        if std::mem::take(&mut self.changed) {
            let lit: Vec<&LedId> = self.lit.iter().collect();
            let _ = self.app.emit(LEDS_EVENT, lit);
        }
    }
}

/// Waits until what was queued before has been sent, at most `timeout`.
fn flush(tx: &mpsc::Sender<WriterCommand>, timeout: std::time::Duration) -> bool {
    let (done, wait) = mpsc::channel();
    tx.send(WriterCommand::Flush(done)).is_ok() && wait.recv_timeout(timeout).is_ok()
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
    pad_out: Option<Writer>,
    daw_out: Option<Writer>,
    osc: Option<OscOutput>,
    /// Reporting setup acknowledgements received, for the heartbeat.
    acks: u32,
    /// Global MIDI channel the padKONTROL answered native mode on with.
    detected_channel: Option<u8>,
    /// Outside native mode: what the padKONTROL sends goes on to the DAW.
    thru: bool,
    /// An exchange waiting for an answer of the padKONTROL (see `exchange`).
    waiter: Option<Waiter>,
    /// The engine ticker, woken when the engine has something new to time.
    ticker: Option<std::thread::Thread>,
}

impl Shared {
    fn wake_ticker(&self) {
        if let Some(ticker) = &self.ticker {
            ticker.unpark();
        }
    }
}

/// The answer an exchange waits for, and where it goes.
struct Waiter {
    accepts: fn(&[u8]) -> bool,
    answer: mpsc::Sender<Vec<u8>>,
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
        let mirror: Vec<OscMessage> = if out.mirror {
            let control = event.map(|e| osc::mirror(&out.prefix, e, encoder_position));
            control
                .into_iter()
                .chain(bank.map(|b| osc::mirror_bank(&out.prefix, b)))
                .collect()
        } else {
            vec![]
        };
        let shown = ui_visible();
        messages
            .iter()
            .chain(&mirror)
            .filter(|m| out.sender.send(m) && shown)
            .map(|m| MidiMessage::osc(Direction::Osc, m))
            .collect()
    }

    fn send_pad(&self, messages: &[Vec<u8>]) {
        if let Some(out) = &self.pad_out {
            for message in messages {
                out.send(message.clone());
            }
        }
    }

    /// Sends what an engine releases before it goes, without the monitor.
    fn send_released(&mut self, out: &Output) {
        self.send_daw(&out.daw);
        self.send_osc(&out.osc, None, None, None);
        keys::send(&out.keys);
    }

    /// Queues for the DAW and returns the messages for the monitor.
    fn send_daw(&self, messages: &[Vec<u8>]) -> Vec<MidiMessage> {
        let Some(out) = &self.daw_out else {
            return vec![];
        };
        let shown = ui_visible();
        messages
            .iter()
            .filter_map(|m| {
                out.send(m.clone());
                shown.then(|| MidiMessage::new(Direction::Out, m))
            })
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
    /// Closes everything and returns the engine, with nothing left on.
    /// `closing`: where to leave the ports the driver is slow to close.
    fn close(self, closing: &Mutex<Vec<mpsc::Receiver<()>>>) -> Option<Engine> {
        let (writers, engine) = {
            let mut shared = self.shared.lock().unwrap();
            // Notes left on would keep sounding in the DAW.
            let mut engine = shared.engine.take();
            if let Some(engine) = engine.as_mut() {
                let released = engine.release_all();
                shared.send_released(&released);
            }
            if self.info.native_mode {
                // Without this message the padKONTROL stays silent until it is power-cycled.
                shared.send_pad(&[padkontrol::exit_native_mode()]);
            }
            ([shared.pad_out.take(), shared.daw_out.take()], engine)
        };
        keys::release_all();
        let slow = writers
            .into_iter()
            .flatten()
            .filter_map(|writer| writer.close(CLOSE_TIMEOUT));
        closing.lock().unwrap().extend(slow);
        engine
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
    waiting: AtomicBool,
    /// Held while connecting, disconnecting or dropping a lost connection, so
    /// that the user and the watchdog never interleave them.
    operations: Mutex<()>,
    /// Bumped by each of them: the watchdog gives up a decision it took
    /// before, such as reconnecting after the user said to stop waiting.
    generation: AtomicU64,
    /// The app is closing: nothing connects any more.
    shutting_down: AtomicBool,
    /// Ports of closed connections the driver has not finished closing.
    closing: Mutex<Vec<mpsc::Receiver<()>>>,
    /// Engine of a lost connection, for the reconnection to go on with.
    kept: Mutex<Option<Engine>>,
    /// Queue of the padKONTROL output, reachable without the other locks, so
    /// that a panic can still hand the padKONTROL back.
    emergency: Mutex<Option<mpsc::Sender<WriterCommand>>>,
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

/// Whether the window shows: hidden in the tray, the monitor and the live
/// view get nothing, and the messages for them are not even described.
static UI_VISIBLE: AtomicBool = AtomicBool::new(true);

pub fn set_ui_visible(visible: bool) {
    UI_VISIBLE.store(visible, Ordering::Relaxed);
}

fn ui_visible() -> bool {
    UI_VISIBLE.load(Ordering::Relaxed)
}

/// Shows messages in the monitor and the live view, when the window shows.
fn emit_messages(app: &AppHandle, messages: impl IntoIterator<Item = MidiMessage>) {
    if ui_visible() {
        for message in messages {
            let _ = app.emit(MESSAGE_EVENT, message);
        }
    }
}
/// Sent when an automatic reconnection worked around a problem, such as a
/// profile that no longer loads.
pub const CONNECTION_PROBLEM_EVENT: &str = "connection-problem";

impl PadConnection {
    pub fn waiting(&self) -> bool {
        self.waiting.load(Ordering::SeqCst)
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Takes `operations`, waiting at most `timeout`: a reconnection stuck in
    /// a frozen MIDI service may hold it for good.
    fn operations(&self, timeout: std::time::Duration) -> Option<std::sync::MutexGuard<'_, ()>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match self.operations.try_lock() {
                Ok(guard) => return Some(guard),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => return Some(poisoned.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(std::sync::TryLockError::WouldBlock) => return None,
            }
        }
    }

    /// Records a change of connection and tells the UI.
    fn changed(&self, app: &AppHandle) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let _ = app.emit(CONNECTION_EVENT, self.state());
    }

    /// The connection and its engine, read together.
    pub fn state(&self) -> ConnectionState {
        let current = self.current.lock().unwrap();
        let connection = current.as_ref();
        ConnectionState {
            connection: connection.map(|c| c.info.clone()),
            waiting: self.waiting(),
            engine: connection.and_then(|c| c.shared.lock().unwrap().engine.as_ref().map(Engine::state)),
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
                let shared = shared.lock().unwrap();
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

    /// Watchdog: the padKONTROL on `input` went away. Drops the connection
    /// and keeps its settings, unless the connection changed since the
    /// watchdog looked (`generation`).
    fn lose_if(&self, app: &AppHandle, input: &str, generation: u64) {
        let Some(_operation) = self.operations(std::time::Duration::ZERO) else {
            return;
        };
        if self.generation() != generation || self.info().is_none_or(|info| info.input != input) {
            return;
        }
        crate::diag::log(format!("lost the padKONTROL on {input}"));
        // Bank, roll and flam, encoder positions: the reconnection goes on
        // where the padKONTROL left off.
        *self.kept.lock().unwrap() = self.close_current();
        let remembered = self.remembered.lock().unwrap().is_some();
        self.waiting.store(remembered, Ordering::SeqCst);
        self.changed(app);
    }

    /// Watchdog: the padKONTROL waited for is back. Reconnects with the
    /// remembered settings, unless the user connected, disconnected or
    /// stopped waiting since the watchdog looked (`generation`).
    fn reconnect_if(&self, app: &AppHandle, generation: u64) {
        let Some(_operation) = self.operations(std::time::Duration::ZERO) else {
            return;
        };
        if self.generation() != generation
            || self.shutting_down.load(Ordering::SeqCst)
            || !self.waiting()
            || self.info().is_some()
        {
            return;
        }
        let Some(remembered) = self.remembered.lock().unwrap().clone() else {
            return;
        };
        // The profile as saved now, on the engine kept from before.
        let kept = self.kept.lock().unwrap().clone();
        let mut problem = None;
        let engine = match remembered.profile {
            Some(file) => match app.state::<ProfileStore>().load(&file) {
                Ok(profile) => Some(match kept {
                    Some(mut engine) if engine.file() == file => {
                        // Its notes were released when the connection went.
                        let _ = engine.reload(profile);
                        engine
                    }
                    _ => Engine::new(profile).with_file(file),
                }),
                Err(errors) => {
                    let errors = errors.join("\n");
                    problem = Some(t!("core.app.invalidProfile", file, errors));
                    // Carries on with the version in memory, if any.
                    kept
                }
            },
            None => None,
        };
        // A failed attempt keeps waiting, and the kept engine: the next round
        // tries again.
        if self
            .connect_locked(app, remembered.options, engine, generation)
            .is_err()
        {
            return;
        }
        // Ports still listed but nobody behind them.
        if self.responds() == Some(false) {
            self.close_current();
            self.waiting.store(true, Ordering::SeqCst);
            return;
        }
        self.kept.lock().unwrap().take();
        crate::diag::log("reconnected");
        self.changed(app);
        if let Some(problem) = problem {
            let _ = app.emit(CONNECTION_PROBLEM_EVENT, problem);
        }
    }

    pub fn info(&self) -> Option<ConnectionInfo> {
        self.current.lock().unwrap().as_ref().map(|c| c.info.clone())
    }

    pub fn engine_state(&self) -> Option<EngineState> {
        let current = self.current.lock().unwrap();
        let shared = current.as_ref()?.shared.lock().unwrap();
        shared.engine.as_ref().map(Engine::state)
    }

    /// Connects on the user's request. Tells the UI even when the command
    /// that asked has given up waiting (frozen MIDI service).
    pub fn connect(
        &self,
        app: &AppHandle,
        options: ConnectOptions,
        profile: Option<(String, Profile)>,
    ) -> Result<ConnectionInfo, String> {
        let Some(_operation) = self.operations(OPERATION_WAIT) else {
            return Err(midi_service_frozen());
        };
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(t!("core.connection.shuttingDown"));
        }
        let engine = profile.map(|(file, profile)| Engine::new(profile).with_file(file));
        let input = options.input.clone();
        let result = self.connect_locked(app, options, engine, self.generation());
        match &result {
            Ok(info) => crate::diag::log(format!("connected: {info:?}")),
            Err(error) => crate::diag::log(format!("connecting to {input} failed: {error}")),
        }
        if result.is_ok() {
            self.kept.lock().unwrap().take();
            self.changed(app);
        }
        result
    }

    /// Opens everything and replaces the current connection. The caller holds
    /// `operations`. Gives up, handing the padKONTROL back, if the connection
    /// changed meanwhile (`generation`): a user disconnecting, or the app
    /// closing, while the MIDI service took its time.
    fn connect_locked(
        &self,
        app: &AppHandle,
        options: ConnectOptions,
        engine: Option<Engine>,
        generation: u64,
    ) -> Result<ConnectionInfo, String> {
        if options.native_mode && options.output.is_none() {
            return Err(t!("core.connection.nativeModeNeedsOutput"));
        }
        check_no_loop(&options)?;
        self.close_current();
        self.wait_for_closing_ports()?;
        let remembered = Remembered {
            options: options.clone(),
            profile: engine.as_ref().map(|engine| engine.file().to_string()),
        };

        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut pad_out = options.output.as_deref().map(open_output).transpose()?;
        let (daw_out, daw_name) = match &options.daw_output {
            Some(choice) => {
                let (mut output, name) = open_daw_output(choice)?;
                let writer = Writer::spawn(sink(move |message| {
                    output.send(message);
                }));
                (Some(writer), Some(name))
            }
            None => (None, None),
        };

        let osc_out = options.osc.as_ref().map(open_osc).transpose()?;
        let osc_name = osc_out.as_ref().map(|o| o.sender.target().to_string());
        let osc_listener = match &options.osc {
            Some(OscOptions {
                listen_port: Some(port),
                listen_network,
                prefix,
                ..
            }) => Some((
                bind_osc_listener(*port, *listen_network)?,
                osc::normalize_prefix(prefix),
            )),
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
        let mut engine = engine.filter(|_| options.native_mode);

        // Global MIDI channel of the padKONTROL (0 to 15), set in its own
        // settings: its native messages carry it.
        let mut channel = 0;
        let mut init = vec![];
        if options.native_mode {
            let out = pad_out.as_mut().expect("output port checked above");
            let send_failed = |e: midir::SendError| t!("core.connection.nativeModeSendFailed", error = e);
            for message in padkontrol::native_mode_on_all_channels() {
                out.send(&message).map_err(send_failed)?;
            }
            // Without an answer (slow or unusual device), assume channel 1.
            let deadline = std::time::Instant::now() + CHANNEL_DETECTION_TIMEOUT;
            while std::time::Instant::now() < deadline {
                if let Some(detected) = shared.lock().unwrap().detected_channel {
                    channel = detected;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            init = padkontrol::enter_native_mode(DISPLAY_TEXT);
            if let Some(engine) = &mut engine {
                init.extend(engine.refresh());
            }
            for message in &init {
                let mut message = message.clone();
                padkontrol::for_channel(&mut message, channel);
                out.send(&message).map_err(send_failed)?;
            }
        }

        if let Some(engine) = &engine {
            let _ = app.emit(ENGINE_EVENT, Some(engine.state()));
        }
        // From here on, the padKONTROL output only sends from its writer.
        let pad_out = pad_out.map(|out| {
            let mut sink = PadSink {
                out,
                channel,
                app: app.clone(),
                lit: Default::default(),
                // The UI starts from no LED lit, even outside native mode.
                changed: true,
            };
            for message in &init {
                sink.observe(message);
            }
            Writer::spawn(sink)
        });
        let emergency = pad_out.as_ref().filter(|_| options.native_mode).map(|w| w.tx.clone());
        *shared.lock().unwrap() = Shared {
            engine,
            pad_out,
            daw_out,
            osc: osc_out,
            acks: 0,
            detected_channel: Some(channel),
            thru: !options.native_mode,
            waiter: None,
            ticker: None,
        };
        // A profile can also be picked later, while connected.
        if options.native_mode {
            let ticker = spawn_engine_ticker(app.clone(), Arc::downgrade(&shared));
            shared.lock().unwrap().ticker = Some(ticker);
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
            global_channel: options.native_mode.then_some(channel + 1),
        };
        let connection = Connection {
            _pad_in: pad_in,
            _feedback_in: feedback_in,
            shared,
            info: info.clone(),
        };
        let mut current = self.current.lock().unwrap();
        if self.generation() != generation || self.shutting_down.load(Ordering::SeqCst) {
            drop(current);
            connection.close(&self.closing);
            return Err(t!("core.connection.cancelled"));
        }
        *current = Some(connection);
        drop(current);
        *self.emergency.lock().unwrap() = emergency;
        *self.remembered.lock().unwrap() = Some(remembered);
        self.waiting.store(false, Ordering::SeqCst);
        Ok(info)
    }

    /// Disconnects on the user's request: nothing reconnects by itself.
    pub fn disconnect(&self, app: &AppHandle) {
        // Even if a reconnection stuck in the MIDI service holds the lock: the
        // generation bump below makes it give up if it ever comes back.
        let _operation = self.operations(OPERATION_WAIT);
        self.close_current();
        self.kept.lock().unwrap().take();
        *self.remembered.lock().unwrap() = None;
        self.waiting.store(false, Ordering::SeqCst);
        self.changed(app);
    }

    /// The app is closing: hands the padKONTROL back, and nothing connects
    /// again, not even a reconnection already under way.
    pub fn shut_down(&self, app: &AppHandle) {
        self.shutting_down.store(true, Ordering::SeqCst);
        self.disconnect(app);
    }

    /// Closes the connection and returns its engine, with nothing left on.
    fn close_current(&self) -> Option<Engine> {
        // The writer only closes once every queue to it is gone.
        self.emergency.lock().unwrap().take();
        let connection = self.current.lock().unwrap().take();
        connection.and_then(|connection| connection.close(&self.closing))
    }

    /// Waits a little more for the ports of a previous connection the driver
    /// was slow to close: opening them again before would fail.
    fn wait_for_closing_ports(&self) -> Result<(), String> {
        let mut closing = self.closing.lock().unwrap();
        let deadline = std::time::Instant::now() + CLOSE_TIMEOUT;
        closing.retain(|closed| {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            matches!(closed.recv_timeout(left), Err(mpsc::RecvTimeoutError::Timeout))
        });
        if closing.is_empty() {
            Ok(())
        } else {
            Err(t!("core.connection.portsStillClosing"))
        }
    }

    /// Last resort while the app goes down after a panic: ends the notes and
    /// hands the padKONTROL back, without waiting on a lock another thread
    /// may hold.
    pub fn release_on_panic(&self) {
        if let Ok(current) = self.current.try_lock()
            && let Some(connection) = current.as_ref()
            && let Ok(mut shared) = connection.shared.try_lock()
        {
            if let Some(engine) = shared.engine.as_mut() {
                let released = engine.release_all();
                shared.send_daw(&released.daw);
            }
            if let Some(daw) = &shared.daw_out {
                daw.flush(PANIC_TIMEOUT);
            }
        }
        keys::release_all();
        if let Ok(emergency) = self.emergency.try_lock()
            && let Some(tx) = emergency.as_ref()
        {
            let _ = tx.send(WriterCommand::Send(padkontrol::exit_native_mode()));
            flush(tx, PANIC_TIMEOUT);
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
            let released = match shared.engine.as_mut() {
                Some(engine) if engine.file() == file => engine.reload(profile),
                _ => {
                    let released = shared.engine.as_mut().map(Engine::release_all).unwrap_or_default();
                    shared.engine = Some(Engine::new(profile).with_file(file));
                    released
                }
            };
            shared.send_released(&released);
            let engine = shared.engine.as_mut().expect("engine set just above");
            let state = engine.state();
            let refresh = engine.refresh();
            shared.send_pad(&refresh);
            Ok(state)
        })
    }

    /// All notes off: ends what the engine left on, then sends All Notes Off,
    /// All Sound Off and sustain off on every channel of the DAW output, and
    /// releases the held keys.
    pub fn panic(&self) -> Result<(), String> {
        let current = self.current.lock().unwrap();
        let connection = current.as_ref().ok_or_else(|| t!("core.connection.notConnected"))?;
        let mut shared = connection.shared.lock().unwrap();
        if let Some(engine) = shared.engine.as_mut() {
            let released = engine.release_all();
            let leds = engine.refresh();
            shared.send_released(&released);
            shared.send_pad(&leds);
        }
        let all_off: Vec<Vec<u8>> = (0..16u8)
            .flat_map(|channel| [123, 120, 64].map(|cc| vec![0xB0 | channel, cc, 0]))
            .collect();
        shared.send_daw(&all_off);
        keys::release_all();
        Ok(())
    }

    /// Sends a message to the padKONTROL and waits for its answer, the first
    /// message `accepts` takes.
    fn exchange(&self, message: Vec<u8>, accepts: fn(&[u8]) -> bool) -> Result<Vec<u8>, String> {
        let shared = {
            let current = self.current.lock().unwrap();
            let connection = current.as_ref().ok_or_else(|| t!("core.connection.notConnected"))?;
            if connection.info.output.is_none() {
                return Err(t!("core.scenes.needsOutput"));
            }
            connection.shared.clone()
        };
        let (answer, wait) = mpsc::channel();
        {
            let mut shared = shared.lock().unwrap();
            shared.waiter = Some(Waiter { accepts, answer });
            shared.send_pad(&[message]);
        }
        let result = wait.recv_timeout(EXCHANGE_TIMEOUT);
        shared.lock().unwrap().waiter = None;
        result.map_err(|_| t!("core.scenes.noAnswer"))
    }

    /// Recalls a scene into the padKONTROL's edit buffer.
    fn change_scene(&self, scene: u8) -> Result<(), String> {
        let accepts = |m: &[u8]| {
            matches!(
                scenes::reply(m),
                Some(scenes::Reply::LoadCompleted | scenes::Reply::LoadError)
            )
        };
        match scenes::reply(&self.exchange(scenes::scene_change_request(scene), accepts)?) {
            Some(scenes::Reply::LoadCompleted) => Ok(()),
            _ => Err(t!("core.scenes.loadError")),
        }
    }

    /// Reads one of the padKONTROL's 16 scenes (0 to 15).
    fn read_scene(&self, scene: u8) -> Result<scenes::Scene, String> {
        self.change_scene(scene)?;
        let dump = self.exchange(scenes::current_scene_request(), scenes::is_scene_dump)?;
        scenes::Scene::from_dump(&dump).ok_or_else(|| t!("core.scenes.unreadable"))
    }

    /// Stores a scene into the padKONTROL's memory, for good.
    fn store_scene(&self, index: u8, scene: &scenes::Scene) -> Result<(), String> {
        let loaded = |m: &[u8]| {
            matches!(
                scenes::reply(m),
                Some(scenes::Reply::LoadCompleted | scenes::Reply::LoadError | scenes::Reply::FormatError)
            )
        };
        if scenes::reply(&self.exchange(scene.dump(), loaded)?) != Some(scenes::Reply::LoadCompleted) {
            return Err(t!("core.scenes.loadError"));
        }
        let written = |m: &[u8]| {
            matches!(
                scenes::reply(m),
                Some(scenes::Reply::WriteCompleted | scenes::Reply::WriteError)
            )
        };
        match scenes::reply(&self.exchange(scenes::scene_write_request(index), written)?) {
            Some(scenes::Reply::WriteCompleted) => Ok(()),
            _ => Err(t!("core.scenes.writeError")),
        }
    }

    /// Runs scene work with no connect or disconnect meanwhile, then puts
    /// the padKONTROL back on scene 1 and the engine's LEDs and display.
    fn with_scenes<T>(&self, work: impl FnOnce(&Self) -> Result<T, String>) -> Result<T, String> {
        let Some(_operation) = self.operations(OPERATION_WAIT) else {
            return Err(midi_service_frozen());
        };
        let result = work(self);
        let _ = self.change_scene(0);
        if let Some(connection) = self.current.lock().unwrap().as_ref() {
            let mut shared = connection.shared.lock().unwrap();
            if let Some(refresh) = shared.engine.as_mut().map(Engine::refresh) {
                shared.send_pad(&refresh);
            }
        }
        result
    }

    /// The padKONTROL's 16 scenes. It ends on scene 1.
    pub fn read_scenes(&self) -> Result<Vec<scenes::Scene>, String> {
        self.with_scenes(|this| (0..scenes::SCENE_COUNT).map(|scene| this.read_scene(scene)).collect())
    }

    /// Stores 16 scenes, from a backup, into the padKONTROL's memory.
    pub fn restore_scenes(&self, backup: &scenes::Backup) -> Result<(), String> {
        if backup.scenes.len() != usize::from(scenes::SCENE_COUNT) {
            return Err(t!("core.scenes.badBackup"));
        }
        self.with_scenes(|this| {
            for (index, scene) in (0..).zip(&backup.scenes) {
                this.store_scene(index, scene)?;
            }
            Ok(())
        })
    }

    /// Writes a profile bank into one of the padKONTROL's scenes, keeping
    /// what a bank does not set. Returns what the scene could not take.
    pub fn write_bank_to_scene(&self, bank: &Bank, index: u8) -> Result<Vec<scenes::Left>, String> {
        self.with_scenes(|this| {
            let mut scene = this.read_scene(index)?;
            let left = scenes::apply_bank(&mut scene.data, bank);
            this.store_scene(index, &scene)?;
            Ok(left)
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

/// Shortest sleep of the engine ticker, so that a deadline already passed
/// does not make it spin.
const ENGINE_TICK_MIN: std::time::Duration = std::time::Duration::from_millis(1);
/// Longest sleep, with nothing to time.
const TICKER_IDLE: std::time::Duration = std::time::Duration::from_millis(250);

/// Ticks the engine: plays rolls and flam notes on time, and brings the bank
/// back on the display after a value. Stops on its own once the connection
/// is gone.
fn spawn_engine_ticker(app: AppHandle, shared: std::sync::Weak<Mutex<Shared>>) -> std::thread::Thread {
    std::thread::spawn(move || {
        while let Some(shared) = shared.upgrade() {
            let now = std::time::Instant::now();
            let (sent, next) = {
                let mut shared = shared.lock().unwrap();
                match shared.engine.as_mut() {
                    Some(engine) => {
                        let out = engine.tick(now);
                        let next = engine.next_deadline();
                        shared.send_pad(&out.pad);
                        (shared.send_daw(&out.daw), next)
                    }
                    None => (vec![], None),
                }
            };
            drop(shared);
            emit_messages(&app, sent);
            // Until the next roll, flam note or display change, or until
            // woken by an event that may bring one; checks the connection
            // is still there now and then.
            let wait = next.map_or(TICKER_IDLE, |at| {
                at.saturating_duration_since(std::time::Instant::now())
            });
            std::thread::park_timeout(wait.clamp(ENGINE_TICK_MIN, TICKER_IDLE));
        }
    })
    .thread()
    .clone()
}

/// How often the OSC listener checks that its connection is still there.
const OSC_LISTEN_POLL: std::time::Duration = std::time::Duration::from_millis(100);

/// Listens on this computer only unless `network`: anyone on the network
/// could otherwise drive the LEDs, the display and the banks.
fn bind_osc_listener(port: u16, network: bool) -> Result<std::net::UdpSocket, String> {
    let host = if network { "0.0.0.0" } else { "127.0.0.1" };
    // On a quick reconnect, the previous listener can hold the port for one
    // more poll before it notices and stops.
    let mut attempt = 0;
    let socket = loop {
        match std::net::UdpSocket::bind((host, port)) {
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
                if ui_visible() {
                    let _ = app.emit(MESSAGE_EVENT, MidiMessage::osc(Direction::OscIn, &message));
                }
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
        if let Some(channel) = padkontrol::native_mode_ack_channel(bytes) {
            shared.lock().unwrap().detected_channel.get_or_insert(channel);
            return;
        }
        {
            let mut shared = shared.lock().unwrap();
            if shared.waiter.as_ref().is_some_and(|waiter| (waiter.accepts)(bytes)) {
                let waiter = shared.waiter.take().expect("checked just above");
                let _ = waiter.answer.send(bytes.to_vec());
            }
        }
        let event = padkontrol::decode(bytes);
        // Described for the monitor only when the window shows.
        let incoming = || MidiMessage::new(Direction::In, bytes);
        // Outside native mode the padKONTROL plays its own scenes: they go
        // on to the DAW as they are, and their notes and CCs to the OSC mirror.
        if event.is_none() {
            let mut sent = vec![];
            {
                let mut shared = shared.lock().unwrap();
                if shared.thru {
                    sent = shared.send_daw(&[bytes.to_vec()]);
                    let mirrored = shared
                        .osc
                        .as_ref()
                        .filter(|osc| osc.mirror)
                        .and_then(|osc| osc::mirror_midi(&osc.prefix, bytes));
                    sent.extend(shared.send_osc(&Vec::from_iter(mirrored), None, None, None));
                }
            }
            emit_messages(&app, ui_visible().then(incoming).into_iter().chain(sent));
            return;
        }
        let mut sent = vec![];
        let mut state = None;
        let mut switch_profile = None;
        let mut position = None;
        if let Some(event) = &event {
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
                    position = out.encoder_position;
                    shared.send_pad(&out.pad);
                    sent = shared.send_daw(&out.daw);
                    keys::send(&out.keys);
                    if ui_visible() {
                        sent.extend(out.keys.iter().map(MidiMessage::key));
                    }
                    switch_profile.clone_from(&out.switch_profile);
                    (out.osc.as_slice(), out.encoder_position)
                }
                None => (&[][..], None),
            };
            // The mirror runs with or without a profile.
            let bank = state.as_ref().map(|s: &EngineState| s.bank);
            sent.extend(shared.send_osc(osc_messages, Some(event), encoder_position, bank));
            // A roll or a flam may have started.
            shared.wake_ticker();
        }
        if ui_visible() {
            let mut incoming = incoming();
            incoming.encoder_position = position;
            emit_messages(&app, std::iter::once(incoming).chain(sent));
        }
        if let Some(state) = state {
            let _ = app.emit(ENGINE_EVENT, Some(state));
        }
        if let Some(file) = switch_profile {
            // Reads the file and takes the connection's locks: not from here.
            let app = app.clone();
            std::thread::spawn(move || switch_to_profile(&app, &file));
        }
    })
}

/// A `switchProfile` action: makes `file` the active profile.
fn switch_to_profile(app: &AppHandle, file: &str) {
    let result = app
        .state::<ProfileStore>()
        .load(file)
        .map_err(|errors| {
            let errors = errors.join("\n");
            t!("core.app.invalidProfile", file, errors)
        })
        .and_then(|profile| app.state::<PadConnection>().set_profile(file, profile));
    match result {
        Ok(state) => {
            let _ = app.emit(ENGINE_EVENT, Some(state));
        }
        Err(problem) => {
            let _ = app.emit(CONNECTION_PROBLEM_EVENT, problem);
        }
    }
}

/// Messages sent back by the DAW: LEDs in `follow` mode.
fn feedback_handler(app: AppHandle, shared: Arc<Mutex<Shared>>) -> Handler {
    Box::new(move |bytes| {
        {
            let mut shared = shared.lock().unwrap();
            if let Some(engine) = shared.engine.as_mut() {
                let leds = engine.feedback(bytes);
                shared.send_pad(&leds);
                // The clock may change the roll's pace.
                shared.wake_ticker();
            }
        }
        // Clock pulses and active sensing would flood the monitor.
        if !matches!(bytes, [0xF8] | [0xFE]) && ui_visible() {
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
    fn writer_sends_in_order_and_closes_its_port() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let port_open = Arc::new(AtomicBool::new(true));
        struct Port(Arc<AtomicBool>);
        impl Drop for Port {
            fn drop(&mut self) {
                // A slow driver closing the port.
                std::thread::sleep(std::time::Duration::from_millis(50));
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let port = Port(port_open.clone());
        let log = sent.clone();
        let writer = Writer::spawn(sink(move |message| {
            let _keep = &port;
            std::thread::sleep(std::time::Duration::from_millis(5));
            log.lock().unwrap().push(message.to_vec());
        }));
        for i in 0..10u8 {
            writer.send(vec![0x90, i, 100]);
        }
        assert!(writer.flush(std::time::Duration::from_secs(2)));
        let sent_now: Vec<u8> = sent.lock().unwrap().iter().map(|m| m[1]).collect();
        assert_eq!(
            sent_now,
            (0..10).collect::<Vec<u8>>(),
            "everything queued before the flush, in order"
        );

        writer.send(vec![0x80, 0, 0]);
        writer.close(std::time::Duration::from_secs(2));
        assert_eq!(sent.lock().unwrap().len(), 11, "closing sends what is queued");
        assert!(!port_open.load(Ordering::SeqCst), "and waits for the port to close");
    }

    #[test]
    fn stuck_midi_work_is_not_piled_up() {
        let ms = std::time::Duration::from_millis;
        assert_eq!(with_timeout(ms(500), || 1), Some(1));
        // Stuck past its timeout: later work is refused without a thread.
        assert_eq!(with_timeout(ms(20), move || std::thread::sleep(ms(300))), None);
        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        assert_eq!(with_timeout(ms(500), move || flag.store(true, Ordering::SeqCst)), None);
        assert!(!ran.load(Ordering::SeqCst), "refused work does not run");
        // Once the stuck work is over, work runs again.
        std::thread::sleep(ms(400));
        assert_eq!(with_timeout(ms(500), || 2), Some(2));
    }

    #[test]
    fn writer_reports_a_port_slow_to_close() {
        struct SlowPort;
        impl Drop for SlowPort {
            fn drop(&mut self) {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        let port = SlowPort;
        let writer = Writer::spawn(sink(move |_| {
            let _keep = &port;
        }));
        let closing = writer
            .close(std::time::Duration::from_millis(20))
            .expect("still closing");
        assert!(
            closing.recv_timeout(std::time::Duration::from_secs(2)).is_ok(),
            "then closed"
        );
    }

    #[test]
    fn writer_flush_gives_up_on_a_stalled_output() {
        let writer = Writer::spawn(sink(|_| std::thread::sleep(std::time::Duration::from_millis(500))));
        writer.send(vec![0xF0, 0xF7]);
        assert!(!writer.flush(std::time::Duration::from_millis(50)));
    }

    #[test]
    fn osc_listener_receives_and_frees_its_port() {
        let socket = bind_osc_listener(0, false).unwrap();
        let port = socket.local_addr().unwrap().port();
        let sender = OscSender::open("127.0.0.1", port).unwrap();
        assert!(sender.send(&OscMessage::new("/tk/bank", vec![osc::OscArg::Int(1)])));
        let mut buffer = [0u8; 512];
        let (size, _) = socket.recv_from(&mut buffer).unwrap();
        assert_eq!(osc::decode(&buffer[..size])[0].describe(), "/tk/bank 1");

        // A second listener on a taken port reports it.
        assert!(bind_osc_listener(port, false).is_err());
        drop(socket);
        assert!(bind_osc_listener(port, false).is_ok());
    }

    /// Diagnostic tool: `cargo test print_ports -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_ports() {
        println!("{:#?}", super::list_ports().unwrap());
    }
}
