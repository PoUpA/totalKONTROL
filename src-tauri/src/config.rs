//! Mapping profiles: JSON format (see `profiles/profile.schema.json`),
//! loading, validation and storage in the app's config directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::keys::KeysError;

pub const FORMAT_VERSION: u32 = 1;
pub const SCHEMA_FILE: &str = "profile.schema.json";
pub const SCHEMA_JSON: &str = include_str!("../profiles/profile.schema.json");
pub const DEFAULT_PROFILE_FILE: &str = "default.json";
pub const DEFAULT_PROFILE_JSON: &str = include_str!("../profiles/default.json");

/// Profiles to start from, bundled with the app: id and JSON.
pub const PRESETS: [(&str, &str); 14] = [
    ("ableton-live", include_str!("../profiles/presets/ableton-live.json")),
    ("bitwig-studio", include_str!("../profiles/presets/bitwig-studio.json")),
    ("cubase", include_str!("../profiles/presets/cubase.json")),
    ("fl-studio", include_str!("../profiles/presets/fl-studio.json")),
    ("logic-pro", include_str!("../profiles/presets/logic-pro.json")),
    ("reaper", include_str!("../profiles/presets/reaper.json")),
    ("traktor-pro", include_str!("../profiles/presets/traktor-pro.json")),
    ("mixxx", include_str!("../profiles/presets/mixxx.json")),
    ("serato-dj", include_str!("../profiles/presets/serato-dj.json")),
    ("rekordbox", include_str!("../profiles/presets/rekordbox.json")),
    ("obs", include_str!("../profiles/presets/obs.json")),
    ("drums-gm", include_str!("../profiles/presets/drums-gm.json")),
    ("mixer-cc", include_str!("../profiles/presets/mixer-cc.json")),
    ("shortcuts", include_str!("../profiles/presets/shortcuts.json")),
];

#[derive(Debug, Serialize)]
pub struct PresetSummary {
    pub id: &'static str,
    pub name: String,
}

pub fn presets() -> Vec<PresetSummary> {
    PRESETS
        .iter()
        .filter_map(|(id, json)| parse_profile(json).ok().map(|p| PresetSummary { id, name: p.name }))
        .collect()
}

const PAD_COUNT: u8 = 16;
/// Longest label of an action, in characters.
const MAX_LABEL: usize = 60;
const KNOB_COUNT: u8 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub ports: Ports,
    #[serde(default, skip_serializing_if = "is_default")]
    pub start_bank: usize,
    #[serde(default, skip_serializing_if = "is_default")]
    pub display: DisplaySettings,
    #[serde(default, skip_serializing_if = "is_default")]
    pub velocity: VelocitySettings,
    #[serde(default, skip_serializing_if = "is_default")]
    pub repeat: RepeatSettings,
    pub banks: Vec<Bank>,
}

/// How the velocity of a pad hit is shaped before it is sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VelocitySettings {
    #[serde(default, skip_serializing_if = "is_default")]
    pub curve: VelocityCurve,
    /// Velocity sent for the softest hit.
    #[serde(default = "min_velocity", skip_serializing_if = "is_min_velocity")]
    pub min: u8,
    /// Velocity sent for the hardest hit.
    #[serde(default = "max_value", skip_serializing_if = "is_max_value")]
    pub max: u8,
}

impl Default for VelocitySettings {
    fn default() -> Self {
        Self {
            curve: VelocityCurve::Linear,
            min: min_velocity(),
            max: max_value(),
        }
    }
}

impl VelocitySettings {
    /// Velocity to send for a hit of `raw` (1 to 127).
    pub fn apply(&self, raw: u8) -> u8 {
        let x = f32::from(raw.clamp(1, 127) - 1) / 126.0;
        let shaped = x.powf(self.curve.exponent());
        let (min, max) = (f32::from(self.min), f32::from(self.max));
        (min + (max - min) * shaped).round().clamp(1.0, 127.0) as u8
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VelocityCurve {
    /// Light hits come out louder.
    Soft,
    #[default]
    Linear,
    /// It takes a hard hit to reach high velocities.
    Hard,
}

impl VelocityCurve {
    fn exponent(self) -> f32 {
        match self {
            VelocityCurve::Soft => 0.5,
            VelocityCurve::Linear => 1.0,
            VelocityCurve::Hard => 2.0,
        }
    }
}

fn min_velocity() -> u8 {
    1
}

fn is_min_velocity(value: &u8) -> bool {
    *value == min_velocity()
}

pub const DISPLAY_LENGTH: usize = 3;
const DISPLAY_DURATION_MS: std::ops::RangeInclusive<u32> = 200..=10_000;

/// What the 3-character LED display shows besides the bank.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplaySettings {
    /// Show the value sent when a knob turns.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub knobs: bool,
    /// Show the value sent when the encoder turns.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub encoder: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub pads: PadDisplay,
    /// How long a value stays before the bank comes back, in milliseconds.
    #[serde(
        default = "default_display_duration",
        skip_serializing_if = "is_default_display_duration"
    )]
    pub duration: u32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            knobs: true,
            encoder: true,
            pads: PadDisplay::None,
            duration: default_display_duration(),
        }
    }
}

/// What a pad hit shows on the display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PadDisplay {
    #[default]
    None,
    Note,
    Velocity,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn default_display_duration() -> u32 {
    1000
}

fn is_default_display_duration(value: &u32) -> bool {
    *value == default_display_duration()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ports {
    #[serde(default = "default_input_port")]
    pub input: String,
    #[serde(default = "default_output_port")]
    pub output: String,
}

impl Default for Ports {
    fn default() -> Self {
        Self {
            input: default_input_port(),
            output: default_output_port(),
        }
    }
}

fn default_input_port() -> String {
    "padKONTROL".into()
}

fn default_output_port() -> String {
    "totalKONTROL".into()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bank {
    pub name: String,
    /// Text shown on the display for this bank. Defaults to the start of
    /// the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pads: BTreeMap<u8, ButtonAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub buttons: BTreeMap<ButtonId, ButtonAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub knobs: BTreeMap<u8, KnobAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xy: Option<XyAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<EncoderAction>,
    /// Action when a finger lands on / leaves the XY pad, like a button
    /// without an LED.
    #[serde(rename = "xyTouch", default, skip_serializing_if = "Option::is_none")]
    pub xy_touch: Option<ButtonAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonId {
    Scene,
    Message,
    Setting,
    NoteCc,
    MidiCh,
    SwType,
    RelVal,
    Velocity,
    Port,
    FixedVelocity,
    ProgChange,
    X,
    Y,
    Knob1Assign,
    Knob2Assign,
    Pedal,
    Roll,
    Flam,
    Hold,
}

/// Action of a pad or a button.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
/// Every variant takes a `label`: what the control does, in words, shown in
/// the editor and the live view.
pub enum ButtonAction {
    Note {
        channel: u8,
        note: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        velocity: Option<Velocity>,
        #[serde(default, skip_serializing_if = "is_default")]
        mode: Mode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        led: Option<Led>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    Cc {
        channel: u8,
        cc: u8,
        #[serde(default = "max_value", skip_serializing_if = "is_max_value")]
        on_value: u8,
        #[serde(default, skip_serializing_if = "is_default")]
        off_value: u8,
        #[serde(default, skip_serializing_if = "is_default")]
        mode: Mode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        led: Option<Led>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    ProgramChange {
        channel: u8,
        program: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        led: Option<Led>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    SwitchBank {
        bank: usize,
        #[serde(default, skip_serializing_if = "is_default")]
        momentary: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Makes another profile, by its file name, the active one.
    SwitchProfile {
        profile: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// OSC message to the connection's OSC destination: `args` on press
    /// (or when a toggle turns on), `release_args` on release (or when it
    /// turns off), if set.
    Osc {
        address: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<OscValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        release_args: Option<Vec<OscValue>>,
        #[serde(default, skip_serializing_if = "is_default")]
        mode: Mode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        led: Option<Led>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Types a keyboard shortcut such as "ctrl+s" on press or, with `hold`,
    /// keeps its keys down while the control is held.
    Key {
        keys: String,
        #[serde(default, skip_serializing_if = "is_default")]
        hold: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        led: Option<Led>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Turns roll on or off: held pads repeat their note. The LED shows it.
    Roll {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Turns flam on or off: each hit plays a softer grace note first. The
    /// LED shows it.
    Flam {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

/// Roll and flam settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepeatSettings {
    /// Tempo used when the DAW sends no MIDI clock.
    #[serde(default = "default_bpm", skip_serializing_if = "is_default_bpm")]
    pub bpm: f32,
    /// Roll speed, in notes per 4/4 bar (one of `ROLL_DIVISIONS`).
    #[serde(default = "default_roll", skip_serializing_if = "is_default_roll")]
    pub roll: u8,
    /// Gap between the grace note and the main note, in milliseconds.
    #[serde(default = "default_flam", skip_serializing_if = "is_default_flam")]
    pub flam: u16,
    /// While roll or flam is on, the XY pad's X axis sets the roll speed or
    /// the flam gap, like on the hardware, instead of its usual action.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub xy_speed: bool,
}

impl Default for RepeatSettings {
    fn default() -> Self {
        Self {
            bpm: default_bpm(),
            roll: default_roll(),
            flam: default_flam(),
            xy_speed: true,
        }
    }
}

pub const ROLL_DIVISIONS: [u8; 7] = [4, 6, 8, 12, 16, 24, 32];
pub const BPM_RANGE: std::ops::RangeInclusive<f32> = 20.0..=300.0;
pub const FLAM_RANGE_MS: std::ops::RangeInclusive<u16> = 5..=200;

fn default_bpm() -> f32 {
    120.0
}

fn is_default_bpm(value: &f32) -> bool {
    *value == default_bpm()
}

fn default_roll() -> u8 {
    16
}

fn is_default_roll(value: &u8) -> bool {
    *value == default_roll()
}

fn default_flam() -> u16 {
    30
}

fn is_default_flam(value: &u16) -> bool {
    *value == default_flam()
}

/// OSC argument in a profile. The string `"$velocity"` stands for the
/// velocity of the hit (127 for a button).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OscValue {
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
}

pub const OSC_VELOCITY: &str = "$velocity";

/// `"pad"`: velocity of the hit; otherwise a fixed value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Velocity {
    Pad(PadVelocity),
    Fixed(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PadVelocity {
    Pad,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Momentary,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Led {
    None,
    Press,
    Toggle,
    Follow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
/// Every variant takes a `label`: what the control does, in words, shown in
/// the editor and the live view.
pub enum KnobAction {
    Cc {
        channel: u8,
        cc: u8,
        #[serde(default, skip_serializing_if = "is_default")]
        min: u8,
        #[serde(default = "max_value", skip_serializing_if = "is_max_value")]
        max: u8,
        /// See `KnobAction::pickup`.
        #[serde(default, skip_serializing_if = "is_default")]
        pickup: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Sends the knob position scaled from `min` to `max`, as a float when
    /// `float` is set, else rounded to an integer.
    Osc {
        address: String,
        #[serde(default, skip_serializing_if = "is_default")]
        min: f32,
        #[serde(default = "max_float", skip_serializing_if = "is_max_float")]
        max: f32,
        #[serde(default, skip_serializing_if = "is_default")]
        float: bool,
        #[serde(default, skip_serializing_if = "is_default")]
        pickup: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

impl ButtonAction {
    pub fn label(&self) -> Option<&str> {
        match self {
            ButtonAction::Note { label, .. }
            | ButtonAction::Cc { label, .. }
            | ButtonAction::ProgramChange { label, .. }
            | ButtonAction::SwitchBank { label, .. }
            | ButtonAction::SwitchProfile { label, .. }
            | ButtonAction::Osc { label, .. }
            | ButtonAction::Key { label, .. }
            | ButtonAction::Roll { label }
            | ButtonAction::Flam { label } => label.as_deref(),
        }
    }
}

impl KnobAction {
    pub fn label(&self) -> Option<&str> {
        match self {
            KnobAction::Cc { label, .. } | KnobAction::Osc { label, .. } => label.as_deref(),
        }
    }

    /// After a bank change, the knob sends nothing until it reaches the value
    /// it had in this bank, so the target does not jump.
    pub fn pickup(&self) -> bool {
        match self {
            KnobAction::Cc { pickup, .. } | KnobAction::Osc { pickup, .. } => *pickup,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
/// Every variant takes a `label`: what the control does, in words, shown in
/// the editor and the live view.
pub enum XyAction {
    Xy {
        channel: u8,
        cc_x: u8,
        cc_y: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Sends x and y (0 to 127) as two integers.
    Osc {
        address: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
/// Every variant takes a `label`: what the control does, in words, shown in
/// the editor and the live view.
pub enum EncoderAction {
    Cc {
        channel: u8,
        cc: u8,
        #[serde(default, skip_serializing_if = "is_default")]
        encoding: Encoding,
        /// See `EncoderAction::acceleration`.
        #[serde(default, skip_serializing_if = "is_default")]
        acceleration: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Sends the step (relative) or the position (absolute) as an integer.
    Osc {
        address: String,
        #[serde(default, skip_serializing_if = "is_default")]
        encoding: Encoding,
        #[serde(default, skip_serializing_if = "is_default")]
        acceleration: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Types a keyboard shortcut for each step to the left or to the right.
    Key {
        left: String,
        right: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

impl XyAction {
    pub fn label(&self) -> Option<&str> {
        match self {
            XyAction::Xy { label, .. } | XyAction::Osc { label, .. } => label.as_deref(),
        }
    }
}

impl EncoderAction {
    pub fn label(&self) -> Option<&str> {
        match self {
            EncoderAction::Cc { label, .. } | EncoderAction::Osc { label, .. } | EncoderAction::Key { label, .. } => {
                label.as_deref()
            }
        }
    }

    pub fn encoding(&self) -> Encoding {
        match self {
            EncoderAction::Cc { encoding, .. } | EncoderAction::Osc { encoding, .. } => *encoding,
            EncoderAction::Key { .. } => Encoding::Relative,
        }
    }

    /// Turned fast, the encoder moves several steps per detent.
    pub fn acceleration(&self) -> bool {
        match self {
            EncoderAction::Cc { acceleration, .. } | EncoderAction::Osc { acceleration, .. } => *acceleration,
            EncoderAction::Key { .. } => false,
        }
    }
}

fn max_float() -> f32 {
    127.0
}

fn is_max_float(value: &f32) -> bool {
    *value == max_float()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Encoding {
    #[default]
    Relative,
    Absolute,
}

fn max_value() -> u8 {
    127
}

fn is_max_value(value: &u8) -> bool {
    *value == max_value()
}

/// Leaves default values out of saved profiles, so they stay as short as
/// hand-written ones.
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// Reads a profile from JSON and validates it. Errors are human-readable
/// messages, prefixed with the path of the offending value.
pub fn parse_profile(json: &str) -> Result<Profile, Vec<String>> {
    let profile: Profile = serde_json::from_str(json).map_err(|e| {
        let (line, column) = (e.line(), e.column());
        vec![t!("core.validation.invalidJson", line, column, error = e)]
    })?;
    checked(profile)
}

/// Same as [`parse_profile`], from a JSON value sent by the editor.
pub fn parse_value(value: serde_json::Value) -> Result<Profile, Vec<String>> {
    let profile: Profile =
        serde_json::from_value(value).map_err(|e| vec![t!("core.validation.invalidProfile", error = e)])?;
    checked(profile)
}

fn checked(profile: Profile) -> Result<Profile, Vec<String>> {
    let errors = validate(&profile);
    if errors.is_empty() { Ok(profile) } else { Err(errors) }
}

/// File name derived from a profile name: lowercase ASCII letters and digits,
/// other characters folded into dashes.
fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "profile".into()
    } else if is_reserved_on_windows(slug) {
        format!("profile-{slug}")
    } else {
        slug.into()
    }
}

/// Device names Windows reserves: `con.json` cannot be created.
fn is_reserved_on_windows(name: &str) -> bool {
    matches!(name, "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|n| n.len() == 1 && n.chars().all(|c| c.is_ascii_digit()))
        })
}

pub fn validate(profile: &Profile) -> Vec<String> {
    let mut v = Validator::default();

    if profile.version != FORMAT_VERSION {
        let version = profile.version;
        v.error(
            "version",
            t!("core.validation.unsupportedVersion", version, expected = FORMAT_VERSION),
        );
    }
    if profile.name.trim().is_empty() {
        v.error("name", empty_name());
    }
    let bank_count = profile.banks.len();
    if bank_count == 0 {
        v.error("banks", t!("core.validation.noBank"));
    } else if profile.start_bank >= bank_count {
        v.error("startBank", missing_bank(profile.start_bank));
    }
    let velocity = &profile.velocity;
    for (field, value) in [("min", velocity.min), ("max", velocity.max)] {
        if !(1..=127).contains(&value) {
            v.error(
                format!("velocity.{field}"),
                t!("core.validation.invalidVelocity", value),
            );
        }
    }
    if velocity.min > velocity.max {
        v.error("velocity", t!("core.validation.minAboveMax"));
    }
    let repeat = &profile.repeat;
    if !BPM_RANGE.contains(&repeat.bpm) {
        let (bpm, min, max) = (repeat.bpm, BPM_RANGE.start(), BPM_RANGE.end());
        v.error("repeat.bpm", t!("core.validation.invalidBpm", bpm, min, max));
    }
    if !ROLL_DIVISIONS.contains(&repeat.roll) {
        let (roll, allowed) = (repeat.roll, ROLL_DIVISIONS.map(|d| d.to_string()).join(", "));
        v.error("repeat.roll", t!("core.validation.invalidRoll", roll, allowed));
    }
    if !FLAM_RANGE_MS.contains(&repeat.flam) {
        let (flam, min, max) = (repeat.flam, FLAM_RANGE_MS.start(), FLAM_RANGE_MS.end());
        v.error("repeat.flam", t!("core.validation.invalidFlam", flam, min, max));
    }
    if !DISPLAY_DURATION_MS.contains(&profile.display.duration) {
        let (duration, min, max) = (
            profile.display.duration,
            DISPLAY_DURATION_MS.start(),
            DISPLAY_DURATION_MS.end(),
        );
        v.error(
            "display.duration",
            t!("core.validation.invalidDuration", duration, min, max),
        );
    }

    for (b, bank) in profile.banks.iter().enumerate() {
        let path = format!("banks[{b}]");
        if bank.name.trim().is_empty() {
            v.error(format!("{path}.name"), empty_name());
        }
        if let Some(text) = &bank.display
            && (text.chars().count() > DISPLAY_LENGTH || !text.chars().all(|c| c.is_ascii() && !c.is_ascii_control()))
        {
            v.error(
                format!("{path}.display"),
                t!("core.validation.invalidDisplayText", length = DISPLAY_LENGTH, text),
            );
        }
        for (pad, action) in &bank.pads {
            let p = format!("{path}.pads.{pad}");
            if !(1..=PAD_COUNT).contains(pad) {
                v.error(&p, t!("core.validation.unknownPad", count = PAD_COUNT));
            }
            v.button_action(&p, action, true, bank_count);
        }
        for (id, action) in &bank.buttons {
            let p = format!(
                "{path}.buttons.{}",
                serde_json::to_string(id).unwrap().trim_matches('"')
            );
            v.button_action(&p, action, false, bank_count);
        }
        if let Some(action) = &bank.xy_touch {
            v.button_action(&format!("{path}.xyTouch"), action, false, bank_count);
        }
        for (knob, action) in &bank.knobs {
            let p = format!("{path}.knobs.{knob}");
            if !(1..=KNOB_COUNT).contains(knob) {
                v.error(&p, t!("core.validation.unknownKnob", count = KNOB_COUNT));
            }
            v.label(&p, action.label());
            match action {
                KnobAction::Cc {
                    channel, cc, min, max, ..
                } => {
                    v.channel(&p, *channel);
                    v.midi_value(&p, "cc", *cc);
                    v.midi_value(&p, "min", *min);
                    v.midi_value(&p, "max", *max);
                    if min > max {
                        v.error(&p, t!("core.validation.minAboveMax"));
                    }
                }
                // An OSC range may run backwards, to invert the knob.
                KnobAction::Osc { address, min, max, .. } => {
                    v.osc_address(&p, address);
                    if !min.is_finite() || !max.is_finite() {
                        v.error(&p, t!("core.validation.invalidRange"));
                    }
                }
            }
        }
        if let Some(xy) = &bank.xy {
            v.label(&format!("{path}.xy"), xy.label());
        }
        if let Some(encoder) = &bank.encoder {
            v.label(&format!("{path}.encoder"), encoder.label());
        }
        match &bank.xy {
            Some(XyAction::Xy {
                channel, cc_x, cc_y, ..
            }) => {
                let p = format!("{path}.xy");
                v.channel(&p, *channel);
                v.midi_value(&p, "ccX", *cc_x);
                v.midi_value(&p, "ccY", *cc_y);
            }
            Some(XyAction::Osc { address, .. }) => v.osc_address(&format!("{path}.xy"), address),
            None => {}
        }
        match &bank.encoder {
            Some(EncoderAction::Cc { channel, cc, .. }) => {
                let p = format!("{path}.encoder");
                v.channel(&p, *channel);
                v.midi_value(&p, "cc", *cc);
            }
            Some(EncoderAction::Osc { address, .. }) => v.osc_address(&format!("{path}.encoder"), address),
            Some(EncoderAction::Key { left, right, .. }) => {
                v.keys(&format!("{path}.encoder.left"), left);
                v.keys(&format!("{path}.encoder.right"), right);
            }
            None => {}
        }
    }

    v.errors
}

fn empty_name() -> String {
    t!("core.validation.emptyName")
}

fn missing_bank(bank: usize) -> String {
    t!("core.validation.missingBank", bank)
}

#[derive(Default)]
struct Validator {
    errors: Vec<String>,
}

impl Validator {
    fn error(&mut self, path: impl AsRef<str>, message: impl AsRef<str>) {
        self.errors.push(format!("{} : {}", path.as_ref(), message.as_ref()));
    }

    fn channel(&mut self, path: &str, channel: u8) {
        if !(1..=16).contains(&channel) {
            self.error(format!("{path}.channel"), t!("core.validation.invalidChannel", channel));
        }
    }

    fn midi_value(&mut self, path: &str, field: &str, value: u8) {
        if value > 127 {
            self.error(format!("{path}.{field}"), t!("core.validation.invalidValue", value));
        }
    }

    /// A label stays short: it shows on a pad of the front panel.
    fn label(&mut self, path: &str, label: Option<&str>) {
        if let Some(label) = label
            && label.chars().count() > MAX_LABEL
        {
            self.error(
                format!("{path}.label"),
                t!("core.validation.longLabel", max = MAX_LABEL),
            );
        }
    }

    fn button_action(&mut self, path: &str, action: &ButtonAction, is_pad: bool, bank_count: usize) {
        self.label(path, action.label());
        match action {
            ButtonAction::Note {
                channel,
                note,
                velocity,
                ..
            } => {
                self.channel(path, *channel);
                self.midi_value(path, "note", *note);
                match velocity {
                    Some(Velocity::Fixed(value)) if !(1..=127).contains(value) => {
                        self.error(format!("{path}.velocity"), t!("core.validation.invalidVelocity", value));
                    }
                    Some(Velocity::Pad(_)) if !is_pad => {
                        self.error(format!("{path}.velocity"), t!("core.validation.padVelocityOnPadOnly"));
                    }
                    _ => {}
                }
            }
            ButtonAction::Cc {
                channel,
                cc,
                on_value,
                off_value,
                ..
            } => {
                self.channel(path, *channel);
                self.midi_value(path, "cc", *cc);
                self.midi_value(path, "onValue", *on_value);
                self.midi_value(path, "offValue", *off_value);
            }
            ButtonAction::ProgramChange { channel, program, .. } => {
                self.channel(path, *channel);
                self.midi_value(path, "program", *program);
            }
            ButtonAction::SwitchBank { bank, .. } => {
                if *bank >= bank_count {
                    self.error(format!("{path}.bank"), missing_bank(*bank));
                }
            }
            ButtonAction::Osc { address, .. } => self.osc_address(path, address),
            ButtonAction::Key { keys, .. } => self.keys(&format!("{path}.keys"), keys),
            ButtonAction::SwitchProfile { profile, .. } => {
                // A file name from the profiles folder.
                if !profile.ends_with(".json") || profile.contains(['/', '\\']) || profile == SCHEMA_FILE {
                    self.error(
                        format!("{path}.profile"),
                        t!("core.validation.invalidProfileFile", profile),
                    );
                }
            }
            ButtonAction::Roll { .. } | ButtonAction::Flam { .. } => {}
        }
    }

    fn keys(&mut self, path: &str, keys: &str) {
        let message = match crate::keys::parse(keys) {
            Ok(_) => return,
            Err(KeysError::Empty) => t!("core.validation.emptyKeys"),
            Err(KeysError::Unknown(key)) => t!("core.validation.unknownKey", key, keys),
            Err(KeysError::SeveralKeys) => t!("core.validation.severalKeys", keys),
        };
        self.error(path, message);
    }

    /// An OSC address starts with `/` and has no spaces.
    fn osc_address(&mut self, path: &str, address: &str) {
        if !address.starts_with('/') || address.chars().any(char::is_whitespace) {
            self.error(
                format!("{path}.address"),
                t!("core.validation.invalidOscAddress", address),
            );
        }
    }
}

/// Writes next to the file, then swaps it in: a crash midway leaves the
/// previous version intact. The temporary name is not listed as a profile.
fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

/// Summary of a profile file, for the list shown in the UI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub file: String,
    pub name: Option<String>,
    pub bank_count: usize,
    pub errors: Vec<String>,
    /// Ports the profile asks for, when they differ from the default ones:
    /// the connection card preselects them when the profile is picked.
    pub ports: PortHints,
}

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct PortHints {
    pub input: Option<String>,
    pub output: Option<String>,
}

impl From<&Ports> for PortHints {
    fn from(ports: &Ports) -> Self {
        let defaults = Ports::default();
        Self {
            input: (ports.input != defaults.input).then(|| ports.input.clone()),
            output: (ports.output != defaults.output).then(|| ports.output.clone()),
        }
    }
}

/// Profile file names in a directory, sorted. A missing directory has none.
fn profile_files_in(dir: &Path) -> std::io::Result<Vec<String>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut files: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json") && name != SCHEMA_FILE)
        .collect();
    files.sort();
    Ok(files)
}

/// The user's profile directory.
pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    /// Creates the directory if needed, updates the schema and installs the
    /// default profile if no profile exists yet.
    pub fn open(dir: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(SCHEMA_FILE), SCHEMA_JSON)?;
        let store = Self { dir };
        if store.profile_files()?.is_empty() {
            fs::write(store.dir.join(DEFAULT_PROFILE_FILE), default_profile_json())?;
        }
        Ok(store)
    }

    /// Like [`ProfileStore::open`], but first copies the profiles of an older
    /// profile directory when this one has none yet (the app identifier, and
    /// so its config directory, changed). The old files are left in place.
    /// Returns the store and the number of profiles copied.
    pub fn open_migrating(dir: PathBuf, legacy: &Path) -> std::io::Result<(Self, usize)> {
        let mut copied = 0;
        if profile_files_in(&dir)?.is_empty() && legacy != dir {
            let old = profile_files_in(legacy)?;
            if !old.is_empty() {
                fs::create_dir_all(&dir)?;
                for file in old {
                    fs::copy(legacy.join(&file), dir.join(&file))?;
                    copied += 1;
                }
            }
        }
        Ok((Self::open(dir)?, copied))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn profile_files(&self) -> std::io::Result<Vec<String>> {
        profile_files_in(&self.dir)
    }

    pub fn list(&self) -> Result<Vec<ProfileSummary>, String> {
        let files = self.profile_files().map_err(|e| e.to_string())?;
        Ok(files
            .into_iter()
            .map(|file| match self.load(&file) {
                Ok(p) => ProfileSummary {
                    file,
                    ports: PortHints::from(&p.ports),
                    name: Some(p.name),
                    bank_count: p.banks.len(),
                    errors: vec![],
                },
                Err(errors) => ProfileSummary {
                    file,
                    ports: PortHints::default(),
                    name: None,
                    bank_count: 0,
                    errors,
                },
            })
            .collect())
    }

    pub fn load(&self, file: &str) -> Result<Profile, Vec<String>> {
        let path = self.resolve(file).map_err(|e| vec![e])?;
        let json = fs::read_to_string(&path).map_err(|e| vec![format!("{file} : {e}")])?;
        parse_profile(&json)
    }

    /// Validates and writes a profile. Nothing is written if it is invalid.
    pub fn save(&self, file: &str, profile: Profile) -> Result<(), Vec<String>> {
        let path = self.resolve(file).map_err(|e| vec![e])?;
        let json = to_saved_json(checked(profile)?).map_err(|e| vec![e.to_string()])?;
        write_atomically(&path, &json).map_err(|e| vec![format!("{file} : {e}")])
    }

    /// Creates an empty profile with one bank and returns its file name.
    pub fn create(&self, name: &str) -> Result<String, String> {
        let profile = Profile {
            schema: None,
            version: FORMAT_VERSION,
            name: name.trim().to_string(),
            ports: Ports::default(),
            start_bank: 0,
            display: DisplaySettings::default(),
            velocity: VelocitySettings::default(),
            repeat: RepeatSettings::default(),
            banks: vec![Bank {
                name: t!("core.profiles.firstBank"),
                display: None,
                pads: BTreeMap::new(),
                buttons: BTreeMap::new(),
                knobs: BTreeMap::new(),
                xy: None,
                xy_touch: None,
                encoder: None,
            }],
        };
        self.add(profile)
    }

    /// Creates a profile with these banks and returns its file name.
    pub fn create_with_banks(&self, name: &str, banks: Vec<Bank>) -> Result<String, String> {
        let mut profile = parse_profile(DEFAULT_PROFILE_JSON).map_err(|errors| errors.join("\n"))?;
        profile.name = name.trim().to_string();
        profile.start_bank = 0;
        profile.banks = banks;
        self.add(profile)
    }

    /// Creates a profile from a bundled preset and returns its file name.
    pub fn create_from_preset(&self, id: &str) -> Result<String, String> {
        let (_, json) = PRESETS
            .iter()
            .find(|(preset, _)| *preset == id)
            .ok_or_else(|| t!("core.profiles.noSuchPreset", id))?;
        let mut profile = parse_profile(json).map_err(|errors| errors.join("\n"))?;
        profile.schema = None;
        self.add(profile)
    }

    /// Adds a profile from a file elsewhere, checked first; returns its new
    /// file name in the store.
    pub fn import(&self, path: &Path) -> Result<String, String> {
        let json = fs::read_to_string(path).map_err(|e| format!("{} : {e}", path.display()))?;
        let mut profile = parse_profile(&json).map_err(|errors| errors.join("\n"))?;
        profile.schema = None;
        self.add(profile)
    }

    /// Copies a profile out of the store.
    pub fn export(&self, file: &str, path: &Path) -> Result<(), String> {
        let profile = self.load(file).map_err(|errors| errors.join("\n"))?;
        let mut profile = profile;
        // The schema stays behind: point editors at none rather than a missing file.
        profile.schema = None;
        let json = to_json(&profile).map_err(|e| e.to_string())?;
        write_atomically(path, &json).map_err(|e| format!("{} : {e}", path.display()))
    }

    /// Copies a profile under a new name and returns the new file name.
    pub fn duplicate(&self, file: &str) -> Result<String, String> {
        let mut profile = self.load(file).map_err(|errors| errors.join("\n"))?;
        let name = profile.name;
        profile.name = t!("core.profiles.copyName", name);
        self.add(profile)
    }

    pub fn delete(&self, file: &str) -> Result<(), String> {
        let path = self.resolve(file)?;
        fs::remove_file(path).map_err(|e| format!("{file} : {e}"))
    }

    pub fn add(&self, profile: Profile) -> Result<String, String> {
        let file = self.free_file_name(&slug(&profile.name)).map_err(|e| e.to_string())?;
        self.save(&file, profile).map_err(|errors| errors.join("\n"))?;
        Ok(file)
    }

    /// `<base>.json`, or `<base>-2.json`, `<base>-3.json`… if it is taken.
    fn free_file_name(&self, base: &str) -> std::io::Result<String> {
        let taken = self.profile_files()?;
        let file = (1..)
            .map(|n| {
                if n == 1 {
                    format!("{base}.json")
                } else {
                    format!("{base}-{n}.json")
                }
            })
            .find(|f| !taken.contains(f) && f != SCHEMA_FILE)
            .expect("an unused name always exists");
        Ok(file)
    }

    /// Only accepts a plain `.json` file name, without a path.
    fn resolve(&self, file: &str) -> Result<PathBuf, String> {
        let valid = file.ends_with(".json")
            && file != SCHEMA_FILE
            && file
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && !file.starts_with('.')
            // Windows opens the device, not a file, for CON.json and the like.
            && !file
                .split('.')
                .next()
                .is_some_and(|stem| is_reserved_on_windows(&stem.to_ascii_lowercase()));
        if valid {
            Ok(self.dir.join(file))
        } else {
            Err(t!("core.profiles.invalidFileName", file))
        }
    }
}

/// JSON written to a profile file: pretty-printed, with the `$schema` line.
fn to_saved_json(mut profile: Profile) -> serde_json::Result<String> {
    profile.schema = Some(format!("./{SCHEMA_FILE}"));
    to_json(&profile)
}

/// Pretty-printed, as it is.
fn to_json(profile: &Profile) -> serde_json::Result<String> {
    let mut json = serde_json::to_string_pretty(profile)?;
    json.push('\n');
    Ok(json)
}

/// The bundled default profile, with its names in the current language.
fn default_profile() -> Profile {
    let mut profile: Profile = serde_json::from_str(DEFAULT_PROFILE_JSON).expect("bundled default profile is valid");
    profile.name = t!("core.profiles.defaultName");
    // The second bank, "Clips", reads the same in every language.
    if let Some(drums) = profile.banks.first_mut() {
        drums.name = t!("core.profiles.drumsBank");
    }
    profile
}

fn default_profile_json() -> String {
    to_saved_json(default_profile()).expect("a profile always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{self, Lang};

    fn minimal(bank: &str) -> String {
        format!(r#"{{ "version": 1, "name": "Test", "banks": [{bank}] }}"#)
    }

    #[test]
    fn default_profile_is_valid() {
        let profile = parse_profile(DEFAULT_PROFILE_JSON).expect("invalid default profile");
        assert_eq!(profile.banks.len(), 2);
        assert_eq!(profile.banks[0].pads.len(), 16);
    }

    fn schema_errors(json: &str) -> Vec<String> {
        let schema: serde_json::Value = serde_json::from_str(SCHEMA_JSON).unwrap();
        let profile: serde_json::Value = serde_json::from_str(json).unwrap();
        let validator = jsonschema::validator_for(&schema).expect("invalid schema");
        validator.iter_errors(&profile).map(|e| e.to_string()).collect()
    }

    #[test]
    fn default_profile_matches_schema() {
        let errors = schema_errors(DEFAULT_PROFILE_JSON);
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[test]
    fn osc_actions_parse_and_match_schema() {
        let json = minimal(
            r#"{ "name": "A",
                 "pads": { "1": { "type": "osc", "address": "/live/clip", "args": [0, 1.5, true, "$velocity"],
                                  "releaseArgs": [], "mode": "toggle", "led": "toggle" } },
                 "knobs": { "1": { "type": "osc", "address": "/pan", "min": -1, "max": 1, "float": true } },
                 "xy": { "type": "osc", "address": "/xy" },
                 "xyTouch": { "type": "note", "channel": 1, "note": 60 },
                 "encoder": { "type": "osc", "address": "/scroll", "encoding": "absolute" } }"#,
        );
        let errors = schema_errors(&json);
        assert!(errors.is_empty(), "{errors:#?}");
        let profile = parse_profile(&json).unwrap();
        let ButtonAction::Osc { args, .. } = &profile.banks[0].pads[&1] else {
            panic!("osc action expected")
        };
        assert_eq!(
            args,
            &vec![
                OscValue::Int(0),
                OscValue::Float(1.5),
                OscValue::Bool(true),
                OscValue::Str("$velocity".into())
            ]
        );
        // Saved files round-trip.
        let saved = serde_json::to_string(&profile).unwrap();
        assert_eq!(parse_profile(&saved).unwrap(), profile);
    }

    #[test]
    fn schema_rejects_what_the_parser_rejects() {
        let invalid = [
            minimal(r#"{ "name": "A", "colour": "red" }"#),
            minimal(r#"{ "name": "A", "buttons": { "shift": { "type": "switchBank", "bank": 0 } } }"#),
            minimal(r#"{ "name": "A", "pads": { "17": { "type": "note", "channel": 1, "note": 36 } } }"#),
            minimal(r#"{ "name": "A", "pads": { "1": { "type": "note", "channel": 0, "note": 36 } } }"#),
            minimal(
                r#"{ "name": "A", "buttons": { "x": { "type": "note", "channel": 1, "note": 1, "velocity": "pad" } } }"#,
            ),
            minimal(r#"{ "name": "A", "knobs": { "3": { "type": "cc", "channel": 1, "cc": 1 } } }"#),
            minimal(r#"{ "name": "A", "display": "TROP" }"#),
            minimal(r#"{ "name": "A", "display": "été" }"#),
            minimal(r#"{ "name": "A", "pads": { "1": { "type": "osc", "address": "live/clip" } } }"#),
            minimal(r#"{ "name": "A", "xy": { "type": "osc", "address": "/x y" } }"#),
            minimal(r#"{ "name": "A", "xyTouch": { "type": "note", "channel": 1, "note": 60, "velocity": "pad" } }"#),
            minimal(r#"{ "name": "A", "knobs": { "1": { "type": "osc", "address": "/v", "channel": 1 } } }"#),
            r#"{ "version": 1, "name": "T", "velocity": { "min": 0 }, "banks": [{ "name": "A" }] }"#.to_string(),
            r#"{ "version": 1, "name": "T", "velocity": { "curve": "exp" }, "banks": [{ "name": "A" }] }"#.to_string(),
            minimal(r#"{ "name": "A", "buttons": { "roll": { "type": "roll", "channel": 1 } } }"#),
            r#"{ "version": 1, "name": "T", "repeat": { "bpm": 10 }, "banks": [{ "name": "A" }] }"#.to_string(),
            r#"{ "version": 1, "name": "T", "repeat": { "roll": 5 }, "banks": [{ "name": "A" }] }"#.to_string(),
            r#"{ "version": 1, "name": "T", "repeat": { "flam": 300 }, "banks": [{ "name": "A" }] }"#.to_string(),
            r#"{ "version": 1, "name": "T", "repeat": { "swing": 1 }, "banks": [{ "name": "A" }] }"#.to_string(),
            minimal(r#"{ "name": "A", "pads": { "1": { "type": "key", "keys": "ctrl+s", "channel": 1 } } }"#),
            minimal(r#"{ "name": "A", "encoder": { "type": "key", "left": "left" } }"#),
            minimal(r#"{ "name": "A", "pads": { "1": { "type": "switchProfile", "profile": "../live.json" } } }"#),
            minimal(r#"{ "name": "A", "pads": { "1": { "type": "switchProfile", "profile": "live" } } }"#),
        ];
        for json in invalid {
            assert!(!schema_errors(&json).is_empty(), "the schema accepts: {json}");
            assert!(parse_profile(&json).is_err(), "the parser accepts: {json}");
        }
        // Beyond what the schema can express.
        let swapped =
            r#"{ "version": 1, "name": "T", "velocity": { "min": 100, "max": 20 }, "banks": [{ "name": "A" }] }"#;
        assert!(
            parse_profile(swapped)
                .unwrap_err()
                .iter()
                .any(|e| e.starts_with("velocity"))
        );
    }

    #[test]
    fn roll_and_flam_parse_and_match_schema() {
        let json = r#"{ "version": 1, "name": "T", "repeat": { "bpm": 96.5, "roll": 12, "flam": 45, "xySpeed": false },
            "banks": [{ "name": "A", "pads": { "16": { "type": "flam" } },
                        "buttons": { "roll": { "type": "roll" }, "flam": { "type": "flam" } } }] }"#;
        assert_eq!(schema_errors(json), Vec::<String>::new());
        let profile = parse_profile(json).unwrap();
        assert_eq!(
            profile.repeat,
            RepeatSettings {
                bpm: 96.5,
                roll: 12,
                flam: 45,
                xy_speed: false
            }
        );
        assert_eq!(
            profile.banks[0].buttons.get(&ButtonId::Roll),
            Some(&ButtonAction::Roll { label: None })
        );
        let saved = serde_json::to_string(&profile).unwrap();
        assert_eq!(parse_profile(&saved).unwrap(), profile);
        // Defaults are left out when saving.
        let plain = parse_profile(&minimal(r#"{ "name": "A" }"#)).unwrap();
        assert!(!serde_json::to_string(&plain).unwrap().contains("repeat"));
    }

    #[test]
    fn labels_parse_and_match_schema() {
        let json = minimal(
            r#"{ "name": "A", "pads": { "1": { "type": "note", "channel": 1, "note": 36, "label": "Kick" },
                                       "2": { "type": "roll", "label": "Roll" } },
                             "knobs": { "1": { "type": "cc", "channel": 1, "cc": 7, "label": "Volume" } },
                             "xy": { "type": "xy", "channel": 1, "ccX": 1, "ccY": 2, "label": "Filter" },
                             "encoder": { "type": "key", "left": "left", "right": "right", "label": "Scroll" } }"#,
        );
        assert_eq!(schema_errors(&json), Vec::<String>::new());
        let profile = parse_profile(&json).unwrap();
        let bank = &profile.banks[0];
        assert_eq!(bank.pads[&1].label(), Some("Kick"));
        assert_eq!(bank.pads[&2].label(), Some("Roll"));
        assert_eq!(bank.knobs[&1].label(), Some("Volume"));
        assert_eq!(bank.xy.as_ref().unwrap().label(), Some("Filter"));
        assert_eq!(bank.encoder.as_ref().unwrap().label(), Some("Scroll"));
        let saved = serde_json::to_string(&profile).unwrap();
        assert_eq!(parse_profile(&saved).unwrap(), profile);

        let long = minimal(&format!(
            r#"{{ "name": "A", "pads": {{ "1": {{ "type": "roll", "label": "{}" }} }} }}"#,
            "x".repeat(61)
        ));
        assert!(!schema_errors(&long).is_empty());
        assert!(parse_profile(&long).unwrap_err()[0].starts_with("banks[0].pads.1.label"));
    }

    #[test]
    fn performance_options_parse_and_match_schema() {
        let json = minimal(
            r#"{ "name": "A", "pads": { "1": { "type": "switchProfile", "profile": "live.json" } },
                             "knobs": { "1": { "type": "cc", "channel": 1, "cc": 7, "pickup": true },
                                        "2": { "type": "osc", "address": "/v", "pickup": true } },
                             "encoder": { "type": "cc", "channel": 1, "cc": 10, "acceleration": true } }"#,
        );
        assert_eq!(schema_errors(&json), Vec::<String>::new());
        let profile = parse_profile(&json).unwrap();
        assert!(profile.banks[0].knobs[&1].pickup());
        assert!(profile.banks[0].encoder.as_ref().unwrap().acceleration());
        let saved = serde_json::to_string(&profile).unwrap();
        assert_eq!(parse_profile(&saved).unwrap(), profile);
    }

    #[test]
    fn key_actions_parse_and_match_schema() {
        let json = minimal(
            r#"{ "name": "A", "pads": { "1": { "type": "key", "keys": "ctrl+shift+s" },
                                       "2": { "type": "key", "keys": "space", "hold": true, "led": "none" } },
                             "xyTouch": { "type": "key", "keys": "shift", "hold": true },
                             "encoder": { "type": "key", "left": "ctrl+minus", "right": "ctrl+plus" } }"#,
        );
        assert_eq!(schema_errors(&json), Vec::<String>::new());
        let profile = parse_profile(&json).unwrap();
        let saved = serde_json::to_string(&profile).unwrap();
        assert_eq!(parse_profile(&saved).unwrap(), profile);

        // Shortcuts the schema cannot check.
        let errors = parse_profile(&minimal(
            r#"{ "name": "A", "pads": { "1": { "type": "key", "keys": "ctrl+banana" },
                                       "2": { "type": "key", "keys": "a+b" },
                                       "3": { "type": "key", "keys": "" } },
                             "encoder": { "type": "key", "left": "left", "right": "rihgt" } }"#,
        ))
        .unwrap_err();
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("banana")));
        assert!(errors.iter().any(|e| e.starts_with("banks[0].encoder.right")));
    }

    #[test]
    fn velocity_curves_cover_the_range() {
        let curve = |curve, min, max| VelocitySettings { curve, min, max };
        for c in [VelocityCurve::Soft, VelocityCurve::Linear, VelocityCurve::Hard] {
            assert_eq!(curve(c, 1, 127).apply(1), 1);
            assert_eq!(curve(c, 1, 127).apply(127), 127);
            assert_eq!(curve(c, 30, 90).apply(1), 30);
            assert_eq!(curve(c, 30, 90).apply(127), 90);
        }
        let middle = |c| curve(c, 1, 127).apply(64);
        assert!(middle(VelocityCurve::Soft) > middle(VelocityCurve::Linear));
        assert!(middle(VelocityCurve::Linear) > middle(VelocityCurve::Hard));
        assert_eq!(VelocitySettings::default().apply(64), 64);
    }

    #[test]
    fn applies_defaults() {
        let profile = parse_profile(&minimal(
            r#"{ "name": "A", "pads": { "1": { "type": "note", "channel": 1, "note": 36 } },
                 "buttons": { "x": { "type": "cc", "channel": 1, "cc": 20 } } }"#,
        ))
        .unwrap();
        assert_eq!(profile.ports, Ports::default());
        assert_eq!(profile.start_bank, 0);
        let bank = &profile.banks[0];
        assert_eq!(
            bank.pads[&1],
            ButtonAction::Note {
                channel: 1,
                note: 36,
                velocity: None,
                mode: Mode::Momentary,
                led: None,
                label: None,
            }
        );
        assert_eq!(
            bank.buttons[&ButtonId::X],
            ButtonAction::Cc {
                channel: 1,
                cc: 20,
                on_value: 127,
                off_value: 0,
                mode: Mode::Momentary,
                led: None,
                label: None,
            }
        );
    }

    #[test]
    fn parses_pad_velocity() {
        let profile = parse_profile(&minimal(
            r#"{ "name": "A", "pads": {
                "1": { "type": "note", "channel": 1, "note": 36, "velocity": "pad" },
                "2": { "type": "note", "channel": 1, "note": 37, "velocity": 90 } } }"#,
        ))
        .unwrap();
        let velocity = |pad| match &profile.banks[0].pads[&pad] {
            ButtonAction::Note { velocity, .. } => *velocity,
            other => panic!("{other:?}"),
        };
        assert_eq!(velocity(1), Some(Velocity::Pad(PadVelocity::Pad)));
        assert_eq!(velocity(2), Some(Velocity::Fixed(90)));
    }

    #[test]
    fn rejects_unknown_fields_and_controls() {
        assert!(parse_profile(&minimal(r#"{ "name": "A", "colour": "red" }"#)).is_err());
        assert!(
            parse_profile(&minimal(
                r#"{ "name": "A", "buttons": { "shift": { "type": "switchBank", "bank": 0 } } }"#
            ))
            .is_err()
        );
        assert!(
            parse_profile(&minimal(
                r#"{ "name": "A", "pads": { "1": { "type": "note", "channel": 1, "note": 36, "vel": 3 } } }"#
            ))
            .is_err()
        );
    }

    #[test]
    fn reports_every_validation_error_with_its_path() {
        let errors = i18n::with_lang(Lang::Fr, || {
            parse_profile(&minimal(
                r#"{ "name": "A",
                 "pads": { "17": { "type": "note", "channel": 0, "note": 200 } },
                 "buttons": { "x": { "type": "note", "channel": 1, "note": 1, "velocity": "pad" },
                              "setting": { "type": "switchBank", "bank": 3 } },
                 "knobs": { "1": { "type": "cc", "channel": 1, "cc": 1, "min": 100, "max": 10 } } }"#,
            ))
            .unwrap_err()
        });
        let expected = [
            "banks[0].pads.17 : pad inconnu",
            "banks[0].pads.17.channel",
            "banks[0].pads.17.note",
            "banks[0].buttons.x.velocity",
            "banks[0].buttons.setting.bank",
            "banks[0].knobs.1 : min doit",
        ];
        for prefix in expected {
            assert!(
                errors.iter().any(|e| e.starts_with(prefix)),
                "{prefix} absent de {errors:#?}"
            );
        }
    }

    #[test]
    fn translates_validation_messages() {
        let json = minimal(r#"{ "name": "A", "pads": { "17": { "type": "note", "channel": 1, "note": 36 } } }"#);
        let first_error = |lang| i18n::with_lang(lang, || parse_profile(&json).unwrap_err().remove(0));
        assert_eq!(first_error(Lang::En), "banks[0].pads.17 : unknown pad (1 to 16)");
        assert_eq!(first_error(Lang::Fr), "banks[0].pads.17 : pad inconnu (1 à 16)");
        assert_eq!(first_error(Lang::Es), "banks[0].pads.17 : pad desconocido (1 a 16)");
    }

    #[test]
    fn default_profile_names_follow_the_language() {
        for (lang, name, drums) in [(Lang::En, "Default", "Drums"), (Lang::Fr, "Défaut", "Batterie")] {
            let json = i18n::with_lang(lang, default_profile_json);
            assert!(
                json.starts_with("{\n  \"$schema\": \"./profile.schema.json\",\n"),
                "{json}"
            );
            let profile = parse_profile(&json).unwrap();
            assert_eq!(profile.name, name);
            assert_eq!(profile.banks[0].name, drums);
            assert_eq!(profile.banks[0].display.as_deref(), Some("DRM"));
            assert_eq!(profile.banks[1].name, "Clips");
            assert_eq!(profile.banks[1].display.as_deref(), Some("CLP"));
        }
    }

    #[test]
    fn rejects_bad_version_and_start_bank() {
        let errors =
            parse_profile(r#"{ "version": 2, "name": "T", "startBank": 1, "banks": [{ "name": "A" }] }"#).unwrap_err();
        assert!(errors.iter().any(|e| e.starts_with("version")));
        assert!(errors.iter().any(|e| e.starts_with("startBank")));
    }

    #[test]
    fn store_installs_default_and_refuses_paths() {
        let dir = std::env::temp_dir().join(format!("totalkontrol-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = ProfileStore::open(dir.clone()).unwrap();
        assert!(dir.join(SCHEMA_FILE).exists());

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].file, DEFAULT_PROFILE_FILE);
        assert!(list[0].errors.is_empty());

        assert!(store.load("../secret.json").is_err());
        assert!(store.load(SCHEMA_FILE).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn store_creates_duplicates_saves_and_deletes() {
        let dir = std::env::temp_dir().join(format!("totalkontrol-edit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = ProfileStore::open(dir.clone()).unwrap();

        let file = store.create("Mon Live à moi").unwrap();
        assert_eq!(file, "mon-live-a-moi.json");
        assert_eq!(store.create("Mon Live à moi").unwrap(), "mon-live-a-moi-2.json");

        let mut profile = store.load(&file).unwrap();
        assert_eq!(profile.banks.len(), 1);
        profile.banks[0].pads.insert(
            1,
            ButtonAction::Note {
                channel: 1,
                note: 36,
                velocity: None,
                mode: Mode::Momentary,
                led: None,
                label: None,
            },
        );
        store.save(&file, profile.clone()).unwrap();
        assert_eq!(store.load(&file).unwrap().banks[0].pads.len(), 1);
        // Saving again replaces the file and leaves no temporary file behind.
        store.save(&file, profile.clone()).unwrap();
        assert!(!store.dir.join(format!("{file}.tmp")).exists());

        // An invalid profile is refused and the file is left untouched.
        let mut broken = profile.clone();
        broken.start_bank = 5;
        assert!(store.save(&file, broken).is_err());
        assert_eq!(
            store.load(&file).unwrap(),
            Profile {
                schema: Some("./profile.schema.json".into()),
                ..profile
            }
        );

        // The copy suffix, and so the file name, follows the language.
        let copy = i18n::with_lang(Lang::Fr, || store.duplicate(&file)).unwrap();
        assert_eq!(copy, "mon-live-a-moi-copie.json");
        assert_eq!(store.load(&copy).unwrap().name, "Mon Live à moi (copie)");

        store.delete(&copy).unwrap();
        assert!(store.load(&copy).is_err());
        assert!(store.delete("../secret.json").is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saved_profiles_leave_out_default_values() {
        let profile = parse_profile(DEFAULT_PROFILE_JSON).unwrap();
        let json = serde_json::to_string(&profile).unwrap();
        assert!(!json.contains("\"mode\":\"momentary\""), "{json}");
        assert!(!json.contains("onValue"));
        assert!(!json.contains("\"min\":0"));
        assert!(!json.contains("startBank"));
        assert!(!json.contains("\"ports\""));
        assert_eq!(parse_profile(&json).unwrap(), profile);
    }

    #[test]
    fn parse_value_reports_shape_and_validation_errors() {
        let shape = i18n::with_lang(Lang::Fr, || {
            parse_value(serde_json::json!({ "version": 1, "name": "A", "banks": "nope" })).unwrap_err()
        });
        assert!(shape[0].starts_with("profil invalide"), "{shape:?}");
        let rules = parse_value(serde_json::json!({ "version": 1, "name": "A", "banks": [] })).unwrap_err();
        assert!(rules.iter().any(|e| e.starts_with("banks")));
    }

    #[test]
    fn port_hints_leave_out_the_defaults() {
        assert_eq!(PortHints::from(&Ports::default()), PortHints::default());
        let ports = Ports {
            input: "padKONTROL 2".into(),
            output: Ports::default().output,
        };
        assert_eq!(
            PortHints::from(&ports),
            PortHints {
                input: Some("padKONTROL 2".into()),
                output: None
            }
        );
    }

    #[test]
    fn presets_are_valid_and_match_the_schema() {
        assert_eq!(presets().len(), PRESETS.len());
        for (id, json) in PRESETS {
            assert_eq!(schema_errors(json), Vec::<String>::new(), "{id}");
        }
    }

    #[test]
    fn imports_and_exports_profiles() {
        let root = std::env::temp_dir().join(format!("totalkontrol-import-{}", std::process::id()));
        let store = ProfileStore::open(root.join("profiles")).unwrap();
        let outside = root.join("mine.json");
        fs::write(&outside, PRESETS[11].1).unwrap();
        let file = store.import(&outside).unwrap();
        assert_eq!(store.load(&file).unwrap().name, "General MIDI drums");
        fs::write(&outside, "{ \"version\": 1 }").unwrap();
        assert!(store.import(&outside).is_err(), "checked before it is added");

        let exported = root.join("out.json");
        store.export(&file, &exported).unwrap();
        let json = fs::read_to_string(&exported).unwrap();
        assert!(!json.contains("$schema"), "the schema stays behind");
        assert_eq!(parse_profile(&json).unwrap().name, "General MIDI drums");
        for reserved in ["con.json", "NUL.json", "com1.json", "aux.backup.json"] {
            assert!(store.resolve(reserved).is_err(), "{reserved}");
        }
        let from_preset = store.create_from_preset("shortcuts").unwrap();
        assert_eq!(store.load(&from_preset).unwrap().name, "Transport shortcuts");
        assert!(store.create_from_preset("nope").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn slugs_file_names() {
        assert_eq!(slug("Batterie électro #2"), "batterie-electro-2");
        assert_eq!(slug("  --  "), "profile");
        assert_eq!(slug("Ça va"), "ca-va");
        assert_eq!(slug("Con"), "profile-con");
        assert_eq!(slug("COM1"), "profile-com1");
        assert_eq!(slug("console"), "console");
    }

    #[test]
    fn migrates_profiles_from_the_legacy_directory_once() {
        let root = std::env::temp_dir().join(format!("totalkontrol-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let legacy = root.join("old/profiles");
        let dir = root.join("new/profiles");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("live.json"), DEFAULT_PROFILE_JSON).unwrap();
        fs::write(legacy.join(SCHEMA_FILE), "{}").unwrap();

        let (store, copied) = ProfileStore::open_migrating(dir.clone(), &legacy).unwrap();
        assert_eq!(copied, 1);
        let files: Vec<String> = store.list().unwrap().into_iter().map(|p| p.file).collect();
        assert_eq!(
            files,
            vec!["live.json"],
            "no default profile added next to migrated ones"
        );
        assert!(legacy.join("live.json").exists(), "old files are kept");

        // Later launches leave the new directory alone.
        fs::write(legacy.join("other.json"), DEFAULT_PROFILE_JSON).unwrap();
        let (_, copied) = ProfileStore::open_migrating(dir.clone(), &legacy).unwrap();
        assert_eq!(copied, 0);
        assert!(!dir.join("other.json").exists());

        // No legacy directory: a plain open with the default profile.
        let (store, copied) = ProfileStore::open_migrating(root.join("fresh"), &root.join("missing")).unwrap();
        assert_eq!(copied, 0);
        assert_eq!(store.list().unwrap()[0].file, DEFAULT_PROFILE_FILE);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn display_settings_have_defaults_and_limits() {
        let profile = parse_profile(&minimal(r#"{ "name": "A", "display": "DRM" }"#)).unwrap();
        assert_eq!(profile.display, DisplaySettings::default());
        assert_eq!(profile.banks[0].display.as_deref(), Some("DRM"));

        let profile = parse_profile(
            r#"{ "version": 1, "name": "T", "display": { "pads": "note", "knobs": false, "duration": 500 },
                 "banks": [{ "name": "A" }] }"#,
        )
        .unwrap();
        assert_eq!(profile.display.pads, PadDisplay::Note);
        assert!(!profile.display.knobs);
        assert!(profile.display.encoder);

        let errors = parse_profile(
            r#"{ "version": 1, "name": "T", "display": { "duration": 50 },
                 "banks": [{ "name": "A", "display": "TROP" }, { "name": "B", "display": "été" }] }"#,
        )
        .unwrap_err();
        for prefix in ["display.duration", "banks[0].display", "banks[1].display"] {
            assert!(
                errors.iter().any(|e| e.starts_with(prefix)),
                "{prefix} absent de {errors:#?}"
            );
        }
    }
}
