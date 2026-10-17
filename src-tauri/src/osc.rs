//! OSC output over UDP: the messages built by the engine from `osc`
//! actions, and the "mirror" that sends every control to a fixed address.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

use rosc::{OscPacket, OscType};

use crate::config::{ButtonId, OSC_VELOCITY, OscValue};
use crate::padkontrol::ControlEvent;

#[derive(Debug, Clone, PartialEq)]
pub enum OscArg {
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OscMessage {
    pub address: String,
    pub args: Vec<OscArg>,
}

impl OscMessage {
    pub fn new(address: impl Into<String>, args: Vec<OscArg>) -> Self {
        Self {
            address: address.into(),
            args,
        }
    }

    /// Readable form for the monitor, e.g. `/live/clip 0 3`.
    pub fn describe(&self) -> String {
        let mut text = self.address.clone();
        for arg in &self.args {
            text.push(' ');
            match arg {
                OscArg::Int(v) => text.push_str(&v.to_string()),
                OscArg::Float(v) => text.push_str(&format!("{v:?}")),
                OscArg::Bool(v) => text.push_str(&v.to_string()),
                OscArg::Str(v) => text.push_str(&format!("\"{v}\"")),
            }
        }
        text
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let args = self
            .args
            .iter()
            .map(|arg| match arg {
                OscArg::Int(v) => OscType::Int(*v),
                OscArg::Float(v) => OscType::Float(*v),
                OscArg::Bool(v) => OscType::Bool(*v),
                OscArg::Str(v) => OscType::String(v.clone()),
            })
            .collect();
        rosc::encoder::encode(&OscPacket::Message(rosc::OscMessage {
            addr: self.address.clone(),
            args,
        }))
        .map_err(|e| e.to_string())
    }
}

impl OscArg {
    /// Whether the argument reads as "on": a non-zero number, `true`, or a
    /// string other than "", "0" and "false".
    pub fn is_on(&self) -> bool {
        match self {
            OscArg::Int(v) => *v != 0,
            OscArg::Float(v) => *v != 0.0,
            OscArg::Bool(v) => *v,
            OscArg::Str(s) => !matches!(s.as_str(), "" | "0" | "false"),
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            OscArg::Int(v) => Some(*v),
            OscArg::Float(v) => Some(v.round() as i32),
            _ => None,
        }
    }
}

/// Messages of a received UDP packet, bundles flattened. Arguments of other
/// types are converted when they map to a number, else dropped.
pub fn decode(bytes: &[u8]) -> Vec<OscMessage> {
    fn collect(packet: OscPacket, out: &mut Vec<OscMessage>) {
        match packet {
            OscPacket::Message(m) => {
                let args = m
                    .args
                    .into_iter()
                    .filter_map(|arg| match arg {
                        OscType::Int(v) => Some(OscArg::Int(v)),
                        OscType::Long(v) => Some(OscArg::Int(v as i32)),
                        OscType::Float(v) => Some(OscArg::Float(v)),
                        OscType::Double(v) => Some(OscArg::Float(v as f32)),
                        OscType::Bool(v) => Some(OscArg::Bool(v)),
                        OscType::String(v) => Some(OscArg::Str(v)),
                        OscType::Char(c) => Some(OscArg::Str(c.to_string())),
                        _ => None,
                    })
                    .collect();
                out.push(OscMessage::new(m.addr, args));
            }
            OscPacket::Bundle(bundle) => bundle.content.into_iter().for_each(|p| collect(p, out)),
        }
    }
    let mut messages = vec![];
    if let Ok((_, packet)) = rosc::decoder::decode_udp(bytes) {
        collect(packet, &mut messages);
    }
    messages
}

/// Arguments of a profile action, with `$velocity` replaced.
pub fn resolve(values: &[OscValue], velocity: u8) -> Vec<OscArg> {
    values
        .iter()
        .map(|value| match value {
            OscValue::Int(v) => OscArg::Int(*v),
            OscValue::Float(v) => OscArg::Float(*v),
            OscValue::Bool(v) => OscArg::Bool(*v),
            OscValue::Str(s) if s == OSC_VELOCITY => OscArg::Int(velocity.into()),
            OscValue::Str(s) => OscArg::Str(s.clone()),
        })
        .collect()
}

/// Address prefix as typed by the user, cleaned up: starts with `/`, no
/// trailing `/`, no spaces.
pub fn normalize_prefix(prefix: &str) -> String {
    let trimmed: String = prefix.trim().chars().filter(|c| !c.is_whitespace()).collect();
    let trimmed = trimmed.trim_end_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

/// Mirror message of a control event: every control, whatever the profile.
/// The encoder sends its step, then its position when the engine keeps one.
pub fn mirror(prefix: &str, event: &ControlEvent, encoder_position: Option<i16>) -> OscMessage {
    match *event {
        ControlEvent::Pad { pad, pressed, velocity } => OscMessage::new(
            format!("{prefix}/pad/{pad}"),
            vec![OscArg::Int(if pressed { velocity.into() } else { 0 })],
        ),
        ControlEvent::Button { button, pressed } => OscMessage::new(
            format!("{prefix}/button/{}", button_name(button)),
            vec![OscArg::Int(pressed.into())],
        ),
        ControlEvent::Knob { knob, value } => {
            OscMessage::new(format!("{prefix}/knob/{knob}"), vec![OscArg::Int(value.into())])
        }
        ControlEvent::Xy { x, y } => OscMessage::new(
            format!("{prefix}/xy"),
            vec![OscArg::Int(x.into()), OscArg::Int(y.into())],
        ),
        ControlEvent::XyTouch { pressed } => {
            OscMessage::new(format!("{prefix}/xy/touch"), vec![OscArg::Int(pressed.into())])
        }
        ControlEvent::Encoder { delta } => {
            let mut args = vec![OscArg::Int(delta.into())];
            args.extend(encoder_position.map(|p| OscArg::Int(p.into())));
            OscMessage::new(format!("{prefix}/encoder"), args)
        }
    }
}

pub fn mirror_bank(prefix: &str, bank: usize) -> OscMessage {
    OscMessage::new(format!("{prefix}/bank"), vec![OscArg::Int(bank as i32)])
}

fn button_name(button: ButtonId) -> String {
    serde_json::to_string(&button)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

/// UDP socket bound to an OSC destination.
pub struct OscSender {
    socket: UdpSocket,
    target: SocketAddr,
}

impl OscSender {
    pub fn open(host: &str, port: u16) -> Result<Self, String> {
        let target = (host, port)
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .next()
            .ok_or_else(|| host.to_string())?;
        let local = if target.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
        let socket = UdpSocket::bind(local).map_err(|e| e.to_string())?;
        Ok(Self { socket, target })
    }

    pub fn send(&self, message: &OscMessage) -> bool {
        message
            .encode()
            .is_ok_and(|bytes| self.socket.send_to(&bytes, self.target).is_ok())
    }

    pub fn target(&self) -> SocketAddr {
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_velocity_in_arguments() {
        let values = vec![
            OscValue::Int(3),
            OscValue::Str("$velocity".into()),
            OscValue::Str("go".into()),
        ];
        assert_eq!(
            resolve(&values, 90),
            vec![OscArg::Int(3), OscArg::Int(90), OscArg::Str("go".into())]
        );
    }

    #[test]
    fn normalizes_prefixes() {
        assert_eq!(normalize_prefix("/totalkontrol/"), "/totalkontrol");
        assert_eq!(normalize_prefix("pad k"), "/padk");
        assert_eq!(normalize_prefix("  "), "");
    }

    #[test]
    fn mirrors_every_control() {
        let p = "/tk";
        let m = |e| mirror(p, &e, None).describe();
        assert_eq!(
            m(ControlEvent::Pad {
                pad: 3,
                pressed: true,
                velocity: 100
            }),
            "/tk/pad/3 100"
        );
        assert_eq!(
            m(ControlEvent::Pad {
                pad: 3,
                pressed: false,
                velocity: 64
            }),
            "/tk/pad/3 0"
        );
        assert_eq!(
            m(ControlEvent::Button {
                button: ButtonId::Hold,
                pressed: true
            }),
            "/tk/button/hold 1"
        );
        assert_eq!(
            m(ControlEvent::Button {
                button: ButtonId::Knob1Assign,
                pressed: false
            }),
            "/tk/button/knob1Assign 0"
        );
        assert_eq!(m(ControlEvent::Knob { knob: 2, value: 64 }), "/tk/knob/2 64");
        assert_eq!(m(ControlEvent::Xy { x: 10, y: 20 }), "/tk/xy 10 20");
        assert_eq!(m(ControlEvent::XyTouch { pressed: true }), "/tk/xy/touch 1");
        assert_eq!(m(ControlEvent::XyTouch { pressed: false }), "/tk/xy/touch 0");
        assert_eq!(m(ControlEvent::Encoder { delta: -1 }), "/tk/encoder -1");
        assert_eq!(
            mirror(p, &ControlEvent::Encoder { delta: 1 }, Some(12)).describe(),
            "/tk/encoder 1 12"
        );
        assert_eq!(mirror_bank(p, 1).describe(), "/tk/bank 1");
    }

    #[test]
    fn decodes_messages_and_bundles() {
        let message = OscMessage::new("/a", vec![OscArg::Int(1), OscArg::Str("x".into())]);
        assert_eq!(decode(&message.encode().unwrap()), vec![message.clone()]);

        let bundle = OscPacket::Bundle(rosc::OscBundle {
            timetag: rosc::OscTime {
                seconds: 0,
                fractional: 1,
            },
            content: vec![
                OscPacket::Message(rosc::OscMessage {
                    addr: "/b".into(),
                    args: vec![OscType::Double(0.5)],
                }),
                OscPacket::Message(rosc::OscMessage {
                    addr: "/c".into(),
                    args: vec![OscType::Nil, OscType::Long(3)],
                }),
            ],
        });
        let bytes = rosc::encoder::encode(&bundle).unwrap();
        assert_eq!(
            decode(&bytes),
            vec![
                OscMessage::new("/b", vec![OscArg::Float(0.5)]),
                OscMessage::new("/c", vec![OscArg::Int(3)]),
            ]
        );
        assert!(decode(b"not osc").is_empty());
    }

    #[test]
    fn reads_on_and_off() {
        assert!(OscArg::Int(1).is_on());
        assert!(!OscArg::Int(0).is_on());
        assert!(OscArg::Float(0.5).is_on());
        assert!(!OscArg::Bool(false).is_on());
        assert!(OscArg::Str("on".into()).is_on());
        assert!(!OscArg::Str("false".into()).is_on());
    }

    #[test]
    fn encodes_and_sends_over_udp() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let sender = OscSender::open("127.0.0.1", receiver.local_addr().unwrap().port()).unwrap();

        let message = OscMessage::new(
            "/live/clip",
            vec![OscArg::Int(0), OscArg::Float(0.5), OscArg::Str("x".into())],
        );
        assert!(sender.send(&message));

        let mut buffer = [0u8; 256];
        let (size, _) = receiver.recv_from(&mut buffer).unwrap();
        let (_, packet) = rosc::decoder::decode_udp(&buffer[..size]).unwrap();
        let OscPacket::Message(received) = packet else {
            panic!("message expected")
        };
        assert_eq!(received.addr, "/live/clip");
        assert_eq!(
            received.args,
            vec![OscType::Int(0), OscType::Float(0.5), OscType::String("x".into())]
        );
    }
}
