//! Korg padKONTROL native mode protocol: input and output SysEx messages,
//! and decoding of received messages into control events.
//! Frames captured from the device are in `docs/padkontrol-sysex.txt`.

use serde::Serialize;

use crate::config::ButtonId;

/// Header shared by all native SysEx messages: Korg, global MIDI channel
/// (0x40 + channel, here channel 1), padKONTROL. Messages are built for
/// channel 1 and readdressed on the way out (see `for_channel`).
const HEADER: [u8; 5] = [0xF0, 0x42, 0x40, 0x6E, 0x08];
const SYSEX_END: u8 = 0xF7;

/// Buttons in the order of their native code (0x00 to 0x12).
const BUTTONS: [ButtonId; 19] = [
    ButtonId::Scene,
    ButtonId::Message,
    ButtonId::Setting,
    ButtonId::NoteCc,
    ButtonId::MidiCh,
    ButtonId::SwType,
    ButtonId::RelVal,
    ButtonId::Velocity,
    ButtonId::Port,
    ButtonId::FixedVelocity,
    ButtonId::ProgChange,
    ButtonId::X,
    ButtonId::Y,
    ButtonId::Knob1Assign,
    ButtonId::Knob2Assign,
    ButtonId::Pedal,
    ButtonId::Roll,
    ButtonId::Flam,
    ButtonId::Hold,
];

fn sysex(body: &[u8]) -> Vec<u8> {
    let mut message = HEADER.to_vec();
    message.extend_from_slice(body);
    message.push(SYSEX_END);
    message
}

/// Messages to send to enter native mode: activation, reporting of all
/// controls, LEDs off and text on the display (3 characters).
pub fn enter_native_mode(display: &str) -> Vec<Vec<u8>> {
    let mut text = [b' '; 3];
    for (slot, c) in text.iter_mut().zip(display.bytes()) {
        *slot = if c.is_ascii() { c } else { b'?' };
    }
    let mut leds_and_display = vec![0x3F, 0x0A, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    leds_and_display.extend_from_slice(&text);

    vec![
        sysex(&[0x00, 0x00, 0x01]),
        report_all_controls(),
        sysex(&leds_and_display),
    ]
}

/// Asks the padKONTROL to report every control. It must follow native mode
/// on, which resets it. The padKONTROL acknowledges it every time without
/// touching the LEDs or the display, so it doubles as a heartbeat.
pub fn report_all_controls() -> Vec<u8> {
    sysex(&[
        0x3F, 0x2A, 0x00, 0x00, 0x05, 0x05, 0x05, 0x7F, 0x7E, 0x7F, 0x7F, 0x03, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A,
        0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
    ])
}

/// The padKONTROL's acknowledgement of `report_all_controls`.
pub fn is_report_ack(message: &[u8]) -> bool {
    body(message) == Some(&[0x5F, 0x00, 0x00])
}

/// "Native mode on" for each of the 16 global MIDI channels: only the
/// padKONTROL's own channel answers (see `native_mode_ack_channel`).
pub fn native_mode_on_all_channels() -> Vec<Vec<u8>> {
    (0..16)
        .map(|channel| {
            let mut message = sysex(&[0x00, 0x00, 0x01]);
            for_channel(&mut message, channel);
            message
        })
        .collect()
}

/// Global MIDI channel (0 to 15) of a padKONTROL acknowledging native mode on.
pub fn native_mode_ack_channel(message: &[u8]) -> Option<u8> {
    (body(message) == Some(&[0x40, 0x00, 0x03])).then(|| message[2] & 0x0F)
}

/// Readdresses a native message built for channel 1 to the padKONTROL's
/// global MIDI channel (0 to 15). Other messages are left alone.
pub fn for_channel(message: &mut [u8], channel: u8) {
    if message.starts_with(&HEADER[..3]) {
        message[2] = 0x40 | (channel & 0x0F);
    }
}

/// Body of a native message, whatever the padKONTROL's global channel.
fn body(message: &[u8]) -> Option<&[u8]> {
    match message {
        [0xF0, 0x42, channel, 0x6E, 0x08, body @ .., SYSEX_END] if channel & 0xF0 == 0x40 => Some(body),
        _ => None,
    }
}

/// Message to send to leave native mode and hand control back to the
/// padKONTROL's normal operation.
pub fn exit_native_mode() -> Vec<u8> {
    sysex(&[0x00, 0x00, 0x00])
}

/// Native button code the padKONTROL sends when a finger lands on (0x7F)
/// or leaves (0x00) the XY pad.
const XY_TOUCH_CODE: u8 = 0x20;

/// Control that can be pressed: a pad (1 to 16), a button, or touching the
/// XY pad. Only pads and buttons have an LED.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    Pad(u8),
    Button(ButtonId),
    XyTouch,
}

impl Control {
    /// Every control with an LED.
    pub fn all() -> impl Iterator<Item = Control> {
        (1..=16)
            .map(Control::Pad)
            .chain(BUTTONS.iter().copied().map(Control::Button))
    }

    pub fn has_led(self) -> bool {
        self.led_code().is_some()
    }

    /// Native LED code: 0x00 to 0x0F for pads, 0x10 + button code.
    fn led_code(self) -> Option<u8> {
        match self {
            Control::Pad(pad) => Some(pad - 1),
            Control::Button(id) => BUTTONS.iter().position(|&b| b == id).map(|code| 0x10 + code as u8),
            Control::XyTouch => None,
        }
    }
}

/// LED message of a control. Only for controls with an LED (`has_led`).
pub fn led(control: Control, on: bool) -> Vec<u8> {
    let code = control.led_code().expect("control with an LED");
    sysex(&[0x01, code, if on { 0x20 } else { 0x00 }])
}

/// Control and state of an LED message, as `led` builds them.
pub fn decode_led(message: &[u8]) -> Option<(Control, bool)> {
    let [0x01, code, value] = *body(message)? else {
        return None;
    };
    let control = match code {
        0x00..=0x0F => Control::Pad(code + 1),
        _ => Control::Button(*BUTTONS.get(usize::from(code.checked_sub(0x10)?))?),
    };
    Some((control, value != 0))
}

/// Text on the 3-character display, in uppercase. Spaces are kept, so values
/// can be right-aligned.
pub fn display(text: &str) -> Vec<u8> {
    let mut chars = [b' '; 3];
    for (slot, c) in chars.iter_mut().zip(text.chars()) {
        *slot = if c.is_ascii() {
            c.to_ascii_uppercase() as u8
        } else {
            b'-'
        };
    }
    sysex(&[0x22, 0x04, 0x00, chars[0], chars[1], chars[2]])
}

/// Event decoded from a native message. Pads and knobs are numbered
/// from 1, as in the profiles.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ControlEvent {
    Pad {
        pad: u8,
        pressed: bool,
        velocity: u8,
    },
    Button {
        button: ButtonId,
        pressed: bool,
    },
    Knob {
        knob: u8,
        value: u8,
    },
    Xy {
        x: u8,
        y: u8,
    },
    /// A finger lands on (`pressed`) or leaves the XY pad.
    XyTouch {
        pressed: bool,
    },
    Encoder {
        delta: i8,
    },
}

pub fn decode(message: &[u8]) -> Option<ControlEvent> {
    match *body(message)? {
        [0x45, pad, velocity] => Some(ControlEvent::Pad {
            pad: (pad & 0x0F) + 1,
            pressed: pad & 0x40 != 0,
            velocity,
        }),
        [0x48, XY_TOUCH_CODE, state] => Some(ControlEvent::XyTouch { pressed: state != 0 }),
        [0x48, code, state] => BUTTONS.get(code as usize).map(|&button| ControlEvent::Button {
            button,
            pressed: state != 0,
        }),
        [0x49, knob @ (0 | 1), value] => Some(ControlEvent::Knob { knob: knob + 1, value }),
        [0x4B, x, y] => Some(ControlEvent::Xy { x, y }),
        // Signed 7-bit step: 0x01 = one click right, 0x7F = one click left.
        [0x43, 0x00, step] => Some(ControlEvent::Encoder {
            delta: ((step << 1) as i8) >> 1,
        }),
        _ => None,
    }
}

/// Human-readable description of any MIDI message, for the monitor.
pub fn describe(message: &[u8]) -> String {
    let Some(&status) = message.first() else {
        return t!("core.monitor.emptyMessage");
    };
    let channel = (status & 0x0F) + 1;
    let data = |i: usize| message.get(i).copied().unwrap_or(0);
    let (d1, d2) = (data(1), data(2));
    match status & 0xF0 {
        0x80 => t!("core.monitor.noteOff", channel, d1, d2),
        0x90 if d2 == 0 => t!("core.monitor.noteOffNoVelocity", channel, d1),
        0x90 => t!("core.monitor.noteOn", channel, d1, d2),
        0xA0 => t!("core.monitor.aftertouch", channel, d1, d2),
        0xB0 => t!("core.monitor.controlChange", channel, d1, d2),
        0xC0 => t!("core.monitor.programChange", channel, d1),
        0xD0 => t!("core.monitor.pressure", channel, d1),
        0xE0 => {
            let bend = (d2 as u16) << 7 | d1 as u16;
            t!("core.monitor.pitchBend", channel, bend)
        }
        _ if status == 0xF0 => {
            let len = message.len();
            t!("core.monitor.sysEx", len)
        }
        _ => t!("core.monitor.system", status = format!("{status:02X}")),
    }
}

pub fn to_hex(message: &[u8]) -> String {
    message.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, with_lang};

    #[test]
    fn heartbeat_messages() {
        // Captured from the device: the reporting setup is acknowledged with
        // 5F 00 00, the LEDs and display with 5F 01 00, native mode on with 40 00 03.
        assert_eq!(enter_native_mode("ABC")[1], report_all_controls());
        assert!(is_report_ack(&hex("f042406e085f0000f7")));
        assert!(!is_report_ack(&hex("f042406e085f0100f7")));
        assert!(!is_report_ack(&hex("f042406e08400003f7")));
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn decoded(s: &str) -> ControlEvent {
        decode(&hex(s)).unwrap_or_else(|| panic!("{s} not decoded"))
    }

    // Frames captured in docs/padkontrol-sysex.txt.
    #[test]
    fn decodes_buttons() {
        assert_eq!(
            decoded("f042406e0848027ff7"),
            ControlEvent::Button {
                button: ButtonId::Setting,
                pressed: true
            }
        );
        assert_eq!(
            decoded("f042406e08480200f7"),
            ControlEvent::Button {
                button: ButtonId::Setting,
                pressed: false
            }
        );
        assert_eq!(
            decoded("f042406e08480b7ff7"),
            ControlEvent::Button {
                button: ButtonId::X,
                pressed: true
            }
        );
        assert_eq!(
            decoded("f042406e08480f7ff7"),
            ControlEvent::Button {
                button: ButtonId::Pedal,
                pressed: true
            }
        );
        assert_eq!(
            decoded("f042406e0848127ff7"),
            ControlEvent::Button {
                button: ButtonId::Hold,
                pressed: true
            }
        );
        assert_eq!(
            decoded("f042406e08480d00f7"),
            ControlEvent::Button {
                button: ButtonId::Knob1Assign,
                pressed: false
            }
        );
    }

    #[test]
    fn decodes_pads() {
        assert_eq!(
            decoded("f042406e0845401df7"),
            ControlEvent::Pad {
                pad: 1,
                pressed: true,
                velocity: 0x1D
            }
        );
        assert_eq!(
            decoded("f042406e08450040f7"),
            ControlEvent::Pad {
                pad: 1,
                pressed: false,
                velocity: 0x40
            }
        );
        assert_eq!(
            decoded("f042406e08454f13f7"),
            ControlEvent::Pad {
                pad: 16,
                pressed: true,
                velocity: 0x13
            }
        );
        assert_eq!(
            decoded("f042406e08450f40f7"),
            ControlEvent::Pad {
                pad: 16,
                pressed: false,
                velocity: 0x40
            }
        );
    }

    #[test]
    fn decodes_encoder_knobs_and_xy() {
        assert_eq!(decoded("f042406e08430001f7"), ControlEvent::Encoder { delta: 1 });
        assert_eq!(decoded("f042406e0843007ff7"), ControlEvent::Encoder { delta: -1 });
        assert_eq!(
            decoded("f042406e0849002af7"),
            ControlEvent::Knob { knob: 1, value: 0x2A }
        );
        assert_eq!(
            decoded("f042406e0849017ff7"),
            ControlEvent::Knob { knob: 2, value: 0x7F }
        );
        assert_eq!(decoded("f042406e084b1040f7"), ControlEvent::Xy { x: 0x10, y: 0x40 });
    }

    #[test]
    fn decodes_xy_touch() {
        // Frames from a capture: the touch comes before each XY position.
        assert_eq!(decoded("f042406e0848207ff7"), ControlEvent::XyTouch { pressed: true });
        assert_eq!(decoded("f042406e08482000f7"), ControlEvent::XyTouch { pressed: false });
        assert!(!Control::XyTouch.has_led());
        assert!(Control::all().all(Control::has_led));
    }

    #[test]
    fn ignores_unknown_messages() {
        assert_eq!(decode(&hex("f042406e0848217ff7")), None);
        assert_eq!(decode(&hex("f042406e08497f00f7")), None);
        assert_eq!(decode(&hex("903664")), None);
        assert_eq!(decode(&hex("f042406e0845")), None);
    }

    #[test]
    fn builds_native_mode_messages() {
        let init = enter_native_mode("TK");
        assert_eq!(init[0], hex("f042406e08000001f7"));
        assert_eq!(to_hex(&init[1][..8]), "F0 42 40 6E 08 3F 2A 00");
        assert_eq!(init[2], hex("f042406e083f0a01000000000000544b20f7"));
        assert!(
            init.iter()
                .all(|m| m.len() >= 6 && m[..5] == HEADER && m.last() == Some(&SYSEX_END))
        );
        assert_eq!(exit_native_mode(), hex("f042406e08000000f7"));
    }

    #[test]
    fn decodes_led_messages() {
        for control in Control::all() {
            assert_eq!(decode_led(&led(control, true)), Some((control, true)));
            assert_eq!(decode_led(&led(control, false)), Some((control, false)));
        }
        assert_eq!(decode_led(&hex("f042406e08017f20f7")), None, "unknown LED code");
        assert_eq!(decode_led(&display("ABC")), None);
    }

    #[test]
    fn works_on_any_global_channel() {
        // A padKONTROL on global channel 3 (0x42) answers and reports with it.
        assert_eq!(native_mode_ack_channel(&hex("f042426e08400003f7")), Some(2));
        assert_eq!(native_mode_ack_channel(&hex("f042406e08400003f7")), Some(0));
        assert_eq!(native_mode_ack_channel(&hex("f042426e08400002f7")), None);
        assert!(is_report_ack(&hex("f042426e085f0000f7")));
        assert_eq!(
            decode(&hex("f042426e08454f40f7")),
            Some(ControlEvent::Pad {
                pad: 16,
                pressed: true,
                velocity: 0x40
            })
        );
        assert_eq!(decode(&hex("f042306e08450f40f7")), None, "not a channel byte");

        let mut led = led(Control::Pad(1), true);
        for_channel(&mut led, 2);
        assert_eq!(led, hex("f042426e08010020f7"));
        let mut note = vec![0x90, 60, 100];
        for_channel(&mut note, 2);
        assert_eq!(note, vec![0x90, 60, 100], "only native messages");

        let all = native_mode_on_all_channels();
        assert_eq!(all.len(), 16);
        assert_eq!(all[15], hex("f0424f6e08000001f7"));
    }

    #[test]
    fn builds_led_and_display_messages() {
        // Frames from the old mappings.xml: SETTING and X LEDs.
        assert_eq!(led(Control::Button(ButtonId::Setting), true), hex("f042406e08011220f7"));
        assert_eq!(led(Control::Button(ButtonId::X), false), hex("f042406e08011b00f7"));
        assert_eq!(led(Control::Pad(1), true), hex("f042406e08010020f7"));
        assert_eq!(led(Control::Pad(16), true), hex("f042406e08010f20f7"));
        assert_eq!(display("Batterie"), hex("f042406e08220400424154f7"));
        assert_eq!(display("a b"), hex("f042406e08220400412042f7"));
        assert_eq!(display(" 64"), hex("f042406e08220400203634f7"));
        assert_eq!(Control::all().count(), 35);
    }

    #[test]
    fn describes_standard_messages() {
        with_lang(Lang::Fr, || {
            assert_eq!(describe(&hex("993664")), "Note On  canal 10  note 54  vél. 100");
            assert_eq!(describe(&hex("903600")), "Note Off  canal 1  note 54");
            assert_eq!(describe(&hex("b10740")), "CC  canal 2  n° 7  valeur 64");
            assert_eq!(describe(&hex("f042406e08000001f7")), "SysEx (9 octets)");
        });
        with_lang(Lang::En, || {
            assert_eq!(describe(&hex("993664")), "Note On  ch. 10  note 54  vel. 100");
            assert_eq!(describe(&hex("b10740")), "CC  ch. 2  #7  value 64");
        });
    }
}
