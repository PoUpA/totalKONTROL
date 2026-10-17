//! Virtual MIDI device on Windows through Windows MIDI Services (WinRT API
//! `Windows.Devices.Midi2`), so loopMIDI is not needed. A single
//! bidirectional "totalKONTROL" device: the DAW reads what the app sends
//! from it, and what the DAW writes to it comes back to the app (LED feedback).
//!
//! The service shipped with Windows before the late November 2026 update has
//! a bug (microsoft/MIDI#1047): closing a virtual device hangs the service,
//! and only the first creation after it starts succeeds. So the device is
//! created only once, never closed (it goes away with the process), and
//! creation has a timeout.

#[allow(non_snake_case, non_upper_case_globals, non_camel_case_types, dead_code, clippy::all)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/midi2.rs"));
}

use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use bindings::Windows::Devices::Midi2::Enumeration::{
    MidiDeclaredEndpointInfo, MidiFunctionBlock, MidiFunctionBlockDirection,
    MidiFunctionBlockRepresentsMidi10Connection,
};
use bindings::Windows::Devices::Midi2::Transports::Virtual::{
    MidiVirtualDevice, MidiVirtualDeviceCreationConfig, MidiVirtualDeviceManager,
};
use bindings::Windows::Devices::Midi2::{
    IMidiMessageReceivedEventSource, MidiApi, MidiEndpointConnection, MidiGroup, MidiMessageProcessingPluginAddResult,
    MidiMessageReceivedEventArgs, MidiSession,
};
use windows::Foundation::TypedEventHandler;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows_core::{HSTRING, Ref};

use crate::midi::Handler;

pub const DEVICE_NAME: &str = "totalKONTROL";
const CREATION_TIMEOUT: Duration = Duration::from_secs(8);
fn service_blocked() -> String {
    t!("core.winmidi.serviceBlocked")
}

type Receiver = Arc<Mutex<Option<Handler>>>;

pub struct VirtualDevice {
    connection: MidiEndpointConnection,
    receiver: Receiver,
    // Kept alive along with the device.
    _session: MidiSession,
    _device: MidiVirtualDevice,
}

// Windows.Devices.Midi2 objects are agile: usable from any thread
// in the multithreaded apartment.
unsafe impl Send for VirtualDevice {}
unsafe impl Sync for VirtualDevice {}

impl VirtualDevice {
    /// Sends a MIDI 1.0 message to the DAW. SysEx and system messages are
    /// not forwarded.
    pub fn send(&self, message: &[u8]) -> bool {
        match to_ump(message) {
            Some(word) => {
                self.connection.SendSingleMessageWords(0, word);
                true
            }
            None => false,
        }
    }

    /// Hooks up handling of messages sent back by the DAW. Feedback stops
    /// when the returned guard is dropped.
    pub fn listen(self: &Arc<Self>, handler: Handler) -> Listener {
        *self.receiver.lock().unwrap() = Some(handler);
        Listener { device: self.clone() }
    }
}

pub struct Listener {
    device: Arc<VirtualDevice>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        *self.device.receiver.lock().unwrap() = None;
    }
}

/// Runs `f` on a thread in the multithreaded apartment. The thread stays
/// alive afterwards: it keeps the apartment alive for the other threads.
fn in_mta<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let _ = tx.send(f());
        loop {
            std::thread::park();
        }
    });
    rx
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Ready,
    /// The installed virtual transport has bug microsoft/MIDI#1047: creating
    /// a device a second time after the service starts freezes the whole
    /// Windows MIDI stack, WinMM included, for every app.
    NeedsWindowsUpdate,
    /// No Windows MIDI Services (API DLL missing, Windows too old…).
    Unavailable,
}

pub fn needs_windows_update() -> String {
    t!("core.winmidi.needsWindowsUpdate")
}

/// Last virtual transport version with bug microsoft/MIDI#1047.
const LAST_BUGGY_TRANSPORT: (u16, u16, u16) = (1, 0, 15);

pub fn availability() -> Availability {
    static AVAILABILITY: OnceLock<Availability> = OnceLock::new();
    *AVAILABILITY.get_or_init(|| {
        match transport_version() {
            None => return Availability::Unavailable,
            Some(version) if version <= LAST_BUGGY_TRANSPORT => return Availability::NeedsWindowsUpdate,
            Some(_) => {}
        }
        let ready = in_mta(|| {
            std::panic::catch_unwind(|| {
                MidiApi::EnsureServiceAvailable() && MidiVirtualDeviceManager::IsTransportAvailable()
            })
            .unwrap_or(false)
        })
        .recv_timeout(CREATION_TIMEOUT)
        .unwrap_or(false);
        if ready {
            Availability::Ready
        } else {
            Availability::Unavailable
        }
    })
}

/// Version (major, minor, build) of the virtual transport shipped in Windows.
fn transport_version() -> Option<(u16, u16, u16)> {
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
    };
    use windows_core::PCWSTR;

    let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let path = HSTRING::from(format!(r"{system}\System32\Midi2.VirtualMidiTransport.dll"));
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(path.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;
        let mut info: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let root = HSTRING::from("\\");
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(root.as_ptr()), &mut info, &mut len).as_bool() || info.is_null()
        {
            return None;
        }
        let info = &*(info as *const VS_FIXEDFILEINFO);
        Some((
            (info.dwFileVersionMS >> 16) as u16,
            info.dwFileVersionMS as u16,
            (info.dwFileVersionLS >> 16) as u16,
        ))
    }
}

/// The virtual device, created on the first call. A failure is permanent for
/// the app's lifetime: after the service hangs, a new attempt would hang
/// too.
pub fn device() -> Result<Arc<VirtualDevice>, String> {
    static DEVICE: Mutex<Option<Result<Arc<VirtualDevice>, String>>> = Mutex::new(None);
    let mut slot = DEVICE.lock().unwrap();
    if let Some(result) = slot.as_ref() {
        return result.clone();
    }
    let result = in_mta(create)
        .recv_timeout(CREATION_TIMEOUT)
        .unwrap_or_else(|_| Err(service_blocked()))
        .map(Arc::new);
    *slot = Some(result.clone());
    result
}

/// Prefixes a WinRT error with the step that failed.
fn error(what: String) -> impl Fn(windows_core::Error) -> String {
    move |e| t!("core.winmidi.stepFailed", error = e, what)
}

fn create() -> Result<VirtualDevice, String> {
    let session = MidiSession::Create(&HSTRING::from(DEVICE_NAME)).ok_or_else(|| t!("core.winmidi.sessionFailed"))?;

    let info = MidiDeclaredEndpointInfo::new().map_err(error(t!("core.winmidi.stepDeviceDescription")))?;
    info.SetName(&HSTRING::from(DEVICE_NAME));
    info.SetProductInstanceId(&HSTRING::from("totalKONTROL-1"));
    info.SetSupportsMidi10Protocol(true);
    info.SetSupportsMidi20Protocol(false);
    info.SetHasStaticFunctionBlocks(true);
    info.SetDeclaredFunctionBlockCount(1);
    info.SetSpecificationVersionMajor(1);
    info.SetSpecificationVersionMinor(1);

    let config = MidiVirtualDeviceCreationConfig::CreateInstance(
        &HSTRING::from(DEVICE_NAME),
        &HSTRING::from(t!("core.winmidi.productDescription")),
        &HSTRING::from("totalKONTROL"),
        &info,
    )
    .map_err(error(t!("core.winmidi.stepDeviceConfiguration")))?;

    // One bidirectional block on group 1: one MIDI 1.0 input and one output
    // on the DAW side.
    let block = MidiFunctionBlock::new().map_err(error(t!("core.winmidi.stepFunctionBlock")))?;
    block.SetNumber(0);
    block.SetIsActive(true);
    block.SetName(&HSTRING::from(DEVICE_NAME));
    block.SetFirstGroup(&MidiGroup::CreateInstance(0).map_err(error(t!("core.winmidi.stepMidiGroup")))?);
    block.SetGroupCount(1);
    block.SetDirection(MidiFunctionBlockDirection::Bidirectional);
    block.SetRepresentsMidi10Connection(MidiFunctionBlockRepresentsMidi10Connection::YesBandwidthUnrestricted);
    config
        .FunctionBlocks()
        .ok_or_else(|| t!("core.winmidi.blockListMissing"))?
        .Append(&block)
        .map_err(error(t!("core.winmidi.stepAddBlock")))?;

    let device =
        MidiVirtualDeviceManager::CreateVirtualDevice(&config).ok_or_else(|| t!("core.winmidi.deviceRefused"))?;
    // MIDI 2.0 discovery messages are handled by the device itself.
    device.SetSuppressHandledMessages(true);

    let connection = session
        .CreateEndpointConnection(&device.DeviceEndpointDeviceId())
        .ok_or_else(|| t!("core.winmidi.connectFailed"))?;

    let receiver: Receiver = Arc::default();
    let incoming = receiver.clone();
    connection
        .MessageReceived(&TypedEventHandler::new(
            move |_: Ref<IMidiMessageReceivedEventSource>, args: Ref<MidiMessageReceivedEventArgs>| {
                if let (Some(args), Some(handler)) = (args.as_ref(), incoming.lock().unwrap().as_mut())
                    && let Some(message) = from_ump(args.PeekFirstWord())
                {
                    handler(&message);
                }
                Ok(())
            },
        ))
        .map_err(error(t!("core.winmidi.stepMessageSubscription")))?;

    // Without this plugin, the device sends but never receives anything.
    if connection.AddMessageProcessingPlugin(&device) != MidiMessageProcessingPluginAddResult::Succeeded {
        return Err(t!("core.winmidi.hookupRefused"));
    }
    if !connection.Open() {
        return Err(t!("core.winmidi.openFailed"));
    }

    Ok(VirtualDevice {
        connection,
        receiver,
        _session: session,
        _device: device,
    })
}

/// MIDI 1.0 message → type 2 UMP (MIDI 1.0 channel voice) on group 1.
fn to_ump(message: &[u8]) -> Option<u32> {
    let status = *message.first()?;
    if !(0x80..0xF0).contains(&status) {
        return None;
    }
    let data = |i: usize| message.get(i).copied().unwrap_or(0) as u32 & 0x7F;
    let second = if matches!(status & 0xF0, 0xC0 | 0xD0) {
        0
    } else {
        data(2)
    };
    Some(0x2000_0000 | (status as u32) << 16 | data(1) << 8 | second)
}

/// Type 1 (system: clock, start, stop…) or type 2 (channel voice) UMP →
/// MIDI 1.0 message. Other types are ignored.
fn from_ump(word: u32) -> Option<Vec<u8>> {
    let status = (word >> 16) as u8;
    let (data1, data2) = ((word >> 8) as u8 & 0x7F, word as u8 & 0x7F);
    match word >> 28 {
        0x1 => {
            return match status {
                0xF1 | 0xF3 => Some(vec![status, data1]),
                0xF2 => Some(vec![status, data1, data2]),
                0xF6 | 0xF8..=0xFF => Some(vec![status]),
                _ => None,
            };
        }
        0x2 => {}
        _ => return None,
    }
    let mut message = vec![status, data1, data2];
    if matches!(status & 0xF0, 0xC0 | 0xD0) {
        message.truncate(2);
    }
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostic tool: `cargo test --features windows-midi-services print_transport -- --ignored --nocapture`.
    /// Reads a file version only: it does not touch the MIDI service.
    #[test]
    #[ignore]
    fn print_transport() {
        let version = transport_version();
        println!(
            "transport: {version:?}, buggy: {}",
            version.is_some_and(|v| v <= LAST_BUGGY_TRANSPORT)
        );
    }

    #[test]
    fn converts_midi1_to_ump_and_back() {
        assert_eq!(to_ump(&[0x90, 60, 100]), Some(0x2090_3C64));
        assert_eq!(to_ump(&[0xB1, 7, 127]), Some(0x20B1_077F));
        assert_eq!(to_ump(&[0xC2, 5]), Some(0x20C2_0500));
        assert_eq!(to_ump(&[0xF0, 0x42, 0xF7]), None);
        assert_eq!(to_ump(&[0x3C]), None);

        assert_eq!(from_ump(0x2090_307F), Some(vec![0x90, 0x30, 0x7F]));
        assert_eq!(from_ump(0x20C2_0500), Some(vec![0xC2, 5]));
        assert_eq!(from_ump(0x4090_3000), None);
        assert_eq!(from_ump(0xF000_0000), None);

        // System messages: clock, start, song position.
        assert_eq!(from_ump(0x10F8_0000), Some(vec![0xF8]));
        assert_eq!(from_ump(0x10FA_0000), Some(vec![0xFA]));
        assert_eq!(from_ump(0x10F2_1020), Some(vec![0xF2, 0x10, 0x20]));
        assert_eq!(from_ump(0x10F4_0000), None);
    }
}
