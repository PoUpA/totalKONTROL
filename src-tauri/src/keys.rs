//! Keyboard shortcuts of `key` actions: parsing of texts such as
//! "ctrl+shift+s", and a thread that types them through the system.

use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    /// Windows key, Command on macOS, Super on Linux.
    Meta,
    /// Command on macOS, Ctrl elsewhere: the key of most shortcuts, so a
    /// profile works on both.
    Primary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character key, typed as is: shortcuts follow the keyboard layout.
    Char(char),
    Space,
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    /// F1 to F20.
    F(u8),
    /// Numeric keypad digit.
    Numpad(u8),
    /// Numeric keypad *, /, +, - and the decimal point.
    NumpadMultiply,
    NumpadDivide,
    NumpadAdd,
    NumpadSubtract,
    NumpadDecimal,
    CapsLock,
    PrintScreen,
    Pause,
    VolumeUp,
    VolumeDown,
    Mute,
    PlayPause,
    NextTrack,
    PreviousTrack,
}

/// Modifiers held around an optional key: "ctrl+s", "f5", or "shift" alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    pub modifiers: Vec<Modifier>,
    pub key: Option<Key>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeysError {
    Empty,
    Unknown(String),
    SeveralKeys,
}

fn modifier(name: &str) -> Option<Modifier> {
    Some(match name {
        "ctrl" | "control" => Modifier::Ctrl,
        "shift" => Modifier::Shift,
        "alt" | "option" => Modifier::Alt,
        "meta" | "win" | "cmd" | "command" | "super" => Modifier::Meta,
        "cmdorctrl" | "primary" => Modifier::Primary,
        _ => return None,
    })
}

fn key(name: &str) -> Option<Key> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return (!c.is_whitespace()).then_some(Key::Char(c));
    }
    let numbered = |prefix: &str, range: std::ops::RangeInclusive<u8>| {
        name.strip_prefix(prefix)
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| range.contains(n))
    };
    if let Some(n) = numbered("f", 1..=20) {
        return Some(Key::F(n));
    }
    if let Some(n) = numbered("num", 0..=9) {
        return Some(Key::Numpad(n));
    }
    if let Some(op) = name.strip_prefix("num") {
        return match op {
            "*" => Some(Key::NumpadMultiply),
            "/" => Some(Key::NumpadDivide),
            "+" => Some(Key::NumpadAdd),
            "-" => Some(Key::NumpadSubtract),
            "." => Some(Key::NumpadDecimal),
            _ => None,
        };
    }
    Some(match name {
        "plus" => Key::Char('+'),
        "minus" => Key::Char('-'),
        "space" => Key::Space,
        "enter" | "return" => Key::Enter,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" | "ins" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "capslock" => Key::CapsLock,
        "printscreen" | "prtsc" => Key::PrintScreen,
        "pause" => Key::Pause,
        "volumeup" => Key::VolumeUp,
        "volumedown" => Key::VolumeDown,
        "mute" => Key::Mute,
        "playpause" => Key::PlayPause,
        "nexttrack" | "next" => Key::NextTrack,
        "prevtrack" | "previous" => Key::PreviousTrack,
        _ => return None,
    })
}

/// Parses "ctrl+shift+s": names separated by `+`, in any case, with at most
/// one key that is not a modifier. `plus` stands for the + key.
pub fn parse(text: &str) -> Result<Combo, KeysError> {
    if text.trim().is_empty() {
        return Err(KeysError::Empty);
    }
    let mut combo = Combo {
        modifiers: vec![],
        key: None,
    };
    for part in text.split('+') {
        let name = part.trim().to_lowercase();
        if let Some(m) = modifier(&name) {
            if !combo.modifiers.contains(&m) {
                combo.modifiers.push(m);
            }
        } else if let Some(k) = key(&name) {
            if combo.key.replace(k).is_some() {
                return Err(KeysError::SeveralKeys);
            }
        } else {
            return Err(KeysError::Unknown(part.trim().to_string()));
        }
    }
    Ok(combo)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyDirection {
    /// Pressed and released at once.
    Tap,
    /// Held until the matching `Release`.
    Press,
    Release,
}

/// Shortcut to type, produced by the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub keys: String,
    pub direction: KeyDirection,
}

impl KeyEvent {
    pub fn new(keys: &str, direction: KeyDirection) -> Self {
        Self {
            keys: keys.to_string(),
            direction,
        }
    }

    /// Text for the monitor.
    pub fn describe(&self) -> String {
        let arrow = match self.direction {
            KeyDirection::Tap => "",
            KeyDirection::Press => " ↓",
            KeyDirection::Release => " ↑",
        };
        format!("{}{arrow}", self.keys)
    }
}

enum Command {
    Type(KeyEvent),
    ReleaseAll,
}

/// Started with the first shortcut, so that releasing alone starts nothing.
static SENDER: OnceLock<Sender<Command>> = OnceLock::new();

fn sender() -> &'static Sender<Command> {
    SENDER.get_or_init(|| {
        let (tx, rx) = channel();
        std::thread::spawn(move || Typist::default().run(rx));
        tx
    })
}

/// Types the shortcuts, without blocking the MIDI callbacks.
pub fn send(events: &[KeyEvent]) {
    for event in events {
        let _ = sender().send(Command::Type(event.clone()));
    }
}

/// Releases the keys still held, when the connection or the profile goes.
pub fn release_all() {
    if let Some(sender) = SENDER.get() {
        let _ = sender.send(Command::ReleaseAll);
    }
}

/// Wait before trying again to reach the system keyboard, for instance
/// after the user grants the permission on macOS.
const RETRY_AFTER: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Typist {
    enigo: Option<enigo::Enigo>,
    failed_at: Option<Instant>,
    /// Keys pressed by `Press` events, in order.
    held: Vec<enigo::Key>,
}

impl Typist {
    fn run(mut self, rx: std::sync::mpsc::Receiver<Command>) {
        for command in rx {
            self.reach_keyboard();
            let Some(enigo) = self.enigo.as_mut() else { continue };
            match command {
                Command::Type(event) => {
                    let Ok(combo) = parse(&event.keys) else { continue };
                    let keys = system_keys(&combo);
                    type_keys(enigo, &keys, event.direction, &mut self.held);
                }
                Command::ReleaseAll => {
                    for key in self.held.drain(..).rev() {
                        let _ = enigo::Keyboard::key(enigo, key, enigo::Direction::Release);
                    }
                }
            }
        }
    }

    fn reach_keyboard(&mut self) {
        if self.enigo.is_none() && self.failed_at.is_none_or(|at| at.elapsed() > RETRY_AFTER) {
            let settings = enigo::Settings {
                release_keys_when_dropped: true,
                ..Default::default()
            };
            match enigo::Enigo::new(&settings) {
                Ok(enigo) => self.enigo = Some(enigo),
                Err(error) => {
                    // Reported once per failure streak, not at every retry.
                    if self.failed_at.is_none() {
                        crate::diag::report(t!("core.keys.unavailable", error));
                    }
                    self.failed_at = Some(Instant::now());
                }
            }
        }
    }
}

/// The first key the system refused, reported; later ones only logged.
static KEY_REFUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn checked(result: Result<(), enigo::InputError>) {
    if let Err(error) = result {
        if KEY_REFUSED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            crate::diag::log(format!("key refused: {error}"));
        } else {
            crate::diag::report(t!("core.keys.refused", error));
        }
    }
}

fn type_keys(enigo: &mut enigo::Enigo, keys: &[enigo::Key], direction: KeyDirection, held: &mut Vec<enigo::Key>) {
    use enigo::{Direction, Keyboard};
    match direction {
        KeyDirection::Tap => {
            let Some((last, modifiers)) = keys.split_last() else {
                return;
            };
            for &key in modifiers {
                checked(enigo.key(key, Direction::Press));
            }
            checked(enigo.key(*last, Direction::Click));
            for &key in modifiers.iter().rev() {
                checked(enigo.key(key, Direction::Release));
            }
        }
        KeyDirection::Press => {
            for &key in keys {
                checked(enigo.key(key, Direction::Press));
                held.push(key);
            }
        }
        KeyDirection::Release => {
            for &key in keys.iter().rev() {
                checked(enigo.key(key, Direction::Release));
                if let Some(i) = held.iter().rposition(|&k| k == key) {
                    held.remove(i);
                }
            }
        }
    }
}

/// Modifiers, then the key, as the system names them. Keys a system does
/// not have are left out.
fn system_keys(combo: &Combo) -> Vec<enigo::Key> {
    use enigo::Key as K;
    let modifiers = combo.modifiers.iter().map(|m| match m {
        Modifier::Ctrl => K::Control,
        Modifier::Shift => K::Shift,
        Modifier::Alt => K::Alt,
        Modifier::Meta => K::Meta,
        #[cfg(target_os = "macos")]
        Modifier::Primary => K::Meta,
        #[cfg(not(target_os = "macos"))]
        Modifier::Primary => K::Control,
    });
    modifiers.chain(combo.key.and_then(system_key)).collect()
}

/// Windows virtual keys of the "=+" and "-_" keys.
#[cfg(windows)]
const VK_OEM_PLUS: u32 = 0xBB;
#[cfg(windows)]
const VK_OEM_MINUS: u32 = 0xBD;

/// The system's name of a key, if it has one.
fn system_key(key: Key) -> Option<enigo::Key> {
    use enigo::Key as K;
    Some(match key {
        // On Windows, enigo maps '+' and '-' with the Shift state of the
        // layout's key, so ctrl+plus would arrive as ctrl+shift+=: the keys
        // themselves go instead, which is what shortcuts mean.
        #[cfg(windows)]
        Key::Char('+') => K::Other(VK_OEM_PLUS),
        #[cfg(windows)]
        Key::Char('-') => K::Other(VK_OEM_MINUS),
        Key::Char(c) => K::Unicode(c),
        Key::Space => K::Space,
        Key::Enter => K::Return,
        Key::Tab => K::Tab,
        Key::Escape => K::Escape,
        Key::Backspace => K::Backspace,
        Key::Delete => K::Delete,
        Key::Home => K::Home,
        Key::End => K::End,
        Key::PageUp => K::PageUp,
        Key::PageDown => K::PageDown,
        Key::Up => K::UpArrow,
        Key::Down => K::DownArrow,
        Key::Left => K::LeftArrow,
        Key::Right => K::RightArrow,
        Key::F(n) => [
            K::F1,
            K::F2,
            K::F3,
            K::F4,
            K::F5,
            K::F6,
            K::F7,
            K::F8,
            K::F9,
            K::F10,
            K::F11,
            K::F12,
            K::F13,
            K::F14,
            K::F15,
            K::F16,
            K::F17,
            K::F18,
            K::F19,
            K::F20,
        ][usize::from(n - 1)],
        Key::Numpad(n) => [
            K::Numpad0,
            K::Numpad1,
            K::Numpad2,
            K::Numpad3,
            K::Numpad4,
            K::Numpad5,
            K::Numpad6,
            K::Numpad7,
            K::Numpad8,
            K::Numpad9,
        ][usize::from(n)],
        Key::NumpadMultiply => K::Multiply,
        Key::NumpadDivide => K::Divide,
        Key::NumpadAdd => K::Add,
        Key::NumpadSubtract => K::Subtract,
        Key::NumpadDecimal => K::Decimal,
        Key::CapsLock => K::CapsLock,
        Key::VolumeUp => K::VolumeUp,
        Key::VolumeDown => K::VolumeDown,
        Key::Mute => K::VolumeMute,
        Key::PlayPause => K::MediaPlayPause,
        Key::NextTrack => K::MediaNextTrack,
        Key::PreviousTrack => K::MediaPrevTrack,
        #[cfg(not(target_os = "macos"))]
        Key::Insert => K::Insert,
        #[cfg(not(target_os = "macos"))]
        Key::PrintScreen => K::PrintScr,
        #[cfg(not(target_os = "macos"))]
        Key::Pause => K::Pause,
        #[cfg(target_os = "macos")]
        Key::Insert | Key::PrintScreen | Key::Pause => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        assert_eq!(
            parse("Ctrl+Shift+S"),
            Ok(Combo {
                modifiers: vec![Modifier::Ctrl, Modifier::Shift],
                key: Some(Key::Char('s'))
            })
        );
        assert_eq!(
            parse("cmd + plus").unwrap(),
            Combo {
                modifiers: vec![Modifier::Meta],
                key: Some(Key::Char('+'))
            }
        );
        assert_eq!(parse("f12").unwrap().key, Some(Key::F(12)));
        assert_eq!(parse("num7").unwrap().key, Some(Key::Numpad(7)));
        assert_eq!(parse("num*").unwrap().key, Some(Key::NumpadMultiply));
        assert_eq!(parse("num.").unwrap().key, Some(Key::NumpadDecimal));
        assert_eq!(parse("CmdOrCtrl+s").unwrap().modifiers, vec![Modifier::Primary]);
        assert_eq!(parse("numx"), Err(KeysError::Unknown("numx".into())));
        assert_eq!(parse("space").unwrap().key, Some(Key::Space));
        assert_eq!(parse("é").unwrap().key, Some(Key::Char('é')));
        assert_eq!(
            parse("shift"),
            Ok(Combo {
                modifiers: vec![Modifier::Shift],
                key: None
            })
        );
    }

    #[test]
    fn rejects_bad_shortcuts() {
        assert_eq!(parse(" "), Err(KeysError::Empty));
        assert_eq!(parse("ctrl+banana"), Err(KeysError::Unknown("banana".into())));
        assert_eq!(parse("ctrl++"), Err(KeysError::Unknown(String::new())));
        assert_eq!(parse("f21"), Err(KeysError::Unknown("f21".into())));
        assert_eq!(parse("a+b"), Err(KeysError::SeveralKeys));
    }

    #[test]
    fn every_key_reaches_the_system() {
        let combo = parse("ctrl+alt+f20").unwrap();
        assert_eq!(system_keys(&combo).len(), 3);
        assert_eq!(system_keys(&parse("num0").unwrap()), vec![enigo::Key::Numpad0]);
        let primary = if cfg!(target_os = "macos") {
            enigo::Key::Meta
        } else {
            enigo::Key::Control
        };
        assert_eq!(
            system_keys(&parse("cmdorctrl+num*").unwrap()),
            vec![primary, enigo::Key::Multiply]
        );
        #[cfg(windows)]
        assert_eq!(
            system_keys(&parse("ctrl+plus").unwrap()),
            vec![enigo::Key::Control, enigo::Key::Other(VK_OEM_PLUS)]
        );
    }
}
