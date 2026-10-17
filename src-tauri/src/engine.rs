//! Mapping engine: applies the active profile to padKONTROL events.
//! It produces MIDI messages for the DAW and LED / display messages
//! for the padKONTROL. No I/O here: wiring to the MIDI ports is done
//! in `midi.rs`.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{
    ButtonAction, DISPLAY_LENGTH, EncoderAction, Encoding, KnobAction, Led, Mode, PadDisplay, Profile, Velocity,
    XyAction,
};
use crate::osc::{self, OscArg, OscMessage};
use crate::padkontrol::{self, Control, ControlEvent};

/// Messages to send in response to an event.
#[derive(Debug, Default, PartialEq)]
pub struct Output {
    pub daw: Vec<Vec<u8>>,
    pub pad: Vec<Vec<u8>>,
    pub bank_changed: bool,
    /// Encoder position after an encoder event, as the display shows it.
    pub encoder_position: Option<i16>,
    /// OSC messages from `osc` actions, for the connection's OSC destination.
    pub osc: Vec<OscMessage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    /// Profile file name in the profile store, empty when unknown.
    pub file: String,
    pub profile: String,
    pub bank: usize,
    pub banks: Vec<String>,
    /// Display text of the active bank.
    pub display: String,
    /// Encoder position in the active bank (see `Engine::encoder`).
    pub encoder: i16,
}

/// Range of the relative encoder counter: what the 3-character display can show.
const RELATIVE_COUNTER: std::ops::RangeInclusive<i16> = -99..=999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FollowKey {
    Note { channel: u8, note: u8 },
    Cc { channel: u8, cc: u8 },
}

pub struct Engine {
    profile: Profile,
    file: String,
    bank: usize,
    /// Pressed controls, with the bank that was active when pressed.
    held: HashMap<Control, usize>,
    /// Toggle-mode controls currently "on", per bank.
    toggles: HashSet<(usize, Control)>,
    /// State of the notes and CCs sent back by the DAW, for `follow` LEDs.
    follow: HashMap<FollowKey, bool>,
    /// State of the OSC addresses received, for `follow` LEDs of `osc` actions.
    osc_follow: HashMap<String, bool>,
    /// LEDs set directly over OSC, until the next bank change.
    led_override: HashMap<Control, bool>,
    /// Encoder position, per bank, shown on the display: the value sent in
    /// absolute mode (0 to 127), a free counter in relative mode (-99 to 999).
    encoder: HashMap<usize, i16>,
    last_xy: Option<(u8, u8)>,
    /// Text currently on the display, to skip resending it.
    shown: String,
    /// When a value shown on the display gives way to the bank again.
    display_until: Option<Instant>,
}

impl Engine {
    pub fn new(profile: Profile) -> Self {
        let bank = profile.start_bank.min(profile.banks.len().saturating_sub(1));
        Self {
            profile,
            file: String::new(),
            bank,
            held: HashMap::new(),
            toggles: HashSet::new(),
            follow: HashMap::new(),
            osc_follow: HashMap::new(),
            led_override: HashMap::new(),
            encoder: HashMap::new(),
            last_xy: None,
            shown: String::new(),
            display_until: None,
        }
    }

    /// Records which profile file this engine runs.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = file.into();
        self
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    /// Swaps in an edited version of the same profile. Stays on the current
    /// bank when it still exists, and keeps the DAW feedback state.
    pub fn reload(&mut self, profile: Profile) {
        let bank = if self.bank < profile.banks.len() { self.bank } else { profile.start_bank };
        let follow = std::mem::take(&mut self.follow);
        let osc_follow = std::mem::take(&mut self.osc_follow);
        *self = Engine::new(profile).with_file(std::mem::take(&mut self.file));
        self.bank = bank;
        self.follow = follow;
        self.osc_follow = osc_follow;
    }

    pub fn state(&self) -> EngineState {
        EngineState {
            file: self.file.clone(),
            profile: self.profile.name.clone(),
            bank: self.bank,
            banks: self.profile.banks.iter().map(|b| b.name.clone()).collect(),
            display: self.bank_text(),
            encoder: self.encoder.get(&self.bank).copied().unwrap_or(0),
        }
    }

    /// LEDs and display of the active bank, to send on connection.
    pub fn refresh(&mut self) -> Vec<Vec<u8>> {
        let mut messages: Vec<Vec<u8>> =
            Control::all().map(|control| padkontrol::led(control, self.led_state(control))).collect();
        messages.push(self.show_bank());
        messages
    }

    /// Text of the active bank on the display: its `display` field, or the
    /// start of its name.
    fn bank_text(&self) -> String {
        let bank = self.bank();
        bank.display.clone().unwrap_or_else(|| bank.name.chars().filter(|c| !c.is_whitespace()).collect())
    }

    fn show_bank(&mut self) -> Vec<u8> {
        self.shown = self.bank_text();
        self.display_until = None;
        padkontrol::display(&self.shown)
    }

    /// Shows a value for the profile's display duration.
    fn show_value(&mut self, text: String, now: Instant, out: &mut Output) {
        self.display_until = Some(now + Duration::from_millis(self.profile.display.duration.into()));
        if text != self.shown {
            out.pad.push(padkontrol::display(&text));
            self.shown = text;
        }
    }

    /// Called regularly: brings the bank back on the display once a value
    /// has been shown long enough.
    pub fn tick(&mut self, now: Instant) -> Vec<Vec<u8>> {
        match self.display_until {
            Some(until) if now >= until => {
                self.display_until = None;
                if self.shown == self.bank_text() { vec![] } else { vec![self.show_bank()] }
            }
            _ => vec![],
        }
    }

    pub fn select_bank(&mut self, bank: usize) -> Output {
        let mut out = Output::default();
        self.switch_bank(bank, &mut out);
        out
    }

    pub fn handle(&mut self, event: &ControlEvent) -> Output {
        self.handle_at(event, Instant::now())
    }

    fn handle_at(&mut self, event: &ControlEvent, now: Instant) -> Output {
        let mut out = Output::default();
        let display = self.profile.display.clone();
        match *event {
            ControlEvent::Pad { pad, pressed: true, velocity } => {
                self.press(Control::Pad(pad), Some(velocity), &mut out);
                let text = match (display.pads, self.action(self.bank, Control::Pad(pad))) {
                    (PadDisplay::Note, Some(ButtonAction::Note { note, .. })) => Some(note_name(*note)),
                    (PadDisplay::Velocity, Some(_)) => Some(format!("{velocity:>DISPLAY_LENGTH$}")),
                    _ => None,
                };
                if let Some(text) = text {
                    self.show_value(text, now, &mut out);
                }
            }
            ControlEvent::Pad { pad, pressed: false, .. } => self.release(Control::Pad(pad), &mut out),
            ControlEvent::Button { button, pressed: true } => self.press(Control::Button(button), None, &mut out),
            ControlEvent::Button { button, pressed: false } => self.release(Control::Button(button), &mut out),
            ControlEvent::XyTouch { pressed: true } => self.press(Control::XyTouch, None, &mut out),
            ControlEvent::XyTouch { pressed: false } => self.release(Control::XyTouch, &mut out),
            ControlEvent::Knob { knob, value } => {
                let shown = match self.bank().knobs.get(&knob).cloned() {
                    Some(KnobAction::Cc { channel, cc, min, max }) => {
                        let scaled = (min as u16 + value as u16 * max.saturating_sub(min) as u16 / 127) as u8;
                        out.daw.push(control_change(channel, cc, scaled));
                        Some(scaled)
                    }
                    Some(KnobAction::Osc { address, min, max, float }) => {
                        out.osc.push(OscMessage::new(address, vec![scale_osc(value, min, max, float)]));
                        Some(value)
                    }
                    None => None,
                };
                if let (Some(shown), true) = (shown, display.knobs) {
                    self.show_value(format!("{shown:>DISPLAY_LENGTH$}"), now, &mut out);
                }
            }
            ControlEvent::Xy { x, y } => match self.bank().xy.clone() {
                Some(XyAction::Xy { channel, cc_x, cc_y }) => {
                    let last = self.last_xy;
                    if last.map(|(lx, _)| lx) != Some(x) {
                        out.daw.push(control_change(channel, cc_x, x));
                    }
                    if last.map(|(_, ly)| ly) != Some(y) {
                        out.daw.push(control_change(channel, cc_y, y));
                    }
                    self.last_xy = Some((x, y));
                }
                Some(XyAction::Osc { address }) => {
                    out.osc.push(OscMessage::new(address, vec![OscArg::Int(x.into()), OscArg::Int(y.into())]));
                }
                None => {}
            },
            ControlEvent::Encoder { delta } => {
                if let Some(action) = self.bank().encoder.clone() {
                    let encoding = action.encoding();
                    // Position shown on the display. In absolute mode it is also
                    // the value sent, so it stays a MIDI value; in relative mode
                    // the DAW only gets steps and the counter is free within
                    // what 3 characters can show.
                    let range = match encoding {
                        Encoding::Relative => RELATIVE_COUNTER,
                        Encoding::Absolute => 0..=127,
                    };
                    let position = self.encoder.entry(self.bank).or_insert(0);
                    *position = (*position + delta as i16).clamp(*range.start(), *range.end());
                    let position = *position;
                    out.encoder_position = Some(position);
                    match action {
                        EncoderAction::Cc { channel, cc, .. } => {
                            let value = match encoding {
                                // Signed 7-bit relative value: 1 = +1, 127 = -1.
                                Encoding::Relative => (delta.clamp(-63, 63) as i16).rem_euclid(128) as u8,
                                Encoding::Absolute => position as u8,
                            };
                            out.daw.push(control_change(channel, cc, value));
                        }
                        EncoderAction::Osc { address, .. } => {
                            let value = match encoding {
                                Encoding::Relative => delta.into(),
                                Encoding::Absolute => position.into(),
                            };
                            out.osc.push(OscMessage::new(address, vec![OscArg::Int(value)]));
                        }
                    }
                    if display.encoder {
                        self.show_value(format!("{position:>DISPLAY_LENGTH$}"), now, &mut out);
                    }
                }
            }
        }
        out
    }

    /// Message sent back by the DAW: updates the LEDs in `follow` mode.
    pub fn feedback(&mut self, message: &[u8]) -> Vec<Vec<u8>> {
        let (Some(&status), Some(&number)) = (message.first(), message.get(1)) else { return vec![] };
        let channel = (status & 0x0F) + 1;
        let value = message.get(2).copied().unwrap_or(0);
        let (key, on) = match status & 0xF0 {
            0x90 => (FollowKey::Note { channel, note: number }, value > 0),
            0x80 => (FollowKey::Note { channel, note: number }, false),
            0xB0 => (FollowKey::Cc { channel, cc: number }, value >= 64),
            _ => return vec![],
        };
        if self.follow.insert(key, on) == Some(on) {
            return vec![];
        }
        Control::all()
            .filter(|&control| {
                self.action(self.bank, control)
                    .is_some_and(|a| led_mode(a) == Led::Follow && follow_key(a) == Some(key))
            })
            .map(|control| padkontrol::led(control, on))
            .collect()
    }

    fn bank(&self) -> &crate::config::Bank {
        &self.profile.banks[self.bank]
    }

    fn action(&self, bank: usize, control: Control) -> Option<&ButtonAction> {
        let bank = self.profile.banks.get(bank)?;
        match control {
            Control::Pad(pad) => bank.pads.get(&pad),
            Control::Button(id) => bank.buttons.get(&id),
            Control::XyTouch => bank.xy_touch.as_ref(),
        }
    }

    fn press(&mut self, control: Control, pad_velocity: Option<u8>, out: &mut Output) {
        let bank = self.bank;
        let Some(action) = self.action(bank, control).cloned() else { return };
        self.held.insert(control, bank);

        match action {
            ButtonAction::Note { channel, note, velocity, mode, .. } => {
                let velocity = match velocity {
                    Some(Velocity::Fixed(v)) => v,
                    _ => pad_velocity.unwrap_or(127),
                }
                .max(1);
                if mode == Mode::Momentary || self.flip(bank, control) {
                    out.daw.push(note_on(channel, note, velocity));
                } else {
                    out.daw.push(note_off(channel, note));
                }
            }
            ButtonAction::Cc { channel, cc, on_value, off_value, mode, .. } => {
                let on = mode == Mode::Momentary || self.flip(bank, control);
                out.daw.push(control_change(channel, cc, if on { on_value } else { off_value }));
            }
            ButtonAction::ProgramChange { channel, program, .. } => {
                out.daw.push(vec![0xC0 | (channel - 1), program]);
            }
            ButtonAction::SwitchBank { bank: target, .. } => self.switch_bank(target, out),
            ButtonAction::Osc { address, args, release_args, mode, .. } => {
                let velocity = pad_velocity.unwrap_or(127);
                if mode == Mode::Momentary || self.flip(bank, control) {
                    out.osc.push(OscMessage::new(address, osc::resolve(&args, velocity)));
                } else if let Some(release_args) = release_args {
                    out.osc.push(OscMessage::new(address, osc::resolve(&release_args, velocity)));
                }
            }
        }
        self.update_led(control, out);
    }

    fn release(&mut self, control: Control, out: &mut Output) {
        let Some(pressed_in) = self.held.remove(&control) else { return };
        let Some(action) = self.action(pressed_in, control).cloned() else { return };

        match action {
            ButtonAction::Note { channel, note, mode: Mode::Momentary, .. } => out.daw.push(note_off(channel, note)),
            ButtonAction::Cc { channel, cc, off_value, mode: Mode::Momentary, .. } => {
                out.daw.push(control_change(channel, cc, off_value))
            }
            ButtonAction::SwitchBank { bank: target, momentary: true } if self.bank == target => {
                self.switch_bank(pressed_in, out)
            }
            ButtonAction::Osc { address, release_args: Some(release_args), mode: Mode::Momentary, .. } => {
                out.osc.push(OscMessage::new(address, osc::resolve(&release_args, 0)));
            }
            _ => {}
        }
        self.update_led(control, out);
    }

    /// Flips a control's toggle state and returns the new state.
    fn flip(&mut self, bank: usize, control: Control) -> bool {
        let key = (bank, control);
        if self.toggles.remove(&key) {
            false
        } else {
            self.toggles.insert(key);
            true
        }
    }

    fn switch_bank(&mut self, bank: usize, out: &mut Output) {
        if bank >= self.profile.banks.len() || bank == self.bank {
            return;
        }
        self.bank = bank;
        self.last_xy = None;
        self.led_override.clear();
        out.bank_changed = true;
        out.pad.extend(self.refresh());
    }

    fn update_led(&self, control: Control, out: &mut Output) {
        if control.has_led() {
            out.pad.push(padkontrol::led(control, self.led_state(control)));
        }
    }

    fn led_state(&self, control: Control) -> bool {
        // Set over OSC: wins until the next bank change.
        if let Some(&on) = self.led_override.get(&control) {
            return on;
        }
        let Some(action) = self.action(self.bank, control) else { return false };
        match led_mode(action) {
            Led::None => false,
            Led::Press => self.held.contains_key(&control),
            Led::Toggle => self.toggles.contains(&(self.bank, control)),
            Led::Follow => match action {
                ButtonAction::Osc { address, .. } => self.osc_follow.get(address).copied().unwrap_or(false),
                _ => follow_key(action).and_then(|k| self.follow.get(&k)).copied().unwrap_or(false),
            },
        }
    }

    /// OSC message received from the software in front. Under `prefix`:
    /// `/led/pad/3 1|0` and `/led/button/hold 1|0` set an LED, `/display
    /// "ABC"` writes on the display, `/bank 2` switches bank. Any other
    /// address drives the `follow` LEDs of the `osc` actions sending to it.
    pub fn osc_feedback(&mut self, message: &OscMessage, prefix: &str) -> Output {
        let mut out = Output::default();
        let first = message.args.first();
        let local = message.address.strip_prefix(prefix).filter(|rest| rest.starts_with('/'));

        match local.map(|rest| rest.split('/').skip(1).collect::<Vec<_>>()).as_deref() {
            Some(["bank"]) => {
                if let Some(bank) = first.and_then(OscArg::as_int).and_then(|b| usize::try_from(b).ok()) {
                    self.switch_bank(bank, &mut out);
                }
            }
            Some(["display"]) => {
                if let Some(OscArg::Str(text)) = first {
                    // Stays until a value or the bank replaces it.
                    self.display_until = None;
                    self.shown = text.clone();
                    out.pad.push(padkontrol::display(text));
                }
            }
            Some(["led", "pad", pad]) => {
                if let Ok(pad @ 1..=16) = pad.parse::<u8>() {
                    self.set_led_override(Control::Pad(pad), first.is_some_and(OscArg::is_on), &mut out);
                }
            }
            Some(["led", "button", name]) => {
                if let Ok(button) = serde_json::from_value(serde_json::Value::String(name.to_string())) {
                    self.set_led_override(Control::Button(button), first.is_some_and(OscArg::is_on), &mut out);
                }
            }
            _ => {
                let on = first.is_some_and(OscArg::is_on);
                if self.osc_follow.insert(message.address.clone(), on) != Some(on) {
                    let following: Vec<Control> = Control::all()
                        .filter(|&control| {
                            self.action(self.bank, control).is_some_and(|a| {
                                led_mode(a) == Led::Follow
                                    && matches!(a, ButtonAction::Osc { address, .. } if *address == message.address)
                            })
                        })
                        .collect();
                    for control in following {
                        self.update_led(control, &mut out);
                    }
                }
            }
        }
        out
    }

    fn set_led_override(&mut self, control: Control, on: bool, out: &mut Output) {
        self.led_override.insert(control, on);
        self.update_led(control, out);
    }
}

/// LED behavior, with the schema's default values.
fn led_mode(action: &ButtonAction) -> Led {
    match action {
        ButtonAction::Note { led, mode, .. } | ButtonAction::Cc { led, mode, .. } | ButtonAction::Osc { led, mode, .. } => {
            led.unwrap_or(match mode {
                Mode::Momentary => Led::Press,
                Mode::Toggle => Led::Toggle,
            })
        }
        ButtonAction::ProgramChange { led, .. } => led.unwrap_or(Led::Press),
        ButtonAction::SwitchBank { .. } => Led::Press,
    }
}

fn follow_key(action: &ButtonAction) -> Option<FollowKey> {
    match *action {
        ButtonAction::Note { channel, note, .. } => Some(FollowKey::Note { channel, note }),
        ButtonAction::Cc { channel, cc, .. } => Some(FollowKey::Cc { channel, cc }),
        _ => None,
    }
}

/// Knob position (0 to 127) scaled to an OSC range, as a float or rounded.
fn scale_osc(value: u8, min: f32, max: f32, float: bool) -> OscArg {
    let scaled = min + (max - min) * f32::from(value) / 127.0;
    if float { OscArg::Float(scaled) } else { OscArg::Int(scaled.round() as i32) }
}

/// Note name for the display, with middle C (60) as C3 like most DAWs.
fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[note as usize % 12], note as i16 / 12 - 2)
}

fn note_on(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    vec![0x90 | (channel - 1), note, velocity]
}

fn note_off(channel: u8, note: u8) -> Vec<u8> {
    vec![0x80 | (channel - 1), note, 0]
}

fn control_change(channel: u8, cc: u8, value: u8) -> Vec<u8> {
    vec![0xB0 | (channel - 1), cc, value]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{parse_profile, ButtonId, DEFAULT_PROFILE_JSON};

    fn engine(banks: &str) -> Engine {
        Engine::new(parse_profile(&format!(r#"{{ "version": 1, "name": "Test", "banks": [{banks}] }}"#)).unwrap())
    }

    fn pad(pad: u8, pressed: bool, velocity: u8) -> ControlEvent {
        ControlEvent::Pad { pad, pressed, velocity }
    }

    fn button(button: ButtonId, pressed: bool) -> ControlEvent {
        ControlEvent::Button { button, pressed }
    }

    fn led_on(out: &Output, control: Control) -> Option<bool> {
        out.pad.iter().rev().find_map(|m| {
            if *m == padkontrol::led(control, true) {
                Some(true)
            } else if *m == padkontrol::led(control, false) {
                Some(false)
            } else {
                None
            }
        })
    }

    #[test]
    fn pad_sends_note_with_pad_velocity_and_lights_while_pressed() {
        let mut e = engine(r#"{ "name": "A", "pads": { "1": { "type": "note", "channel": 10, "note": 36 } } }"#);
        let out = e.handle(&pad(1, true, 90));
        assert_eq!(out.daw, vec![vec![0x99, 36, 90]]);
        assert_eq!(led_on(&out, Control::Pad(1)), Some(true));

        let out = e.handle(&pad(1, false, 64));
        assert_eq!(out.daw, vec![vec![0x89, 36, 0]]);
        assert_eq!(led_on(&out, Control::Pad(1)), Some(false));
    }

    #[test]
    fn fixed_velocity_and_buttons_default_to_127() {
        let mut e = engine(
            r#"{ "name": "A",
                 "pads": { "2": { "type": "note", "channel": 1, "note": 40, "velocity": 100 } },
                 "buttons": { "x": { "type": "note", "channel": 1, "note": 5 } } }"#,
        );
        assert_eq!(e.handle(&pad(2, true, 10)).daw, vec![vec![0x90, 40, 100]]);
        assert_eq!(e.handle(&button(ButtonId::X, true)).daw, vec![vec![0x90, 5, 127]]);
    }

    #[test]
    fn toggle_alternates_and_keeps_its_led() {
        let mut e = engine(
            r#"{ "name": "A", "buttons": { "hold": { "type": "cc", "channel": 1, "cc": 64, "mode": "toggle" } } }"#,
        );
        let hold = Control::Button(ButtonId::Hold);
        assert_eq!(e.handle(&button(ButtonId::Hold, true)).daw, vec![vec![0xB0, 64, 127]]);
        let out = e.handle(&button(ButtonId::Hold, false));
        assert!(out.daw.is_empty());
        assert_eq!(led_on(&out, hold), Some(true));

        assert_eq!(e.handle(&button(ButtonId::Hold, true)).daw, vec![vec![0xB0, 64, 0]]);
        assert_eq!(led_on(&e.handle(&button(ButtonId::Hold, false)), hold), Some(false));
    }

    #[test]
    fn switch_bank_refreshes_leds_and_display() {
        let mut e = engine(
            r#"{ "name": "Un", "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "Deux", "pads": { "1": { "type": "note", "channel": 2, "note": 60 } } }"#,
        );
        let out = e.handle(&button(ButtonId::Setting, true));
        assert!(out.bank_changed);
        assert!(out.daw.is_empty());
        assert!(out.pad.contains(&padkontrol::display("Deux")));
        assert_eq!(e.state().bank, 1);
        assert_eq!(e.handle(&pad(1, true, 50)).daw, vec![vec![0x91, 60, 50]]);
    }

    #[test]
    fn momentary_switch_returns_on_release() {
        let mut e = engine(
            r#"{ "name": "Un", "buttons": { "pedal": { "type": "switchBank", "bank": 1, "momentary": true } } },
               { "name": "Deux" }"#,
        );
        assert!(e.handle(&button(ButtonId::Pedal, true)).bank_changed);
        assert_eq!(e.state().bank, 1);
        assert!(e.handle(&button(ButtonId::Pedal, false)).bank_changed);
        assert_eq!(e.state().bank, 0);
    }

    #[test]
    fn note_held_across_a_bank_switch_is_released_with_its_own_note() {
        let mut e = engine(
            r#"{ "name": "Un",
                 "pads": { "1": { "type": "note", "channel": 1, "note": 36 } },
                 "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "Deux", "pads": { "1": { "type": "note", "channel": 2, "note": 60 } } }"#,
        );
        e.handle(&pad(1, true, 80));
        e.handle(&button(ButtonId::Setting, true));
        assert_eq!(e.handle(&pad(1, false, 64)).daw, vec![vec![0x80, 36, 0]]);
    }

    #[test]
    fn unmapped_controls_do_nothing() {
        let mut e = engine(r#"{ "name": "A" }"#);
        assert_eq!(e.handle(&pad(3, true, 80)), Output::default());
        assert_eq!(e.handle(&pad(3, false, 64)), Output::default());
        assert_eq!(e.handle(&ControlEvent::Knob { knob: 1, value: 10 }), Output::default());
    }

    #[test]
    fn knob_is_scaled_to_its_range() {
        let mut e = engine(
            r#"{ "name": "A", "knobs": { "2": { "type": "cc", "channel": 3, "cc": 10, "min": 32, "max": 96 } } }"#,
        );
        assert_eq!(e.handle(&ControlEvent::Knob { knob: 2, value: 0 }).daw, vec![vec![0xB2, 10, 32]]);
        assert_eq!(e.handle(&ControlEvent::Knob { knob: 2, value: 127 }).daw, vec![vec![0xB2, 10, 96]]);
    }

    #[test]
    fn xy_only_sends_changed_axes() {
        let mut e = engine(r#"{ "name": "A", "xy": { "type": "xy", "channel": 1, "ccX": 3, "ccY": 4 } }"#);
        assert_eq!(e.handle(&ControlEvent::Xy { x: 10, y: 20 }).daw, vec![vec![0xB0, 3, 10], vec![0xB0, 4, 20]]);
        assert_eq!(e.handle(&ControlEvent::Xy { x: 11, y: 20 }).daw, vec![vec![0xB0, 3, 11]]);
    }

    #[test]
    fn encoder_relative_and_absolute() {
        let mut e = engine(
            r#"{ "name": "R", "encoder": { "type": "cc", "channel": 1, "cc": 10 },
                 "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "A", "encoder": { "type": "cc", "channel": 1, "cc": 11, "encoding": "absolute" } }"#,
        );
        assert_eq!(e.handle(&ControlEvent::Encoder { delta: 1 }).daw, vec![vec![0xB0, 10, 1]]);
        assert_eq!(e.handle(&ControlEvent::Encoder { delta: -1 }).daw, vec![vec![0xB0, 10, 127]]);

        e.handle(&button(ButtonId::Setting, true));
        assert_eq!(e.handle(&ControlEvent::Encoder { delta: -1 }).daw, vec![vec![0xB0, 11, 0]]);
        assert_eq!(e.handle(&ControlEvent::Encoder { delta: 3 }).daw, vec![vec![0xB0, 11, 3]]);
    }

    #[test]
    fn follow_leds_track_daw_feedback() {
        let mut e = engine(
            r#"{ "name": "A", "pads": { "5": { "type": "note", "channel": 2, "note": 64, "led": "follow" } } }"#,
        );
        let control = Control::Pad(5);
        assert_eq!(e.feedback(&[0x91, 64, 127]), vec![padkontrol::led(control, true)]);
        assert!(e.feedback(&[0x91, 64, 100]).is_empty(), "état inchangé, rien à envoyer");
        assert_eq!(e.feedback(&[0x91, 64, 0]), vec![padkontrol::led(control, false)]);
        assert!(e.feedback(&[0x90, 64, 127]).is_empty(), "autre canal");

        // Pressing does not change a DAW-driven LED.
        e.feedback(&[0x91, 64, 127]);
        assert_eq!(led_on(&e.handle(&pad(5, false, 0)), control), None);
        assert_eq!(led_on(&e.handle(&pad(5, true, 90)), control), Some(true));
        assert_eq!(led_on(&e.handle(&pad(5, false, 0)), control), Some(true));
    }

    #[test]
    fn reload_keeps_the_bank_and_feedback() {
        let mut e = engine(
            r#"{ "name": "Un", "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "Deux", "pads": { "1": { "type": "note", "channel": 2, "note": 60, "led": "follow" } } }"#,
        )
        .with_file("live.json");
        e.handle(&button(ButtonId::Setting, true));
        e.feedback(&[0x91, 60, 127]);

        let mut edited = e.profile.clone();
        edited.banks[1].name = "Clips".into();
        e.reload(edited);
        assert_eq!(e.state().bank, 1);
        assert_eq!(e.state().file, "live.json");
        assert_eq!(e.state().banks[1], "Clips");
        assert!(e.refresh().contains(&padkontrol::led(Control::Pad(1), true)));

        let mut shorter = e.profile.clone();
        shorter.banks.truncate(1);
        e.reload(shorter);
        assert_eq!(e.state().bank, 0);
    }

    #[test]
    fn default_profile_runs() {
        let mut e = Engine::new(parse_profile(DEFAULT_PROFILE_JSON).unwrap());
        assert_eq!(e.refresh().len(), 36);
        assert_eq!(e.handle(&pad(1, true, 100)).daw, vec![vec![0x99, 36, 100]]);
        assert!(e.handle(&button(ButtonId::Setting, true)).bank_changed);
        assert_eq!(e.state().banks, vec!["Batterie", "Clips"]);
    }

    fn engine_with(profile: &str) -> Engine {
        Engine::new(parse_profile(profile).unwrap())
    }

    #[test]
    fn display_shows_the_bank_text_or_name() {
        let mut e = engine(r#"{ "name": "Ma batterie", "display": "DRM" }, { "name": "Long clips" }"#);
        assert_eq!(e.refresh().last(), Some(&padkontrol::display("DRM")));
        assert!(e.select_bank(1).pad.contains(&padkontrol::display("LON")));
    }

    #[test]
    fn knob_value_shows_then_the_bank_comes_back() {
        let mut e = engine(r#"{ "name": "Bat", "knobs": { "1": { "type": "cc", "channel": 1, "cc": 7 } } }"#);
        e.refresh();
        let t0 = Instant::now();

        let out = e.handle_at(&ControlEvent::Knob { knob: 1, value: 64 }, t0);
        assert_eq!(out.pad, vec![padkontrol::display(" 64")]);
        // Same value again: nothing new to send.
        assert!(e.handle_at(&ControlEvent::Knob { knob: 1, value: 64 }, t0).pad.is_empty());

        assert!(e.tick(t0 + Duration::from_millis(500)).is_empty());
        assert_eq!(e.tick(t0 + Duration::from_millis(1000)), vec![padkontrol::display("BAT")]);
        assert!(e.tick(t0 + Duration::from_millis(1500)).is_empty());
    }

    #[test]
    fn a_new_value_extends_the_display_time() {
        let mut e = engine(r#"{ "name": "Bat", "knobs": { "1": { "type": "cc", "channel": 1, "cc": 7 } } }"#);
        e.refresh();
        let t0 = Instant::now();
        e.handle_at(&ControlEvent::Knob { knob: 1, value: 10 }, t0);
        e.handle_at(&ControlEvent::Knob { knob: 1, value: 20 }, t0 + Duration::from_millis(800));
        assert!(e.tick(t0 + Duration::from_millis(1200)).is_empty());
        assert_eq!(e.tick(t0 + Duration::from_millis(1800)), vec![padkontrol::display("BAT")]);
    }

    #[test]
    fn encoder_shows_a_counter_in_both_modes() {
        let mut e = engine(
            r#"{ "name": "R", "encoder": { "type": "cc", "channel": 1, "cc": 10 },
                 "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "A", "encoder": { "type": "cc", "channel": 1, "cc": 11, "encoding": "absolute" } }"#,
        );
        let t0 = Instant::now();
        let turn = |e: &mut Engine, delta| e.handle_at(&ControlEvent::Encoder { delta }, t0);

        // Relative: the DAW still gets steps, the display counts.
        let out = turn(&mut e, 1);
        assert_eq!(out.daw, vec![vec![0xB0, 10, 1]]);
        assert_eq!(out.pad, vec![padkontrol::display("  1")]);
        assert_eq!(turn(&mut e, 1).pad, vec![padkontrol::display("  2")]);
        let out = turn(&mut e, -1);
        assert_eq!(out.daw, vec![vec![0xB0, 10, 127]]);
        assert_eq!(out.pad, vec![padkontrol::display("  1")]);
        // The relative counter goes below zero, down to what the display shows.
        turn(&mut e, -1);
        let out = turn(&mut e, -1);
        assert_eq!(out.daw, vec![vec![0xB0, 10, 127]]);
        assert_eq!(out.pad, vec![padkontrol::display(" -1")]);
        assert_eq!(out.encoder_position, Some(-1), "reported for the live view");
        turn(&mut e, -63);
        turn(&mut e, -63);
        assert_eq!(turn(&mut e, -1).encoder_position, Some(-99));
        assert_eq!(e.state().encoder, -99);

        // Absolute: each bank keeps its own counter, a MIDI value from 0 to 127.
        e.handle(&button(ButtonId::Setting, true));
        assert_eq!(e.state().encoder, 0);
        assert_eq!(turn(&mut e, -1).encoder_position, Some(0), "never negative in absolute mode");
        turn(&mut e, 3);
        let out = turn(&mut e, 2);
        assert_eq!(out.daw, vec![vec![0xB0, 11, 5]]);
        assert_eq!(out.pad, vec![padkontrol::display("  5")]);
        assert_eq!(out.encoder_position, Some(5));
        assert_eq!(e.state().encoder, 5);
    }

    #[test]
    fn relative_counter_stops_at_999() {
        let mut e = engine(r#"{ "name": "R", "encoder": { "type": "cc", "channel": 1, "cc": 10 } }"#);
        for _ in 0..20 {
            e.handle(&ControlEvent::Encoder { delta: 63 });
        }
        let out = e.handle(&ControlEvent::Encoder { delta: 1 });
        assert_eq!(out.encoder_position, Some(999));
        assert_eq!(out.daw, vec![vec![0xB0, 10, 1]], "the DAW still gets the step");
    }

    #[test]
    fn pads_can_show_their_note_or_velocity() {
        let pads = r#""pads": { "1": { "type": "note", "channel": 10, "note": 37 } }"#;
        let t0 = Instant::now();

        let mut e = engine(&format!(r#"{{ "name": "A", {pads} }}"#));
        assert!(e.handle_at(&pad(1, true, 90), t0).pad.iter().all(|m| m[5] != 0x22), "rien par défaut");

        let mut e = engine_with(&format!(
            r#"{{ "version": 1, "name": "T", "display": {{ "pads": "note" }}, "banks": [{{ "name": "A", {pads} }}] }}"#
        ));
        assert!(e.handle_at(&pad(1, true, 90), t0).pad.contains(&padkontrol::display("C#1")));

        let mut e = engine_with(&format!(
            r#"{{ "version": 1, "name": "T", "display": {{ "pads": "velocity" }}, "banks": [{{ "name": "A", {pads} }}] }}"#
        ));
        assert!(e.handle_at(&pad(1, true, 90), t0).pad.contains(&padkontrol::display(" 90")));
    }

    #[test]
    fn display_can_be_turned_off_for_knobs() {
        let mut e = engine_with(
            r#"{ "version": 1, "name": "T", "display": { "knobs": false },
                 "banks": [{ "name": "A", "knobs": { "1": { "type": "cc", "channel": 1, "cc": 7 } } }] }"#,
        );
        assert!(e.handle(&ControlEvent::Knob { knob: 1, value: 64 }).pad.is_empty());
    }

    fn osc_text(out: &Output) -> Vec<String> {
        out.osc.iter().map(OscMessage::describe).collect()
    }

    #[test]
    fn osc_button_sends_args_on_press_and_release() {
        let mut e = engine(
            r#"{ "name": "A", "pads": { "1": { "type": "osc", "address": "/live/clip", "args": [0, "$velocity"],
                                               "releaseArgs": [0, 0] } },
                 "buttons": { "x": { "type": "osc", "address": "/go" } } }"#,
        );
        let out = e.handle(&pad(1, true, 90));
        assert_eq!(osc_text(&out), vec!["/live/clip 0 90"]);
        assert!(out.daw.is_empty());
        assert_eq!(led_on(&out, Control::Pad(1)), Some(true));
        assert_eq!(osc_text(&e.handle(&pad(1, false, 64))), vec!["/live/clip 0 0"]);

        // No release args: nothing on release; a button stands in for velocity 127.
        assert_eq!(osc_text(&e.handle(&button(ButtonId::X, true))), vec!["/go"]);
        assert!(e.handle(&button(ButtonId::X, false)).osc.is_empty());
    }

    #[test]
    fn osc_toggle_alternates_between_args_and_release_args() {
        let mut e = engine(
            r#"{ "name": "A", "buttons": { "hold": { "type": "osc", "address": "/mute", "args": [true],
                                                    "releaseArgs": [false], "mode": "toggle" } } }"#,
        );
        assert_eq!(osc_text(&e.handle(&button(ButtonId::Hold, true))), vec!["/mute true"]);
        assert!(e.handle(&button(ButtonId::Hold, false)).osc.is_empty());
        assert_eq!(osc_text(&e.handle(&button(ButtonId::Hold, true))), vec!["/mute false"]);
    }

    #[test]
    fn osc_knob_xy_and_encoder() {
        let mut e = engine(
            r#"{ "name": "A",
                 "knobs": { "1": { "type": "osc", "address": "/vol" },
                            "2": { "type": "osc", "address": "/pan", "min": -1, "max": 1, "float": true } },
                 "xy": { "type": "osc", "address": "/xy" },
                 "encoder": { "type": "osc", "address": "/scroll" },
                 "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "B", "encoder": { "type": "osc", "address": "/pos", "encoding": "absolute" } }"#,
        );
        let out = e.handle(&ControlEvent::Knob { knob: 1, value: 64 });
        assert_eq!(osc_text(&out), vec!["/vol 64"]);
        assert!(out.pad.contains(&padkontrol::display(" 64")), "the display still shows the knob");
        assert_eq!(osc_text(&e.handle(&ControlEvent::Knob { knob: 2, value: 127 })), vec!["/pan 1.0"]);
        assert_eq!(osc_text(&e.handle(&ControlEvent::Knob { knob: 2, value: 0 })), vec!["/pan -1.0"]);
        assert_eq!(osc_text(&e.handle(&ControlEvent::Xy { x: 10, y: 20 })), vec!["/xy 10 20"]);

        let out = e.handle(&ControlEvent::Encoder { delta: -1 });
        assert_eq!(osc_text(&out), vec!["/scroll -1"]);
        assert_eq!(out.encoder_position, Some(-1));

        e.handle(&button(ButtonId::Setting, true));
        e.handle(&ControlEvent::Encoder { delta: 5 });
        assert_eq!(osc_text(&e.handle(&ControlEvent::Encoder { delta: 2 })), vec!["/pos 7"]);
    }

    #[test]
    fn xy_touch_acts_like_a_button_without_led() {
        let mut e = engine(
            r#"{ "name": "A", "xyTouch": { "type": "cc", "channel": 1, "cc": 80 },
                 "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "B", "xyTouch": { "type": "osc", "address": "/fx", "args": [1], "releaseArgs": [0], "mode": "toggle" } }"#,
        );
        let touch = |e: &mut Engine, pressed| e.handle(&ControlEvent::XyTouch { pressed });

        let out = touch(&mut e, true);
        assert_eq!(out.daw, vec![vec![0xB0, 80, 127]]);
        assert!(out.pad.is_empty(), "no LED message for the XY pad");
        assert_eq!(touch(&mut e, false).daw, vec![vec![0xB0, 80, 0]]);

        e.handle(&button(ButtonId::Setting, true));
        assert_eq!(osc_text(&touch(&mut e, true)), vec!["/fx 1"]);
        assert!(touch(&mut e, false).osc.is_empty());
        assert_eq!(osc_text(&touch(&mut e, true)), vec!["/fx 0"]);
    }

    fn osc_in(address: &str, args: Vec<OscArg>) -> OscMessage {
        OscMessage::new(address, args)
    }

    #[test]
    fn osc_feedback_drives_follow_leds_of_osc_actions() {
        let mut e = engine(
            r#"{ "name": "A", "pads": {
                "1": { "type": "osc", "address": "/live/clip/1", "led": "follow" },
                "2": { "type": "osc", "address": "/live/clip/2" } } }"#,
        );
        let out = e.osc_feedback(&osc_in("/live/clip/1", vec![OscArg::Int(1)]), "/tk");
        assert_eq!(out.pad, vec![padkontrol::led(Control::Pad(1), true)]);
        assert!(e.osc_feedback(&osc_in("/live/clip/1", vec![OscArg::Float(0.7)]), "/tk").pad.is_empty(), "no change");
        assert_eq!(
            e.osc_feedback(&osc_in("/live/clip/1", vec![OscArg::Int(0)]), "/tk").pad,
            vec![padkontrol::led(Control::Pad(1), false)]
        );
        // Pad 2 does not follow (press LED): nothing to send.
        assert!(e.osc_feedback(&osc_in("/live/clip/2", vec![OscArg::Int(1)]), "/tk").pad.is_empty());
    }

    #[test]
    fn osc_feedback_sets_leds_display_and_bank() {
        let mut e = engine(
            r#"{ "name": "Un", "pads": { "3": { "type": "note", "channel": 1, "note": 38 } } }, { "name": "Deux" }"#,
        );
        e.refresh();

        let out = e.osc_feedback(&osc_in("/tk/led/pad/3", vec![OscArg::Int(1)]), "/tk");
        assert_eq!(out.pad, vec![padkontrol::led(Control::Pad(3), true)]);
        // The override wins over the press LED until the bank changes.
        assert_eq!(led_on(&e.handle(&pad(3, false, 0)), Control::Pad(3)), None);
        assert_eq!(led_on(&e.handle(&pad(3, true, 90)), Control::Pad(3)), Some(true));
        assert_eq!(led_on(&e.handle(&pad(3, false, 0)), Control::Pad(3)), Some(true));

        let out = e.osc_feedback(&osc_in("/tk/led/button/hold", vec![OscArg::Bool(true)]), "/tk");
        assert_eq!(out.pad, vec![padkontrol::led(Control::Button(ButtonId::Hold), true)]);
        assert!(e.osc_feedback(&osc_in("/tk/led/button/shift", vec![OscArg::Int(1)]), "/tk").pad.is_empty());
        assert!(e.osc_feedback(&osc_in("/tk/led/pad/17", vec![OscArg::Int(1)]), "/tk").pad.is_empty());

        let out = e.osc_feedback(&osc_in("/tk/display", vec![OscArg::Str("ABC".into())]), "/tk");
        assert_eq!(out.pad, vec![padkontrol::display("ABC")]);
        assert!(e.tick(Instant::now() + Duration::from_secs(10)).is_empty(), "the text stays");

        let out = e.osc_feedback(&osc_in("/tk/bank", vec![OscArg::Int(1)]), "/tk");
        assert!(out.bank_changed);
        assert!(out.pad.contains(&padkontrol::led(Control::Pad(3), false)), "overrides cleared by the bank change");
        assert!(out.pad.contains(&padkontrol::display("DEU")));
        assert!(!e.osc_feedback(&osc_in("/tk/bank", vec![OscArg::Int(9)]), "/tk").bank_changed);
    }

    #[test]
    fn osc_follow_survives_a_profile_reload() {
        let mut e = engine(r#"{ "name": "A", "pads": { "1": { "type": "osc", "address": "/c", "led": "follow" } } }"#);
        e.osc_feedback(&osc_in("/c", vec![OscArg::Int(1)]), "/tk");
        let profile = e.profile.clone();
        e.reload(profile);
        assert!(e.refresh().contains(&padkontrol::led(Control::Pad(1), true)));
    }

    #[test]
    fn note_names_use_c3_as_middle_c() {
        assert_eq!(note_name(60), "C3");
        assert_eq!(note_name(36), "C1");
        assert_eq!(note_name(37), "C#1");
        assert_eq!(note_name(0), "C-2");
        assert_eq!(note_name(127), "G8");
    }
}
