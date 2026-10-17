import type { Translate } from "./i18n";

export type MidiPort = { name: string; isPadKontrol: boolean };
export type MidiPorts = {
  inputs: MidiPort[];
  outputs: MidiPort[];
  virtualSupported: boolean;
  /** Why virtual ports are not offered, when there is a specific reason. */
  virtualNote: string | null;
  virtualOutputName: string;
  virtualFeedbackName: string;
};

export type ProfileSummary = { file: string; name: string | null; bankCount: number; errors: string[] };

/** An existing port, or a virtual port created by the app. */
export type PortChoice = { port: string } | "virtual";

export type ConnectOptions = {
  input: string;
  output: string | null;
  nativeMode: boolean;
  dawOutput: PortChoice | null;
  feedbackInput: PortChoice | null;
  osc: OscOptions | null;
};

/** OSC destination of a connection. */
export type OscOptions = {
  host: string;
  port: number;
  /** Address prefix of the mirror messages, e.g. "/totalkontrol". */
  prefix: string;
  /** Send every control to <prefix>/pad/3, <prefix>/knob/1… */
  mirror: boolean;
  /** UDP port to listen on for LED, display and bank messages. */
  listenPort: number | null;
};

export type ConnectionInfo = {
  input: string;
  output: string | null;
  nativeMode: boolean;
  dawOutput: string | null;
  feedbackInput: string | null;
  /** OSC destination, as "host:port". */
  osc: string | null;
  /** UDP port the app listens on for OSC. */
  oscListen: number | null;
};

export type EngineState = {
  file: string;
  profile: string;
  bank: number;
  banks: string[];
  display: string;
  /** Encoder position in the active bank: 0 to 127 in absolute mode, -99 to 999 in relative mode. */
  encoder: number;
};

// Profile format, mirroring src-tauri/profiles/profile.schema.json. Optional
// fields take the schema's default when missing.

export type Mode = "momentary" | "toggle";
export type Led = "none" | "press" | "toggle" | "follow";

export type ButtonAction =
  | { type: "note"; channel: number; note: number; velocity?: number | "pad"; mode?: Mode; led?: Led }
  | { type: "cc"; channel: number; cc: number; onValue?: number; offValue?: number; mode?: Mode; led?: Led }
  | { type: "programChange"; channel: number; program: number; led?: Led }
  | { type: "switchBank"; bank: number; momentary?: boolean }
  | { type: "osc"; address: string; args?: OscValue[]; releaseArgs?: OscValue[]; mode?: Mode; led?: Led };

/** OSC argument; the string "$velocity" stands for the velocity of the hit. */
export type OscValue = number | boolean | string;

export type Encoding = "relative" | "absolute";

export type KnobAction =
  | { type: "cc"; channel: number; cc: number; min?: number; max?: number }
  | { type: "osc"; address: string; min?: number; max?: number; float?: boolean };
export type XyAction = { type: "xy"; channel: number; ccX: number; ccY: number } | { type: "osc"; address: string };
export type EncoderAction =
  | { type: "cc"; channel: number; cc: number; encoding?: Encoding }
  | { type: "osc"; address: string; encoding?: Encoding };

export type Bank = {
  name: string;
  /** Text of the 3-character display; defaults to the start of the name. */
  display?: string;
  pads?: Record<string, ButtonAction>;
  buttons?: Partial<Record<ButtonId, ButtonAction>>;
  knobs?: Record<string, KnobAction>;
  xy?: XyAction;
  encoder?: EncoderAction;
  /** Finger landing on / leaving the XY pad, like a button without an LED. */
  xyTouch?: ButtonAction;
};

export type Profile = {
  $schema?: string;
  version: 1;
  name: string;
  ports?: { input?: string; output?: string };
  startBank?: number;
  display?: DisplaySettings;
  banks: Bank[];
};

export type DisplaySettings = {
  knobs?: boolean;
  encoder?: boolean;
  pads?: "none" | "note" | "velocity";
  /** Milliseconds a value stays before the bank comes back. */
  duration?: number;
};

/** A control of the padKONTROL, as selected in the profile editor. */
export type ControlRef =
  | { kind: "pad"; pad: number }
  | { kind: "button"; button: ButtonId }
  | { kind: "knob"; knob: number }
  | { kind: "xy" }
  | { kind: "xyTouch" }
  | { kind: "encoder" };

export function controlOfEvent(event: ControlEvent): ControlRef {
  switch (event.kind) {
    case "pad":
      return { kind: "pad", pad: event.pad };
    case "button":
      return { kind: "button", button: event.button };
    case "knob":
      return { kind: "knob", knob: event.knob };
    case "xy":
      return { kind: "xy" };
    case "xyTouch":
      return { kind: "xyTouch" };
    case "encoder":
      return { kind: "encoder" };
  }
}

export type ButtonId =
  | "scene"
  | "message"
  | "setting"
  | "noteCc"
  | "midiCh"
  | "swType"
  | "relVal"
  | "velocity"
  | "port"
  | "fixedVelocity"
  | "progChange"
  | "x"
  | "y"
  | "knob1Assign"
  | "knob2Assign"
  | "pedal"
  | "roll"
  | "flam"
  | "hold";

export type ControlEvent =
  | { kind: "pad"; pad: number; pressed: boolean; velocity: number }
  | { kind: "button"; button: ButtonId; pressed: boolean }
  | { kind: "knob"; knob: number; value: number }
  | { kind: "xy"; x: number; y: number }
  | { kind: "xyTouch"; pressed: boolean }
  | { kind: "encoder"; delta: number };

export type MidiMessage = {
  receivedAt: number;
  direction: "in" | "out" | "feedback" | "osc" | "oscIn";
  hex: string;
  description: string;
  event: ControlEvent | null;
  /** Encoder position kept by the engine, for encoder events it handled. */
  encoderPosition: number | null;
};

/** Monitor entry, with a stable id for React keys. */
export type MonitorEntry = MidiMessage & { id: number };

export const BUTTON_LABELS: Record<ButtonId, string> = {
  scene: "SCENE",
  message: "MESSAGE",
  setting: "SETTING",
  noteCc: "NOTE/CC#",
  midiCh: "MIDI CH",
  swType: "SW TYPE",
  relVal: "REL. VAL",
  velocity: "VELOCITY",
  port: "PORT",
  fixedVelocity: "FIXED VEL",
  progChange: "PROG CHANGE",
  x: "X",
  y: "Y",
  knob1Assign: "KNOB 1 ASSIGN",
  knob2Assign: "KNOB 2 ASSIGN",
  pedal: "PEDAL",
  roll: "ROLL",
  flam: "FLAM",
  hold: "HOLD",
};

export function describeEvent(event: ControlEvent, t: Translate): string {
  switch (event.kind) {
    case "pad":
      return event.pressed
        ? t("event.padDown", { pad: event.pad, velocity: event.velocity })
        : t("event.padUp", { pad: event.pad });
    case "button":
      return `${BUTTON_LABELS[event.button]} ${event.pressed ? "↓" : "↑"}`;
    case "knob":
      return t("event.knob", { knob: event.knob, value: event.value });
    case "xy":
      return `XY  x ${event.x}  y ${event.y}`;
    case "xyTouch":
      return `${t("control.xyTouch")} ${event.pressed ? "↓" : "↑"}`;
    case "encoder":
      return t("event.encoder", { delta: `${event.delta > 0 ? "+" : ""}${event.delta}` });
  }
}
