//! Mapping engine: applies the active profile to padKONTROL events.
//! It produces MIDI messages for the DAW and LED / display messages
//! for the padKONTROL. No I/O here: wiring to the MIDI ports is done
//! in `midi.rs`.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{
    BPM_RANGE, ButtonAction, DISPLAY_LENGTH, EncoderAction, Encoding, FLAM_RANGE_MS, KnobAction, Led, Mode, PadDisplay,
    Profile, ROLL_DIVISIONS, Velocity, XyAction,
};
use crate::keys::{KeyDirection, KeyEvent};
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
    /// Shortcuts from `key` actions, to type.
    pub keys: Vec<KeyEvent>,
    /// Profile file a `switchProfile` action asks for.
    pub switch_profile: Option<String>,
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
/// How close a knob must come to its value to be picked up, for knobs that
/// jump over values when turned fast.
const PICKUP_TOLERANCE: u8 = 2;
/// Most shortcuts typed for one encoder message, however fast it turns.
const MAX_KEY_STEPS: u8 = 8;

/// Velocity of a flam's grace note, relative to the main note.
const FLAM_GRACE: f32 = 0.6;
/// Released before its flam main note played: the note off follows it after this.
const RELEASE_AFTER_FLAM: Duration = Duration::from_millis(20);
/// MIDI clock pulses per quarter note.
const CLOCK_PPQN: f32 = 24.0;
/// The DAW's clock counts as running if a pulse arrived within this time.
const CLOCK_TIMEOUT: Duration = Duration::from_millis(500);
/// Shortest time between two clock pulses taken into account (1 ms is
/// already 2500 BPM).
const MIN_CLOCK_GAP: Duration = Duration::from_millis(1);

/// A held pad repeating its note while roll is on.
#[derive(Debug, Clone, Copy)]
struct Roll {
    channel: u8,
    note: u8,
    velocity: u8,
    next: Instant,
}

/// Tempo of the MIDI clock received from the DAW.
#[derive(Debug, Clone, Copy, Default)]
struct Clock {
    last: Option<Instant>,
    /// Smoothed time between two pulses.
    pulse: Option<Duration>,
}

impl Clock {
    fn pulse(&mut self, now: Instant) {
        if let Some(last) = self.last {
            let gap = now.saturating_duration_since(last);
            // Ignore gaps after a stop or a stall, and pulses delivered in a
            // burst, which would make the tempo shoot up.
            if (MIN_CLOCK_GAP..CLOCK_TIMEOUT).contains(&gap) {
                self.pulse = Some(match self.pulse {
                    Some(pulse) => pulse.mul_f32(0.9) + gap.mul_f32(0.1),
                    None => gap,
                });
            }
        }
        self.last = Some(now);
    }

    fn bpm(&self, now: Instant) -> Option<f32> {
        let running = self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < CLOCK_TIMEOUT);
        let pulse = self.pulse.filter(|_| running)?;
        Some(60.0 / (pulse.as_secs_f32() * CLOCK_PPQN))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FollowKey {
    Note { channel: u8, note: u8 },
    Cc { channel: u8, cc: u8 },
}

#[derive(Clone)]
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
    /// Modes toggled by the `roll` and `flam` actions.
    roll_on: bool,
    flam_on: bool,
    /// Roll speed and flam gap set with the XY pad, over the profile's values.
    roll_override: Option<u8>,
    flam_override: Option<u16>,
    /// Held pads repeating their note.
    rolls: HashMap<Control, Roll>,
    /// MIDI messages to send later (flam notes), in time order.
    scheduled: Vec<(Instant, Vec<u8>)>,
    clock: Clock,
    /// Knob pickup: last value sent per bank and knob, where each knob is,
    /// and the knobs waiting to reach their value after a bank change.
    knob_sent: HashMap<(usize, u8), u8>,
    knob_physical: HashMap<u8, u8>,
    knob_waiting: HashSet<(usize, u8)>,
    /// Last encoder move, for its acceleration.
    last_encoder: Option<Instant>,
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
            roll_on: false,
            flam_on: false,
            roll_override: None,
            flam_override: None,
            rolls: HashMap::new(),
            scheduled: Vec::new(),
            clock: Clock::default(),
            knob_sent: HashMap::new(),
            knob_physical: HashMap::new(),
            knob_waiting: HashSet::new(),
            last_encoder: None,
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
    /// bank when it still exists, and keeps the DAW feedback state, the
    /// encoder positions and the roll and flam settings. Returns
    /// what ends the notes the previous version left on (see `release_all`).
    pub fn reload(&mut self, profile: Profile) -> Output {
        let out = self.release_all();
        let bank = if self.bank < profile.banks.len() {
            self.bank
        } else {
            profile.start_bank
        };
        let follow = std::mem::take(&mut self.follow);
        let osc_follow = std::mem::take(&mut self.osc_follow);
        let encoder = std::mem::take(&mut self.encoder);
        let (roll_on, flam_on, clock) = (self.roll_on, self.flam_on, self.clock);
        let (roll_override, flam_override) = (self.roll_override, self.flam_override);
        let knobs = (
            std::mem::take(&mut self.knob_sent),
            std::mem::take(&mut self.knob_physical),
            std::mem::take(&mut self.knob_waiting),
        );
        *self = Engine::new(profile).with_file(std::mem::take(&mut self.file));
        (self.knob_sent, self.knob_physical, self.knob_waiting) = knobs;
        self.bank = bank;
        self.follow = follow;
        self.osc_follow = osc_follow;
        self.encoder = encoder;
        self.roll_on = roll_on;
        self.flam_on = flam_on;
        self.roll_override = roll_override;
        self.flam_override = flam_override;
        self.clock = clock;
        out
    }

    /// Ends what this engine left on in the DAW and the OSC software, before
    /// it goes (disconnection, profile change): held notes and CCs as if
    /// released, toggles turned off, and the note offs of pending flams.
    pub fn release_all(&mut self) -> Output {
        let mut out = Output::default();
        let held: Vec<(Control, usize)> = self.held.drain().collect();
        let toggled: Vec<(usize, Control)> = self.toggles.drain().collect();
        let controls = held
            .into_iter()
            .map(|(control, bank)| (bank, control, Mode::Momentary))
            .chain(toggled.into_iter().map(|(bank, control)| (bank, control, Mode::Toggle)));
        for (bank, control, state) in controls {
            match self.action(bank, control).cloned() {
                Some(ButtonAction::Note {
                    channel, note, mode, ..
                }) if mode == state => out.daw.push(note_off(channel, note)),
                Some(ButtonAction::Cc {
                    channel,
                    cc,
                    off_value,
                    mode,
                    ..
                }) if mode == state => out.daw.push(control_change(channel, cc, off_value)),
                Some(ButtonAction::Osc {
                    address,
                    release_args: Some(args),
                    mode,
                    ..
                }) if mode == state => out.osc.push(OscMessage::new(address, osc::resolve(&args, 0))),
                Some(ButtonAction::Key { keys, hold: true, .. }) if state == Mode::Momentary => {
                    out.keys.push(KeyEvent::new(&keys, KeyDirection::Release))
                }
                _ => {}
            }
        }
        // Grace notes of pending flams end; their main notes never start.
        out.daw.extend(
            std::mem::take(&mut self.scheduled)
                .into_iter()
                .map(|(_, message)| message)
                .filter(|message| matches!(message.first(), Some(status) if status & 0xF0 == 0x80)),
        );
        self.rolls.clear();
        out
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
        let mut messages: Vec<Vec<u8>> = Control::all()
            .map(|control| padkontrol::led(control, self.led_state(control)))
            .collect();
        messages.push(self.show_bank());
        messages
    }

    /// Text of the active bank on the display: its `display` field, or the
    /// start of its name.
    fn bank_text(&self) -> String {
        let bank = self.bank();
        bank.display
            .clone()
            .unwrap_or_else(|| bank.name.chars().filter(|c| !c.is_whitespace()).collect())
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

    /// Called every few milliseconds: plays the rolls and the scheduled flam
    /// notes that are due, and brings the bank back on the display once a
    /// value has been shown long enough.
    pub fn tick(&mut self, now: Instant) -> Output {
        let mut out = Output::default();
        if let Some(until) = self.display_until
            && now >= until
        {
            self.display_until = None;
            if self.shown != self.bank_text() {
                out.pad.push(self.show_bank());
            }
        }

        let interval = self.roll_interval(now);
        for roll in self.rolls.values_mut() {
            if now >= roll.next {
                out.daw.push(note_off(roll.channel, roll.note));
                out.daw.push(note_on(roll.channel, roll.note, roll.velocity));
                // Late ticks do not pile up repeats.
                roll.next = (roll.next + interval).max(now + interval / 2);
            }
        }

        if self.scheduled.first().is_some_and(|(at, _)| *at <= now) {
            let (due, later): (Vec<_>, Vec<_>) = self.scheduled.drain(..).partition(|(at, _)| *at <= now);
            self.scheduled = later;
            out.daw.extend(due.into_iter().map(|(_, message)| message));
        }
        out
    }

    fn schedule(&mut self, at: Instant, message: Vec<u8>) {
        let index = self.scheduled.partition_point(|(t, _)| *t <= at);
        self.scheduled.insert(index, (at, message));
    }

    /// Tempo of the DAW's MIDI clock when it runs, else the profile's.
    fn bpm(&self, now: Instant) -> f32 {
        self.clock.bpm(now).map_or(self.profile.repeat.bpm, |bpm| {
            bpm.clamp(*BPM_RANGE.start(), *BPM_RANGE.end())
        })
    }

    /// Time between two roll repeats.
    fn roll_interval(&self, now: Instant) -> Duration {
        let division = self.roll_override.unwrap_or(self.profile.repeat.roll);
        // A 4/4 bar lasts four beats.
        Duration::from_secs_f32(240.0 / self.bpm(now) / f32::from(division))
    }

    fn flam_gap(&self) -> Duration {
        Duration::from_millis(self.flam_override.unwrap_or(self.profile.repeat.flam).into())
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
            ControlEvent::Pad {
                pad,
                pressed: true,
                velocity,
            } => {
                // Shaped by the profile's velocity curve, for notes, OSC and the display.
                let velocity = self.profile.velocity.apply(velocity);
                self.press(Control::Pad(pad), Some(velocity), now, &mut out);
                let text = match (display.pads, self.action(self.bank, Control::Pad(pad))) {
                    (PadDisplay::Note, Some(ButtonAction::Note { note, .. })) => Some(note_name(*note)),
                    (PadDisplay::Velocity, Some(_)) => Some(format!("{velocity:>DISPLAY_LENGTH$}")),
                    _ => None,
                };
                if let Some(text) = text {
                    self.show_value(text, now, &mut out);
                }
            }
            ControlEvent::Pad {
                pad, pressed: false, ..
            } => self.release(Control::Pad(pad), now, &mut out),
            ControlEvent::Button { button, pressed: true } => self.press(Control::Button(button), None, now, &mut out),
            ControlEvent::Button { button, pressed: false } => self.release(Control::Button(button), now, &mut out),
            ControlEvent::XyTouch { pressed: true } => self.press(Control::XyTouch, None, now, &mut out),
            ControlEvent::XyTouch { pressed: false } => self.release(Control::XyTouch, now, &mut out),
            ControlEvent::Knob { knob, value } => {
                let action = self.bank().knobs.get(&knob).cloned();
                if !self.picked_up(knob, value, action.as_ref().is_some_and(KnobAction::pickup)) {
                    return out;
                }
                let shown = match action {
                    Some(KnobAction::Cc {
                        channel, cc, min, max, ..
                    }) => {
                        let scaled = (min as u16 + value as u16 * max.saturating_sub(min) as u16 / 127) as u8;
                        out.daw.push(control_change(channel, cc, scaled));
                        Some(scaled)
                    }
                    Some(KnobAction::Osc {
                        address,
                        min,
                        max,
                        float,
                        ..
                    }) => {
                        out.osc
                            .push(OscMessage::new(address, vec![scale_osc(value, min, max, float)]));
                        Some(value)
                    }
                    None => None,
                };
                if let (Some(shown), true) = (shown, display.knobs) {
                    self.show_value(format!("{shown:>DISPLAY_LENGTH$}"), now, &mut out);
                }
            }
            // Like on the hardware: while roll or flam is on, X sets their speed.
            ControlEvent::Xy { x, .. } if self.profile.repeat.xy_speed && (self.roll_on || self.flam_on) => {
                let shown = if self.roll_on {
                    let division = ROLL_DIVISIONS[usize::from(x) * ROLL_DIVISIONS.len() / 128];
                    self.roll_override = Some(division);
                    division.to_string()
                } else {
                    let (min, max) = (*FLAM_RANGE_MS.start(), *FLAM_RANGE_MS.end());
                    let gap = min + u16::from(x) * (max - min) / 127;
                    self.flam_override = Some(gap);
                    gap.to_string()
                };
                self.show_value(format!("{shown:>DISPLAY_LENGTH$}"), now, &mut out);
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
                    out.osc.push(OscMessage::new(
                        address,
                        vec![OscArg::Int(x.into()), OscArg::Int(y.into())],
                    ));
                }
                None => {}
            },
            ControlEvent::Encoder { delta } => {
                if let Some(action) = self.bank().encoder.clone() {
                    let delta = self.accelerate(delta, action.acceleration(), now);
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
                        EncoderAction::Key { left, right } => {
                            let keys = if delta < 0 { left } else { right };
                            let steps = delta.unsigned_abs().min(MAX_KEY_STEPS);
                            out.keys
                                .extend((0..steps).map(|_| KeyEvent::new(&keys, KeyDirection::Tap)));
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
        self.feedback_at(message, Instant::now())
    }

    fn feedback_at(&mut self, message: &[u8], now: Instant) -> Vec<Vec<u8>> {
        // MIDI clock from the DAW: sets the roll tempo.
        if message == [MIDI_CLOCK] {
            self.clock.pulse(now);
            return vec![];
        }
        let (Some(&status), Some(&number)) = (message.first(), message.get(1)) else {
            return vec![];
        };
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

    fn press(&mut self, control: Control, pad_velocity: Option<u8>, now: Instant, out: &mut Output) {
        let bank = self.bank;
        let Some(action) = self.action(bank, control).cloned() else {
            return;
        };
        self.held.insert(control, bank);

        match action {
            ButtonAction::Note {
                channel,
                note,
                velocity,
                mode,
                ..
            } => {
                let velocity = match velocity {
                    Some(Velocity::Fixed(v)) => v,
                    _ => pad_velocity.unwrap_or(127),
                }
                .max(1);
                // Roll and flam apply to pads playing momentary notes.
                let repeatable = mode == Mode::Momentary && matches!(control, Control::Pad(_));
                let mut first_repeat = now;
                if repeatable && self.flam_on {
                    let gap = self.flam_gap();
                    let grace = (f32::from(velocity) * FLAM_GRACE).round().max(1.0) as u8;
                    out.daw.push(note_on(channel, note, grace));
                    self.schedule(now + gap, note_off(channel, note));
                    self.schedule(now + gap, note_on(channel, note, velocity));
                    first_repeat = now + gap;
                } else if mode == Mode::Momentary || self.flip(bank, control) {
                    out.daw.push(note_on(channel, note, velocity));
                } else {
                    out.daw.push(note_off(channel, note));
                }
                if repeatable && self.roll_on {
                    let next = first_repeat + self.roll_interval(now);
                    let roll = Roll {
                        channel,
                        note,
                        velocity,
                        next,
                    };
                    self.rolls.insert(control, roll);
                }
            }
            ButtonAction::Cc {
                channel,
                cc,
                on_value,
                off_value,
                mode,
                ..
            } => {
                let on = mode == Mode::Momentary || self.flip(bank, control);
                out.daw
                    .push(control_change(channel, cc, if on { on_value } else { off_value }));
            }
            ButtonAction::ProgramChange { channel, program, .. } => {
                out.daw.push(vec![0xC0 | (channel - 1), program]);
            }
            ButtonAction::SwitchBank { bank: target, .. } => self.switch_bank(target, out),
            ButtonAction::SwitchProfile { profile } => out.switch_profile = Some(profile),
            ButtonAction::Osc {
                address,
                args,
                release_args,
                mode,
                ..
            } => {
                let velocity = pad_velocity.unwrap_or(127);
                if mode == Mode::Momentary || self.flip(bank, control) {
                    out.osc.push(OscMessage::new(address, osc::resolve(&args, velocity)));
                } else if let Some(release_args) = release_args {
                    out.osc
                        .push(OscMessage::new(address, osc::resolve(&release_args, velocity)));
                }
            }
            ButtonAction::Key { keys, hold, .. } => {
                let direction = if hold { KeyDirection::Press } else { KeyDirection::Tap };
                out.keys.push(KeyEvent::new(&keys, direction));
            }
            ButtonAction::Roll {} => {
                self.roll_on = !self.roll_on;
                if !self.roll_on {
                    // Held notes stop repeating and end on release.
                    self.rolls.clear();
                }
            }
            ButtonAction::Flam {} => self.flam_on = !self.flam_on,
        }
        self.update_led(control, out);
    }

    fn release(&mut self, control: Control, now: Instant, out: &mut Output) {
        let Some(pressed_in) = self.held.remove(&control) else {
            return;
        };
        let Some(action) = self.action(pressed_in, control).cloned() else {
            return;
        };

        match action {
            ButtonAction::Note {
                channel,
                note,
                mode: Mode::Momentary,
                ..
            } => {
                self.rolls.remove(&control);
                // Released before its flam main note: end the note right after it.
                let main_note = self
                    .scheduled
                    .iter()
                    .filter(|(_, message)| is_note_on(message, channel, note))
                    .map(|(at, _)| *at)
                    .max();
                match main_note {
                    Some(at) => self.schedule(at.max(now) + RELEASE_AFTER_FLAM, note_off(channel, note)),
                    None => out.daw.push(note_off(channel, note)),
                }
            }
            ButtonAction::Cc {
                channel,
                cc,
                off_value,
                mode: Mode::Momentary,
                ..
            } => out.daw.push(control_change(channel, cc, off_value)),
            ButtonAction::SwitchBank {
                bank: target,
                momentary: true,
            } if self.bank == target => self.switch_bank(pressed_in, out),
            ButtonAction::Osc {
                address,
                release_args: Some(release_args),
                mode: Mode::Momentary,
                ..
            } => {
                out.osc.push(OscMessage::new(address, osc::resolve(&release_args, 0)));
            }
            ButtonAction::Key { keys, hold: true, .. } => out.keys.push(KeyEvent::new(&keys, KeyDirection::Release)),
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
        // Knobs that moved since this bank last used them wait to be picked up.
        for (&(knob_bank, knob), &sent) in &self.knob_sent {
            if knob_bank == bank && self.knob_physical.get(&knob).is_some_and(|&now| now != sent) {
                self.knob_waiting.insert((bank, knob));
            }
        }
        out.bank_changed = true;
        out.pad.extend(self.refresh());
    }

    /// Whether a knob move goes through. With `pickup`, after a bank change
    /// the knob waits until it reaches, or crosses, the value it had in this
    /// bank, so that the target does not jump.
    fn picked_up(&mut self, knob: u8, value: u8, pickup: bool) -> bool {
        let key = (self.bank, knob);
        let previous = self.knob_physical.insert(knob, value);
        if pickup && self.knob_waiting.contains(&key) {
            let target = self.knob_sent[&key];
            let from = previous.unwrap_or(value);
            let reached =
                (from.min(value)..=from.max(value)).contains(&target) || value.abs_diff(target) <= PICKUP_TOLERANCE;
            if !reached {
                return false;
            }
        }
        self.knob_waiting.remove(&key);
        self.knob_sent.insert(key, value);
        true
    }

    /// Steps of an encoder move, more of them when turned fast.
    fn accelerate(&mut self, delta: i8, acceleration: bool, now: Instant) -> i8 {
        let previous = self.last_encoder.replace(now);
        if !acceleration {
            return delta;
        }
        let factor = match previous.map(|at| now.saturating_duration_since(at)) {
            Some(gap) if gap < Duration::from_millis(20) => 4,
            Some(gap) if gap < Duration::from_millis(50) => 2,
            _ => 1,
        };
        delta.saturating_mul(factor)
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
        let Some(action) = self.action(self.bank, control) else {
            return false;
        };
        match action {
            ButtonAction::Roll {} => return self.roll_on,
            ButtonAction::Flam {} => return self.flam_on,
            _ => {}
        }
        match led_mode(action) {
            Led::None => false,
            Led::Press => self.held.contains_key(&control),
            Led::Toggle => self.toggles.contains(&(self.bank, control)),
            Led::Follow => match action {
                ButtonAction::Osc { address, .. } => self.osc_follow.get(address).copied().unwrap_or(false),
                _ => follow_key(action)
                    .and_then(|k| self.follow.get(&k))
                    .copied()
                    .unwrap_or(false),
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
        let local = message
            .address
            .strip_prefix(prefix)
            .filter(|rest| rest.starts_with('/'));

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
                // Only addresses some action follows: any sender could
                // otherwise fill this table without limit.
                let followed = self.profile.banks.iter().any(|bank| {
                    bank.pads
                        .values()
                        .chain(bank.buttons.values())
                        .any(|a| matches!(a, ButtonAction::Osc { address, .. } if *address == message.address))
                });
                if !followed {
                    return out;
                }
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
        ButtonAction::Note { led, mode, .. }
        | ButtonAction::Cc { led, mode, .. }
        | ButtonAction::Osc { led, mode, .. } => led.unwrap_or(match mode {
            Mode::Momentary => Led::Press,
            Mode::Toggle => Led::Toggle,
        }),
        ButtonAction::ProgramChange { led, .. } | ButtonAction::Key { led, .. } => led.unwrap_or(Led::Press),
        ButtonAction::SwitchBank { .. } | ButtonAction::SwitchProfile { .. } => Led::Press,
        // Lit while the mode is on (see `led_state`).
        ButtonAction::Roll {} | ButtonAction::Flam {} => Led::Toggle,
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
    if float {
        OscArg::Float(scaled)
    } else {
        OscArg::Int(scaled.round() as i32)
    }
}

/// Note name for the display, with middle C (60) as C3 like most DAWs.
fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[note as usize % 12], note as i16 / 12 - 2)
}

fn note_on(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    vec![0x90 | (channel - 1), note, velocity]
}

/// Timing clock message, 24 per quarter note.
pub const MIDI_CLOCK: u8 = 0xF8;

fn is_note_on(message: &[u8], channel: u8, note: u8) -> bool {
    matches!(*message, [status, n, velocity] if status == 0x90 | (channel - 1) && n == note && velocity > 0)
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
    use crate::config::{ButtonId, DEFAULT_PROFILE_JSON, parse_profile};

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
        assert_eq!(
            e.handle(&ControlEvent::Knob { knob: 2, value: 0 }).daw,
            vec![vec![0xB2, 10, 32]]
        );
        assert_eq!(
            e.handle(&ControlEvent::Knob { knob: 2, value: 127 }).daw,
            vec![vec![0xB2, 10, 96]]
        );
    }

    #[test]
    fn xy_only_sends_changed_axes() {
        let mut e = engine(r#"{ "name": "A", "xy": { "type": "xy", "channel": 1, "ccX": 3, "ccY": 4 } }"#);
        assert_eq!(
            e.handle(&ControlEvent::Xy { x: 10, y: 20 }).daw,
            vec![vec![0xB0, 3, 10], vec![0xB0, 4, 20]]
        );
        assert_eq!(
            e.handle(&ControlEvent::Xy { x: 11, y: 20 }).daw,
            vec![vec![0xB0, 3, 11]]
        );
    }

    #[test]
    fn encoder_relative_and_absolute() {
        let mut e = engine(
            r#"{ "name": "R", "encoder": { "type": "cc", "channel": 1, "cc": 10 },
                 "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "A", "encoder": { "type": "cc", "channel": 1, "cc": 11, "encoding": "absolute" } }"#,
        );
        assert_eq!(
            e.handle(&ControlEvent::Encoder { delta: 1 }).daw,
            vec![vec![0xB0, 10, 1]]
        );
        assert_eq!(
            e.handle(&ControlEvent::Encoder { delta: -1 }).daw,
            vec![vec![0xB0, 10, 127]]
        );

        e.handle(&button(ButtonId::Setting, true));
        assert_eq!(
            e.handle(&ControlEvent::Encoder { delta: -1 }).daw,
            vec![vec![0xB0, 11, 0]]
        );
        assert_eq!(
            e.handle(&ControlEvent::Encoder { delta: 3 }).daw,
            vec![vec![0xB0, 11, 3]]
        );
    }

    #[test]
    fn follow_leds_track_daw_feedback() {
        let mut e = engine(
            r#"{ "name": "A", "pads": { "5": { "type": "note", "channel": 2, "note": 64, "led": "follow" } } }"#,
        );
        let control = Control::Pad(5);
        assert_eq!(e.feedback(&[0x91, 64, 127]), vec![padkontrol::led(control, true)]);
        assert!(
            e.feedback(&[0x91, 64, 100]).is_empty(),
            "state unchanged, nothing to send"
        );
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
        assert_eq!(e.state().banks, vec!["Drums", "Clips"]);
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
        assert!(
            e.handle_at(&ControlEvent::Knob { knob: 1, value: 64 }, t0)
                .pad
                .is_empty()
        );

        assert!(e.tick(t0 + Duration::from_millis(500)).pad.is_empty());
        assert_eq!(
            e.tick(t0 + Duration::from_millis(1000)).pad,
            vec![padkontrol::display("BAT")]
        );
        assert!(e.tick(t0 + Duration::from_millis(1500)).pad.is_empty());
    }

    #[test]
    fn a_new_value_extends_the_display_time() {
        let mut e = engine(r#"{ "name": "Bat", "knobs": { "1": { "type": "cc", "channel": 1, "cc": 7 } } }"#);
        e.refresh();
        let t0 = Instant::now();
        e.handle_at(&ControlEvent::Knob { knob: 1, value: 10 }, t0);
        e.handle_at(
            &ControlEvent::Knob { knob: 1, value: 20 },
            t0 + Duration::from_millis(800),
        );
        assert!(e.tick(t0 + Duration::from_millis(1200)).pad.is_empty());
        assert_eq!(
            e.tick(t0 + Duration::from_millis(1800)).pad,
            vec![padkontrol::display("BAT")]
        );
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
        assert_eq!(
            turn(&mut e, -1).encoder_position,
            Some(0),
            "never negative in absolute mode"
        );
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
        assert!(
            e.handle_at(&pad(1, true, 90), t0).pad.iter().all(|m| m[5] != 0x22),
            "nothing by default"
        );

        let mut e = engine_with(&format!(
            r#"{{ "version": 1, "name": "T", "display": {{ "pads": "note" }}, "banks": [{{ "name": "A", {pads} }}] }}"#
        ));
        assert!(
            e.handle_at(&pad(1, true, 90), t0)
                .pad
                .contains(&padkontrol::display("C#1"))
        );

        let mut e = engine_with(&format!(
            r#"{{ "version": 1, "name": "T", "display": {{ "pads": "velocity" }}, "banks": [{{ "name": "A", {pads} }}] }}"#
        ));
        assert!(
            e.handle_at(&pad(1, true, 90), t0)
                .pad
                .contains(&padkontrol::display(" 90"))
        );
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
        assert!(
            out.pad.contains(&padkontrol::display(" 64")),
            "the display still shows the knob"
        );
        assert_eq!(
            osc_text(&e.handle(&ControlEvent::Knob { knob: 2, value: 127 })),
            vec!["/pan 1.0"]
        );
        assert_eq!(
            osc_text(&e.handle(&ControlEvent::Knob { knob: 2, value: 0 })),
            vec!["/pan -1.0"]
        );
        assert_eq!(
            osc_text(&e.handle(&ControlEvent::Xy { x: 10, y: 20 })),
            vec!["/xy 10 20"]
        );

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
        assert!(
            e.osc_feedback(&osc_in("/live/clip/1", vec![OscArg::Float(0.7)]), "/tk")
                .pad
                .is_empty(),
            "no change"
        );
        assert_eq!(
            e.osc_feedback(&osc_in("/live/clip/1", vec![OscArg::Int(0)]), "/tk").pad,
            vec![padkontrol::led(Control::Pad(1), false)]
        );
        // Pad 2 does not follow (press LED): nothing to send.
        assert!(
            e.osc_feedback(&osc_in("/live/clip/2", vec![OscArg::Int(1)]), "/tk")
                .pad
                .is_empty()
        );
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
        assert!(
            e.osc_feedback(&osc_in("/tk/led/button/shift", vec![OscArg::Int(1)]), "/tk")
                .pad
                .is_empty()
        );
        assert!(
            e.osc_feedback(&osc_in("/tk/led/pad/17", vec![OscArg::Int(1)]), "/tk")
                .pad
                .is_empty()
        );

        let out = e.osc_feedback(&osc_in("/tk/display", vec![OscArg::Str("ABC".into())]), "/tk");
        assert_eq!(out.pad, vec![padkontrol::display("ABC")]);
        assert!(
            e.tick(Instant::now() + Duration::from_secs(10)).pad.is_empty(),
            "the text stays"
        );

        let out = e.osc_feedback(&osc_in("/tk/bank", vec![OscArg::Int(1)]), "/tk");
        assert!(out.bank_changed);
        assert!(
            out.pad.contains(&padkontrol::led(Control::Pad(3), false)),
            "overrides cleared by the bank change"
        );
        assert!(out.pad.contains(&padkontrol::display("DEU")));
        assert!(
            !e.osc_feedback(&osc_in("/tk/bank", vec![OscArg::Int(9)]), "/tk")
                .bank_changed
        );
    }

    #[test]
    fn osc_follow_survives_a_profile_reload() {
        let mut e = engine(r#"{ "name": "A", "pads": { "1": { "type": "osc", "address": "/c", "led": "follow" } } }"#);
        e.osc_feedback(&osc_in("/c", vec![OscArg::Int(1)]), "/tk");
        let profile = e.profile.clone();
        e.reload(profile);
        assert!(e.refresh().contains(&padkontrol::led(Control::Pad(1), true)));

        // Addresses no action uses are not kept.
        for i in 0..100 {
            e.osc_feedback(&osc_in(&format!("/noise/{i}"), vec![OscArg::Int(1)]), "/tk");
        }
        assert_eq!(e.osc_follow.len(), 1);
    }

    #[test]
    fn velocity_curve_shapes_pad_hits() {
        let pads = r#""pads": { "1": { "type": "note", "channel": 1, "note": 36 },
                                "2": { "type": "note", "channel": 1, "note": 37, "velocity": 100 },
                                "3": { "type": "osc", "address": "/hit", "args": ["$velocity"] } }"#;
        let hit = |e: &mut Engine, pad_number, velocity| e.handle(&pad(pad_number, true, velocity));

        let mut e = engine_with(&format!(
            r#"{{ "version": 1, "name": "T", "velocity": {{ "curve": "soft", "min": 40, "max": 120 }},
                  "display": {{ "pads": "velocity" }}, "banks": [{{ "name": "A", {pads} }}] }}"#
        ));
        assert_eq!(hit(&mut e, 1, 1).daw, vec![vec![0x90, 36, 40]], "softest hit gives min");
        assert_eq!(
            hit(&mut e, 1, 127).daw,
            vec![vec![0x90, 36, 120]],
            "hardest hit gives max"
        );
        let out = hit(&mut e, 1, 64);
        let soft_middle = out.daw[0][2];
        assert!(soft_middle > 80, "a soft curve lifts light hits: {soft_middle}");
        assert!(
            out.pad.contains(&padkontrol::display(&format!("{soft_middle:>3}"))),
            "the display shows it"
        );
        assert_eq!(
            hit(&mut e, 2, 10).daw,
            vec![vec![0x90, 37, 100]],
            "fixed velocities are left alone"
        );
        assert_eq!(osc_text(&hit(&mut e, 3, 127)), vec!["/hit 120"]);

        let mut e = engine_with(&format!(
            r#"{{ "version": 1, "name": "T", "velocity": {{ "curve": "hard" }}, "banks": [{{ "name": "A", {pads} }}] }}"#
        ));
        let hard_middle = hit(&mut e, 1, 64).daw[0][2];
        assert!(hard_middle < 45, "a hard curve holds back light hits: {hard_middle}");

        // Linear by default: the hit is sent as is.
        let mut e = engine(&format!(r#"{{ "name": "A", {pads} }}"#));
        assert_eq!(hit(&mut e, 1, 64).daw, vec![vec![0x90, 36, 64]]);
    }

    /// Bank with a note on pad 1 and the ROLL / FLAM buttons; 120 BPM, 1/16
    /// rolls (125 ms) and 30 ms flams unless `repeat` says otherwise.
    fn repeat_engine(repeat: &str) -> Engine {
        engine_with(&format!(
            r#"{{ "version": 1, "name": "T", "repeat": {{ {repeat} }}, "banks": [{{ "name": "A",
                "pads": {{ "1": {{ "type": "note", "channel": 1, "note": 36 }},
                           "2": {{ "type": "note", "channel": 1, "note": 37, "mode": "toggle" }} }},
                "buttons": {{ "roll": {{ "type": "roll" }}, "flam": {{ "type": "flam" }} }},
                "xy": {{ "type": "xy", "channel": 1, "ccX": 3, "ccY": 4 }},
                "encoder": {{ "type": "cc", "channel": 1, "cc": 10 }} }}] }}"#
        ))
    }

    fn press_at(e: &mut Engine, event: ControlEvent, at: Instant) -> Output {
        e.handle_at(&event, at)
    }

    #[test]
    fn roll_repeats_a_held_pad_on_the_beat() {
        let mut e = repeat_engine("");
        let t0 = Instant::now();
        let out = press_at(&mut e, button(ButtonId::Roll, true), t0);
        assert_eq!(
            led_on(&out, Control::Button(ButtonId::Roll)),
            Some(true),
            "the ROLL LED shows the mode"
        );
        assert_eq!(
            led_on(
                &press_at(&mut e, button(ButtonId::Roll, false), t0),
                Control::Button(ButtonId::Roll)
            ),
            Some(true),
            "still on after release"
        );

        assert_eq!(press_at(&mut e, pad(1, true, 100), t0).daw, vec![vec![0x90, 36, 100]]);
        assert!(e.tick(t0 + Duration::from_millis(100)).daw.is_empty());
        assert_eq!(
            e.tick(t0 + Duration::from_millis(126)).daw,
            vec![vec![0x80, 36, 0], vec![0x90, 36, 100]],
            "one 1/16 later at 120 BPM"
        );
        assert!(e.tick(t0 + Duration::from_millis(200)).daw.is_empty());
        assert_eq!(e.tick(t0 + Duration::from_millis(251)).daw.len(), 2);

        assert_eq!(press_at(&mut e, pad(1, false, 0), t0).daw, vec![vec![0x80, 36, 0]]);
        assert!(
            e.tick(t0 + Duration::from_millis(500)).daw.is_empty(),
            "stops on release"
        );

        // Toggle pads do not roll.
        press_at(&mut e, pad(2, true, 90), t0);
        assert!(e.tick(t0 + Duration::from_millis(500)).daw.is_empty());

        // Roll off: nothing repeats any more.
        press_at(&mut e, button(ButtonId::Roll, true), t0);
        press_at(&mut e, pad(1, true, 100), t0);
        assert!(e.tick(t0 + Duration::from_millis(500)).daw.is_empty());
    }

    #[test]
    fn flam_plays_a_softer_grace_note_first() {
        let mut e = repeat_engine(r#""flam": 40"#);
        let t0 = Instant::now();
        press_at(&mut e, button(ButtonId::Flam, true), t0);

        assert_eq!(
            press_at(&mut e, pad(1, true, 100), t0).daw,
            vec![vec![0x90, 36, 60]],
            "grace note at 60 %"
        );
        assert!(e.tick(t0 + Duration::from_millis(30)).daw.is_empty());
        assert_eq!(
            e.tick(t0 + Duration::from_millis(40)).daw,
            vec![vec![0x80, 36, 0], vec![0x90, 36, 100]],
            "main note 40 ms later"
        );
        assert_eq!(press_at(&mut e, pad(1, false, 0), t0).daw, vec![vec![0x80, 36, 0]]);

        // Released before the main note: the note off comes right after it.
        press_at(&mut e, pad(1, true, 100), t0);
        assert!(
            press_at(&mut e, pad(1, false, 0), t0 + Duration::from_millis(10))
                .daw
                .is_empty()
        );
        let main = e.tick(t0 + Duration::from_millis(40)).daw;
        assert_eq!(main.last(), Some(&vec![0x90, 36, 100]));
        assert_eq!(e.tick(t0 + Duration::from_millis(60)).daw, vec![vec![0x80, 36, 0]]);
    }

    #[test]
    fn xy_sets_roll_speed_while_roll_is_on() {
        let mut e = repeat_engine("");
        let t0 = Instant::now();
        // Normal XY action while roll and flam are off.
        assert_eq!(press_at(&mut e, ControlEvent::Xy { x: 127, y: 0 }, t0).daw.len(), 2);

        press_at(&mut e, button(ButtonId::Roll, true), t0);
        let out = press_at(&mut e, ControlEvent::Xy { x: 127, y: 50 }, t0);
        assert!(out.daw.is_empty(), "the XY pad sets the speed instead");
        assert!(out.pad.contains(&padkontrol::display(" 32")));
        press_at(&mut e, pad(1, true, 100), t0);
        // 1/32 at 120 BPM: 62.5 ms.
        assert_eq!(e.tick(t0 + Duration::from_millis(63)).daw.len(), 2);

        // xySpeed off: the XY pad keeps its action.
        let mut e = repeat_engine(r#""xySpeed": false"#);
        press_at(&mut e, button(ButtonId::Roll, true), t0);
        assert_eq!(press_at(&mut e, ControlEvent::Xy { x: 127, y: 0 }, t0).daw.len(), 2);
    }

    #[test]
    fn roll_follows_the_daw_clock() {
        let mut e = repeat_engine("");
        let t0 = Instant::now();
        // 24 pulses per quarter note at 60 BPM: one every 41.67 ms.
        let pulse = Duration::from_secs_f64(1.0 / 24.0);
        for i in 0..48 {
            e.feedback_at(&[MIDI_CLOCK], t0 + pulse * i);
        }
        let now = t0 + pulse * 47;
        assert!((e.bpm(now) - 60.0).abs() < 1.0, "clock tempo: {}", e.bpm(now));
        // 1/16 at 60 BPM: 250 ms.
        assert!((e.roll_interval(now).as_secs_f32() - 0.25).abs() < 0.01);
        // The clock stopped: back to the profile's tempo.
        assert_eq!(e.bpm(now + Duration::from_secs(1)), 120.0);
    }

    #[test]
    fn knob_pickup_waits_for_the_value_of_the_bank() {
        let mut e = engine(
            r#"{ "name": "A", "knobs": { "1": { "type": "cc", "channel": 1, "cc": 7, "pickup": true } },
                 "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "B", "knobs": { "1": { "type": "cc", "channel": 1, "cc": 8, "pickup": true } },
                 "buttons": { "setting": { "type": "switchBank", "bank": 0 } } }"#,
        );
        let knob = |value| ControlEvent::Knob { knob: 1, value };
        assert_eq!(
            e.handle(&knob(100)).daw,
            vec![vec![0xB0, 7, 100]],
            "first use: no pickup"
        );
        e.handle(&button(ButtonId::Setting, true));
        // Bank B never saw this knob: it follows at once.
        assert_eq!(e.handle(&knob(20)).daw, vec![vec![0xB0, 8, 20]]);
        e.handle(&button(ButtonId::Setting, false));
        e.handle(&button(ButtonId::Setting, true));
        // Back in A, the knob is at 20 but A last sent 100.
        assert_eq!(e.state().bank, 0);
        assert!(e.handle(&knob(40)).daw.is_empty());
        assert!(e.handle(&knob(90)).daw.is_empty());
        assert_eq!(
            e.handle(&knob(105)).daw,
            vec![vec![0xB0, 7, 105]],
            "crossed 100: picked up"
        );
        assert_eq!(e.handle(&knob(60)).daw, vec![vec![0xB0, 7, 60]], "then follows");
    }

    #[test]
    fn knobs_without_pickup_jump() {
        let mut e = engine(
            r#"{ "name": "A", "knobs": { "1": { "type": "cc", "channel": 1, "cc": 7 } },
                 "buttons": { "setting": { "type": "switchBank", "bank": 1 } } },
               { "name": "B", "buttons": { "setting": { "type": "switchBank", "bank": 0 } } }"#,
        );
        let knob = |value| ControlEvent::Knob { knob: 1, value };
        e.handle(&knob(100));
        e.handle(&button(ButtonId::Setting, true));
        e.handle(&knob(20));
        e.handle(&button(ButtonId::Setting, false));
        e.handle(&button(ButtonId::Setting, true));
        assert_eq!(e.handle(&knob(25)).daw, vec![vec![0xB0, 7, 25]]);
    }

    #[test]
    fn encoder_accelerates_when_turned_fast() {
        let mut e =
            engine(r#"{ "name": "A", "encoder": { "type": "cc", "channel": 1, "cc": 10, "acceleration": true } }"#);
        let t0 = Instant::now();
        let turn = |e: &mut Engine, at| {
            e.handle_at(&ControlEvent::Encoder { delta: 1 }, t0 + at)
                .encoder_position
        };
        assert_eq!(turn(&mut e, Duration::ZERO), Some(1));
        assert_eq!(turn(&mut e, Duration::from_millis(500)), Some(2), "slow: one step");
        assert_eq!(turn(&mut e, Duration::from_millis(530)), Some(4), "faster: two");
        assert_eq!(turn(&mut e, Duration::from_millis(540)), Some(8), "fast: four");

        let mut plain = engine(r#"{ "name": "A", "encoder": { "type": "cc", "channel": 1, "cc": 10 } }"#);
        plain.handle_at(&ControlEvent::Encoder { delta: 1 }, t0);
        let out = plain.handle_at(&ControlEvent::Encoder { delta: 1 }, t0 + Duration::from_millis(5));
        assert_eq!(out.encoder_position, Some(2));
    }

    #[test]
    fn switch_profile_asks_for_the_file() {
        let mut e = engine(r#"{ "name": "A", "pads": { "1": { "type": "switchProfile", "profile": "live.json" } } }"#);
        let out = e.handle(&pad(1, true, 100));
        assert_eq!(out.switch_profile.as_deref(), Some("live.json"));
        assert_eq!(led_on(&out, Control::Pad(1)), Some(true));
    }

    #[test]
    fn reload_keeps_the_encoder_and_the_roll_settings() {
        let mut e = repeat_engine("");
        let t0 = Instant::now();
        e.handle_at(&button(ButtonId::Roll, true), t0);
        e.handle_at(&ControlEvent::Xy { x: 127, y: 0 }, t0);
        e.handle_at(&ControlEvent::Encoder { delta: 5 }, t0);
        let before = e.roll_interval(t0);
        let profile = e.profile.clone();
        e.reload(profile);
        assert!(e.roll_on);
        assert_eq!(e.roll_interval(t0), before, "the 1/32 set with the XY pad stays");
        assert_eq!(e.state().encoder, 5);
    }

    #[test]
    fn release_all_ends_what_is_on() {
        let json = r#"{ "version": 1, "name": "T", "banks": [{ "name": "A",
                "pads": { "1": { "type": "note", "channel": 1, "note": 36 },
                          "2": { "type": "cc", "channel": 2, "cc": 7, "mode": "toggle", "offValue": 5 },
                          "3": { "type": "note", "channel": 1, "note": 38 },
                          "4": { "type": "note", "channel": 1, "note": 40, "mode": "toggle" } },
                "buttons": { "flam": { "type": "flam" } } }] }"#;
        let mut e = engine_with(json);
        let t0 = Instant::now();
        e.handle_at(&pad(1, true, 100), t0);
        e.handle_at(&pad(2, true, 100), t0);
        e.handle_at(&pad(2, false, 0), t0);
        // Toggled on then off again: nothing to end.
        e.handle_at(&pad(4, true, 100), t0);
        e.handle_at(&pad(4, false, 0), t0);
        e.handle_at(&pad(4, true, 100), t0);
        e.handle_at(&pad(4, false, 0), t0);
        // A flam in flight: its grace note is on, its main note pending.
        e.handle_at(&button(ButtonId::Flam, true), t0);
        e.handle_at(&pad(3, true, 100), t0);

        let mut daw = e.release_all().daw;
        daw.sort();
        assert_eq!(
            daw,
            vec![
                vec![0x80, 36, 0],
                vec![0x80, 38, 0],
                vec![0x80, 38, 0],
                vec![0xB1, 7, 5]
            ],
            "held note, the flam's grace note (pad 3 is held too) and the toggled CC"
        );
        assert!(
            e.tick(t0 + Duration::from_secs(1)).daw.is_empty(),
            "the main flam note never starts"
        );
        assert!(e.handle(&pad(1, false, 0)).daw.is_empty(), "already released");
        assert!(e.release_all().daw.is_empty());

        // Reloading ends the notes too.
        e.handle(&button(ButtonId::Flam, true));
        e.handle(&pad(1, true, 100));
        assert_eq!(e.reload(parse_profile(json).unwrap()).daw, vec![vec![0x80, 36, 0]]);
    }

    #[test]
    fn clock_bursts_do_not_break_the_tempo() {
        let mut e = engine_with(r#"{ "version": 1, "name": "T", "banks": [{ "name": "A" }] }"#);
        let t0 = Instant::now();
        // Two pulses at once, then a steady 120 BPM (20.8 ms per pulse).
        e.feedback_at(&[MIDI_CLOCK], t0);
        e.feedback_at(&[MIDI_CLOCK], t0);
        let pulse = Duration::from_secs_f64(60.0 / 120.0 / 24.0);
        for i in 1..48 {
            e.feedback_at(&[MIDI_CLOCK], t0 + pulse * i);
        }
        let now = t0 + pulse * 47;
        assert!((e.bpm(now) - 120.0).abs() < 1.0, "{}", e.bpm(now));
        // A clock far too fast is held to the supported range.
        let mut e = engine_with(r#"{ "version": 1, "name": "T", "banks": [{ "name": "A" }] }"#);
        for i in 0..48 {
            e.feedback_at(&[MIDI_CLOCK], t0 + Duration::from_millis(2) * i);
        }
        assert_eq!(e.bpm(t0 + Duration::from_millis(94)), 300.0);
    }

    #[test]
    fn key_actions_type_shortcuts() {
        let mut e = engine_with(
            r#"{ "version": 1, "name": "T", "banks": [{ "name": "A",
                "pads": { "1": { "type": "key", "keys": "ctrl+s" },
                          "2": { "type": "key", "keys": "shift", "hold": true } },
                "encoder": { "type": "key", "left": "ctrl+minus", "right": "ctrl+plus" } }] }"#,
        );
        let tap = e.handle(&pad(1, true, 100));
        assert_eq!(tap.keys, vec![KeyEvent::new("ctrl+s", KeyDirection::Tap)]);
        assert!(tap.daw.is_empty());
        assert_eq!(led_on(&tap, Control::Pad(1)), Some(true), "lit while pressed");
        assert!(e.handle(&pad(1, false, 0)).keys.is_empty());

        assert_eq!(
            e.handle(&pad(2, true, 100)).keys,
            vec![KeyEvent::new("shift", KeyDirection::Press)]
        );
        assert_eq!(
            e.handle(&pad(2, false, 0)).keys,
            vec![KeyEvent::new("shift", KeyDirection::Release)]
        );

        let turned = e.handle(&ControlEvent::Encoder { delta: -2 });
        assert_eq!(turned.keys, vec![KeyEvent::new("ctrl+minus", KeyDirection::Tap); 2]);
        let fast = e.handle(&ControlEvent::Encoder { delta: 40 });
        assert_eq!(fast.keys.len(), usize::from(MAX_KEY_STEPS));
        assert_eq!(fast.keys[0].keys, "ctrl+plus");
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
