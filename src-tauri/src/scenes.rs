//! The padKONTROL's 16 scenes, in its own memory: what it plays outside
//! native mode. Data dumps per "padKONTROL MIDI Implementation (Native Mode)"
//! (Korg, 2005), TABLE 1: requests, the 7-in-8 encoding, and conversion
//! between a scene and a profile bank.
//!
//! The specification's note on the encoding is missing from the copies
//! around: the usual Korg one is used (one byte of high bits, then the 7
//! bytes it completes), which gives the sizes the specification announces,
//! and a dump that does not decode to a plausible scene is refused rather
//! than guessed. The bytes between the command and the data are kept as the
//! padKONTROL sends them, and replayed when writing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::{Bank, ButtonAction, KnobAction, Mode, Velocity, XyAction};

/// Bytes of a scene, TABLE 1.
pub const SCENE_SIZE: usize = 120;
/// Longest framing seen before a scene's data: function code and a few
/// bytes of length, as in `7F 02 0A 02 40`.
const MAX_PREFIX: usize = 8;
/// The same, encoded 7 bits per byte: 17 groups of 8 bytes, then 2.
const SCENE_WIRE_SIZE: usize = 138;
pub const SCENE_COUNT: u8 = 16;
const PAD_COUNT: usize = 16;

const HEADER: [u8; 5] = [0xF0, 0x42, 0x40, 0x6E, 0x08];
const DATA_DUMP_REQUEST: u8 = 0x1F;
const DATA_DUMP: u8 = 0x7F;
const CURRENT_SCENE_DUMP: u8 = 0x40;

/// Answers of the padKONTROL to dumps and requests (command 5F).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    WriteCompleted,
    WriteError,
    LoadCompleted,
    LoadError,
    FormatError,
    SceneChanged(u8),
}

fn request(function: u8, value: u8) -> Vec<u8> {
    let mut message = HEADER.to_vec();
    message.extend([DATA_DUMP_REQUEST, function, value & 0x7F, 0xF7]);
    message
}

/// Asks for the scene in the edit buffer.
pub fn current_scene_request() -> Vec<u8> {
    request(0x10, 0)
}

/// Recalls scene `scene` (0 to 15) into the edit buffer.
pub fn scene_change_request(scene: u8) -> Vec<u8> {
    request(0x14, scene)
}

/// Stores the edit buffer into scene `scene` (0 to 15), for good.
pub fn scene_write_request(scene: u8) -> Vec<u8> {
    request(0x11, scene)
}

/// Body of a padKONTROL message (after the header, before F7), whatever its
/// global channel.
fn body(message: &[u8]) -> Option<&[u8]> {
    match message {
        [0xF0, 0x42, channel, 0x6E, 0x08, body @ .., 0xF7] if channel & 0xF0 == 0x40 => Some(body),
        _ => None,
    }
}

pub fn reply(message: &[u8]) -> Option<Reply> {
    Some(match *body(message)? {
        [0x5F, 0x21, _] => Reply::WriteCompleted,
        [0x5F, 0x22, _] => Reply::WriteError,
        [0x5F, 0x23, _] => Reply::LoadCompleted,
        [0x5F, 0x24, _] => Reply::LoadError,
        [0x5F, 0x26, _] => Reply::FormatError,
        [0x5F, 0x4F, scene] => Reply::SceneChanged(scene),
        _ => return None,
    })
}

/// Whether a message is a current scene dump.
pub fn is_scene_dump(message: &[u8]) -> bool {
    body(message).is_some_and(|body| body.first() == Some(&DATA_DUMP) && body.len() > SCENE_WIRE_SIZE)
}

/// Encodes 8-bit bytes for SysEx: per group of 7, one byte with their high
/// bits (bit n for byte n), then the 7 bytes without it.
pub fn pack(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len().div_ceil(7) * 8);
    for group in data.chunks(7) {
        out.push(group.iter().enumerate().fold(0, |high, (i, b)| high | ((b >> 7) << i)));
        out.extend(group.iter().map(|b| b & 0x7F));
    }
    out
}

pub fn unpack(wire: &[u8]) -> Vec<u8> {
    wire.chunks(8)
        .flat_map(|group| {
            let (high, bytes) = group.split_first().map_or((0, &[][..]), |(h, b)| (*h, b));
            bytes
                .iter()
                .enumerate()
                .map(move |(i, b)| b | (((high >> i) & 1) << 7))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A scene as the padKONTROL sent it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// The 120 bytes of TABLE 1.
    pub data: Vec<u8>,
    /// Bytes between the dump command and the data, function included, as
    /// the padKONTROL framed them: replayed when writing.
    pub prefix: Vec<u8>,
}

impl Scene {
    /// Reads a current scene dump. `None` for anything else, or for data
    /// that does not look like a scene.
    pub fn from_dump(message: &[u8]) -> Option<Scene> {
        let body = body(message)?;
        let (&command, rest) = body.split_first()?;
        if command != DATA_DUMP || rest.len() <= SCENE_WIRE_SIZE {
            return None;
        }
        // The data closes the message; the function code comes right before.
        let (prefix, wire) = rest.split_at(rest.len() - SCENE_WIRE_SIZE);
        let scene = Scene {
            data: unpack(wire),
            prefix: prefix.to_vec(),
        };
        scene.is_valid().then_some(scene)
    }

    /// Data that looks like a scene, framed like a current scene dump: only
    /// such a scene is sent back to the padKONTROL.
    pub fn is_valid(&self) -> bool {
        plausible(&self.data)
            && (1..=MAX_PREFIX).contains(&self.prefix.len())
            && self.prefix.iter().all(|b| *b < 0x80)
            && self.prefix.last() == Some(&CURRENT_SCENE_DUMP)
    }

    /// The dump that loads this scene into the edit buffer.
    pub fn dump(&self) -> Vec<u8> {
        let mut message = HEADER.to_vec();
        message.push(DATA_DUMP);
        message.extend(&self.prefix);
        message.extend(pack(&self.data));
        message.push(0xF7);
        message
    }
}

/// TABLE 1 leaves the two top bits of every pad mode byte unused: a dump
/// decoded the wrong way would not keep them clear.
fn plausible(data: &[u8]) -> bool {
    data.len() == SCENE_SIZE && data[..PAD_COUNT].iter().all(|mode| mode & 0xC0 == 0) && data[81] & 0x80 == 0
}

/// Pad mode byte: B5 toggle, B4 enabled, B0-3 channel.
const TOGGLE: u8 = 0x20;
const ENABLED: u8 = 0x10;

/// Knob, X and Y blocks: mode byte (B4-7 assign, B1 reversed, B0 pedal
/// channel), CC number, then the pads whose channels it sends on.
const KNOB_BLOCKS: [usize; 2] = [88, 92];
const X_BLOCK: usize = 96;
const Y_BLOCK: usize = 101;
const ASSIGN_CC: u8 = 3;

/// Channel a knob or axis sends on: the first pad it follows, else the
/// pedal's, else 1. The padKONTROL can send on several.
fn control_channel(data: &[u8], block: usize) -> u8 {
    let pads = u16::from(data[block + 2]) | u16::from(data[block + 3]) << 8;
    match (0..PAD_COUNT).find(|i| pads >> i & 1 == 1) {
        Some(pad) => (data[pad] & 0x0F) + 1,
        None if data[block] & 0x01 != 0 => (data[81] & 0x0F) + 1,
        None => 1,
    }
}

/// CC number of a knob or axis assigned to a control change.
fn control_cc(data: &[u8], block: usize) -> Option<u8> {
    (data[block] >> 4 == ASSIGN_CC).then_some(data[block + 1] & 0x7F)
}

/// The profile bank that plays like the scene. Bend and aftertouch on the
/// knobs and the XY pad, the pedal and roll/flam settings have no
/// equivalent and are left out.
pub fn to_bank(data: &[u8], name: String) -> Bank {
    let mut pads = BTreeMap::new();
    for i in 0..PAD_COUNT {
        let mode = data[i];
        if mode & ENABLED == 0 {
            continue;
        }
        let channel = (mode & 0x0F) + 1;
        let mode_kind = if mode & TOGGLE != 0 {
            Mode::Toggle
        } else {
            Mode::Momentary
        };
        let assign = data[16 + i];
        let velocity = data[32 + i];
        let action = if assign < 128 {
            ButtonAction::Note {
                channel,
                note: assign,
                velocity: (velocity > 128).then(|| Velocity::Fixed(velocity - 128)),
                mode: mode_kind,
                led: None,
                label: None,
            }
        } else {
            ButtonAction::Cc {
                channel,
                cc: assign - 128,
                on_value: data[64 + i] & 0x7F,
                off_value: data[48 + i] & 0x7F,
                mode: mode_kind,
                led: None,
                label: None,
            }
        };
        pads.insert(i as u8 + 1, action);
    }
    let knobs = KNOB_BLOCKS
        .iter()
        .enumerate()
        .filter_map(|(i, &block)| {
            let cc = control_cc(data, block)?;
            let action = KnobAction::Cc {
                channel: control_channel(data, block),
                cc,
                min: 0,
                max: 127,
                pickup: false,
                label: None,
            };
            Some((i as u8 + 1, action))
        })
        .collect();
    let xy = control_cc(data, X_BLOCK)
        .zip(control_cc(data, Y_BLOCK))
        .map(|(cc_x, cc_y)| XyAction::Xy {
            channel: control_channel(data, X_BLOCK),
            cc_x,
            cc_y,
            label: None,
        });
    Bank {
        name,
        pads,
        knobs,
        xy,
        ..Bank::default()
    }
}

/// What a scene could not take from a bank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Left {
    /// The pad's action has no equivalent in a scene: the pad is turned off.
    Pad {
        pad: u8,
    },
    /// Kept as the scene had it.
    Knob {
        knob: u8,
    },
    Xy,
    /// The knobs and the XY pad send on the channels set in the scene.
    ControlChannels,
}

/// Writes a bank's pads, knobs and XY pad into a scene's data, keeping what
/// the bank does not set (pedal, roll/flam, ports, channels of the knobs).
pub fn apply_bank(data: &mut [u8], bank: &Bank) -> Vec<Left> {
    let mut left = vec![];
    for i in 0..PAD_COUNT {
        let pad = i as u8 + 1;
        let keep = data[i] & 0xC0;
        match bank.pads.get(&pad) {
            Some(ButtonAction::Note {
                channel,
                note,
                velocity,
                mode,
                ..
            }) => {
                data[i] = keep | ENABLED | mode_bits(*mode) | (channel - 1);
                data[16 + i] = *note;
                if let Some(Velocity::Fixed(v)) = velocity {
                    data[32 + i] = 128 + v;
                } else if data[32 + i] > 128 {
                    // The value for "velocity of the hit" is not documented:
                    // the scene keeps a fixed velocity.
                    left.push(Left::Pad { pad });
                }
            }
            Some(ButtonAction::Cc {
                channel,
                cc,
                on_value,
                off_value,
                mode,
                ..
            }) => {
                data[i] = keep | ENABLED | mode_bits(*mode) | (channel - 1);
                data[16 + i] = 128 + cc;
                data[48 + i] = *off_value;
                data[64 + i] = *on_value;
            }
            None => data[i] &= !ENABLED,
            Some(_) => {
                data[i] &= !ENABLED;
                left.push(Left::Pad { pad });
            }
        }
    }
    let mut channels_differ = false;
    for (i, &block) in KNOB_BLOCKS.iter().enumerate() {
        match bank.knobs.get(&(i as u8 + 1)) {
            Some(KnobAction::Cc { channel, cc, .. }) => {
                data[block] = (data[block] & 0x0F) | ASSIGN_CC << 4;
                data[block + 1] = *cc;
                channels_differ |= control_channel(data, block) != *channel;
            }
            None => data[block] &= 0x0F,
            Some(_) => left.push(Left::Knob { knob: i as u8 + 1 }),
        }
    }
    match &bank.xy {
        Some(XyAction::Xy {
            channel, cc_x, cc_y, ..
        }) => {
            for (block, cc) in [(X_BLOCK, cc_x), (Y_BLOCK, cc_y)] {
                data[block] = (data[block] & 0x0F) | ASSIGN_CC << 4;
                data[block + 1] = *cc;
                channels_differ |= control_channel(data, block) != *channel;
            }
        }
        None => {
            data[X_BLOCK] &= 0x0F;
            data[Y_BLOCK] &= 0x0F;
        }
        Some(_) => left.push(Left::Xy),
    }
    if channels_differ {
        left.push(Left::ControlChannels);
    }
    left
}

fn mode_bits(mode: Mode) -> u8 {
    match mode {
        Mode::Momentary => 0,
        Mode::Toggle => TOGGLE,
    }
}

/// The 16 scenes, saved to a file to restore them later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    pub scenes: Vec<Scene>,
}

/// Backup files, in a folder of their own next to the profiles.
pub struct Backups {
    dir: std::path::PathBuf,
}

impl Backups {
    pub fn new(dir: std::path::PathBuf) -> Self {
        Self { dir }
    }

    /// Saves the scenes as `scenes-<date>.json` and returns the file name.
    pub fn save(&self, scenes: &[Scene]) -> Result<String, String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        // Two backups in the same second get a number rather than one file.
        let stamp = crate::diag::file_stamp();
        let file = (1..)
            .map(|n| match n {
                1 => format!("scenes-{stamp}.json"),
                n => format!("scenes-{stamp}-{n}.json"),
            })
            .find(|file| !self.dir.join(file).exists())
            .expect("a free name");
        let json = serde_json::to_string_pretty(&Backup {
            scenes: scenes.to_vec(),
        })
        .map_err(|e| e.to_string())?;
        std::fs::write(self.dir.join(&file), json).map_err(|e| e.to_string())?;
        Ok(file)
    }

    /// Backup file names, newest first.
    pub fn list(&self) -> Vec<String> {
        let mut files: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.starts_with("scenes-") && name.ends_with(".json"))
            .collect();
        files.sort_unstable_by(|a, b| b.cmp(a));
        files
    }

    pub fn load(&self, file: &str) -> Result<Backup, String> {
        if file.contains(['/', '\\']) {
            return Err(t!("core.scenes.badBackup"));
        }
        let json = std::fs::read_to_string(self.dir.join(file)).map_err(|e| e.to_string())?;
        let backup: Backup = serde_json::from_str(&json).map_err(|_| t!("core.scenes.badBackup"))?;
        let valid = backup.scenes.len() == usize::from(SCENE_COUNT) && backup.scenes.iter().all(Scene::is_valid);
        if valid {
            Ok(backup)
        } else {
            Err(t!("core.scenes.badBackup"))
        }
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene: pads 1-16 play notes 36-51 on channel 10, pad 16 is a
    /// toggle CC 20 with a fixed velocity, knob 1 sends CC 7 on pad 1's
    /// channel, X and Y send CCs 1 and 2.
    fn scene_data() -> Vec<u8> {
        let mut data = vec![0u8; SCENE_SIZE];
        for i in 0..PAD_COUNT {
            data[i] = ENABLED | 9;
            data[16 + i] = 36 + i as u8;
        }
        data[15] = ENABLED | TOGGLE | 9;
        data[31] = 128 + 20;
        data[47] = 128 + 100;
        data[63] = 0;
        data[79] = 127;
        data[81] = 0x10;
        data[88] = ASSIGN_CC << 4;
        data[89] = 7;
        data[90] = 0x01;
        data[96] = ASSIGN_CC << 4;
        data[97] = 1;
        data[98] = 0x01;
        data[101] = ASSIGN_CC << 4;
        data[102] = 2;
        data[103] = 0x01;
        data[106] = 200;
        data
    }

    fn dump(data: &[u8]) -> Vec<u8> {
        // The framing of the specification: over 0x7F, 2 bytes, count, function.
        Scene {
            data: data.to_vec(),
            prefix: vec![0x7F, 0x02, 0x0A, 0x02, 0x40],
        }
        .dump()
    }

    #[test]
    fn backups_round_trip() {
        let dir = std::env::temp_dir().join(format!("totalkontrol-scenes-{}", std::process::id()));
        let backups = Backups::new(dir.clone());
        let scene = Scene::from_dump(&dump(&scene_data())).unwrap();
        let file = backups.save(&vec![scene.clone(); 16]).unwrap();
        assert!(file.starts_with("scenes-20"));
        assert_eq!(backups.list(), vec![file.clone()]);
        assert_eq!(backups.load(&file).unwrap().scenes[15], scene);
        assert!(backups.load("../profiles/x.json").is_err());
        std::fs::write(dir.join("scenes-bad.json"), r#"{ "scenes": [] }"#).unwrap();
        assert!(backups.load("scenes-bad.json").is_err());
        assert_ne!(
            backups.save(&vec![scene.clone(); 16]).unwrap(),
            file,
            "never overwritten"
        );

        // The prefix goes to the padKONTROL as is: only a dump's framing.
        for prefix in [
            vec![],
            vec![0x7F, 0x02],
            vec![0xF7, 0x40],
            vec![0x00; 9].into_iter().chain([0x40]).collect(),
        ] {
            let mut tampered = vec![scene.clone(); 16];
            tampered[3].prefix = prefix.clone();
            let file = backups.save(&tampered).unwrap();
            assert!(backups.load(&file).is_err(), "{prefix:02X?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn encodes_8_bit_data_in_7_bits() {
        let data: Vec<u8> = (0..=255).cycle().take(SCENE_SIZE).collect();
        let wire = pack(&data);
        assert_eq!(wire.len(), SCENE_WIRE_SIZE, "the size the specification announces");
        assert!(wire.iter().all(|b| b & 0x80 == 0));
        assert_eq!(unpack(&wire), data);
        assert_eq!(pack(&[0x80, 0x01]), vec![0x01, 0x00, 0x01]);
        assert_eq!(pack(&[0u8; 88]).len(), 101, "the global dump's size");
    }

    #[test]
    fn reads_a_scene_dump_and_writes_it_back_as_framed() {
        let message = dump(&scene_data());
        assert!(is_scene_dump(&message));
        let scene = Scene::from_dump(&message).unwrap();
        assert_eq!(scene.data, scene_data());
        assert_eq!(scene.prefix, vec![0x7F, 0x02, 0x0A, 0x02, 0x40]);
        assert_eq!(scene.dump(), message, "same bytes back");

        // Another framing, from another channel: kept as is.
        let mut other = HEADER.to_vec();
        other[2] = 0x43;
        other.extend([0x7F, 0x40]);
        other.extend(pack(&scene_data()));
        other.push(0xF7);
        assert_eq!(Scene::from_dump(&other).unwrap().prefix, vec![0x40]);
    }

    #[test]
    fn refuses_what_is_not_a_plausible_scene() {
        let mut bad = scene_data();
        bad[0] = 0xC0;
        assert_eq!(Scene::from_dump(&dump(&bad)), None);
        assert_eq!(Scene::from_dump(&current_scene_request()), None);
        let mut short = dump(&scene_data());
        short.remove(20);
        assert_eq!(Scene::from_dump(&short), None);
    }

    #[test]
    fn reads_replies() {
        assert_eq!(
            reply(&[0xF0, 0x42, 0x40, 0x6E, 0x08, 0x5F, 0x23, 0x00, 0xF7]),
            Some(Reply::LoadCompleted)
        );
        assert_eq!(
            reply(&[0xF0, 0x42, 0x45, 0x6E, 0x08, 0x5F, 0x4F, 0x03, 0xF7]),
            Some(Reply::SceneChanged(3))
        );
        assert_eq!(
            reply(&[0xF0, 0x42, 0x40, 0x6E, 0x08, 0x5F, 0x00, 0x00, 0xF7]),
            None,
            "the heartbeat's"
        );
        assert_eq!(
            scene_write_request(15),
            vec![0xF0, 0x42, 0x40, 0x6E, 0x08, 0x1F, 0x11, 0x0F, 0xF7]
        );
        assert_eq!(
            scene_change_request(2),
            vec![0xF0, 0x42, 0x40, 0x6E, 0x08, 0x1F, 0x14, 0x02, 0xF7]
        );
        assert_eq!(
            current_scene_request(),
            vec![0xF0, 0x42, 0x40, 0x6E, 0x08, 0x1F, 0x10, 0x00, 0xF7]
        );
    }

    #[test]
    fn a_scene_becomes_a_bank() {
        let bank = to_bank(&scene_data(), "Scene 1".into());
        assert_eq!(bank.pads.len(), 16);
        assert_eq!(
            bank.pads[&1],
            ButtonAction::Note {
                channel: 10,
                note: 36,
                velocity: None,
                mode: Mode::Momentary,
                led: None,
                label: None,
            }
        );
        assert_eq!(
            bank.pads[&16],
            ButtonAction::Cc {
                channel: 10,
                cc: 20,
                on_value: 127,
                off_value: 0,
                mode: Mode::Toggle,
                led: None,
                label: None,
            }
        );
        assert_eq!(
            bank.knobs[&1],
            KnobAction::Cc {
                channel: 10,
                cc: 7,
                min: 0,
                max: 127,
                pickup: false,
                label: None,
            }
        );
        assert!(!bank.knobs.contains_key(&2), "knob 2 is off");
        assert_eq!(
            bank.xy,
            Some(XyAction::Xy {
                channel: 10,
                cc_x: 1,
                cc_y: 2,
                label: None,
            })
        );
    }

    #[test]
    fn a_bank_goes_back_into_a_scene() {
        let mut data = scene_data();
        let bank = to_bank(&data, "Scene".into());
        assert_eq!(apply_bank(&mut data, &bank), vec![]);
        assert_eq!(data, scene_data(), "a bank made from a scene changes nothing");

        let mut bank = bank;
        bank.pads.insert(
            2,
            ButtonAction::Note {
                channel: 2,
                note: 60,
                velocity: Some(Velocity::Fixed(90)),
                mode: Mode::Toggle,
                led: None,
                label: None,
            },
        );
        bank.pads.insert(3, ButtonAction::Roll { label: None });
        bank.pads.remove(&4);
        bank.knobs.remove(&1);
        let left = apply_bank(&mut data, &bank);
        assert_eq!(left, vec![Left::Pad { pad: 3 }]);
        assert_eq!(data[1], ENABLED | TOGGLE | 1);
        assert_eq!((data[17], data[33]), (60, 128 + 90));
        assert_eq!(data[2] & ENABLED, 0, "roll has no equivalent: pad 3 off");
        assert_eq!(data[3] & ENABLED, 0);
        assert_eq!(data[88] >> 4, 0, "knob 1 off");
        assert_eq!(data[106], 200, "roll speed kept");
    }
}
