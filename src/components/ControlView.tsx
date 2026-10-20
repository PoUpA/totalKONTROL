import { useI18n } from "../i18n";
import { BUTTON_LABELS, type ButtonId, type ControlRef } from "../types";
import { PadLayout } from "./PadLayout";

export type ControlState = {
  pads: Record<number, number>; // pad → velocity, present while it is held down
  buttons: Partial<Record<ButtonId, boolean>>;
  knobs: Record<number, number>;
  xy: { x: number; y: number } | null;
  /** A finger is on the XY pad. */
  xyTouched: boolean;
  encoder: number;
  seen: Set<string>;
};

export const initialControlState = (): ControlState => ({
  pads: {},
  buttons: {},
  knobs: {},
  xy: null,
  xyTouched: false,
  encoder: 0,
  seen: new Set(),
});

// Pads, buttons, knobs, XY pad, XY touch, encoder.
const TOTAL_CONTROLS = 16 + Object.keys(BUTTON_LABELS).length + 2 + 1 + 1 + 1;

/** Key of a control in `ControlState.seen`. */
function seenKey(control: ControlRef): string {
  switch (control.kind) {
    case "pad":
      return `pad${control.pad}`;
    case "button":
      return control.button;
    case "knob":
      return `knob${control.knob}`;
    case "xy":
      return "xy";
    case "xyTouch":
      return "xyTouch";
    case "encoder":
      return "encoder";
  }
}

/**
 * `leds`: keys (as in `ControlState.seen`) of the LEDs lit on the padKONTROL.
 * `labels`: what each control does in the bank playing, by the same keys.
 */
export function ControlView({
  state,
  display,
  leds,
  labels,
}: {
  state: ControlState;
  display: string;
  leds: Set<string>;
  labels: Record<string, string>;
}) {
  const { t, tn } = useI18n();
  return (
    <section className="card">
      <div className="card-header">
        <h2>{t("controls.title")}</h2>
        <span className="muted">{tn("controls.seen", state.seen.size, { total: TOTAL_CONTROLS })}</span>
      </div>

      <PadLayout
        display={display}
        render={(control, label, className) => {
          const lit = leds.has(seenKey(control));
          const described = labels[seenKey(control)];
          const title = [described, lit ? t("controls.ledOn") : ""].filter(Boolean).join(" · ") || undefined;
          const seen = (state.seen.has(seenKey(control)) ? " seen" : "") + (lit ? " lit" : "");
          switch (control.kind) {
            case "pad": {
              const velocity = state.pads[control.pad];
              const on = velocity !== undefined;
              return (
                <div
                  key={label}
                  className={`${className}${on ? " on" : ""}${seen}`}
                  style={on ? { opacity: 0.4 + (velocity / 127) * 0.6 } : undefined}
                  title={title}
                  aria-label={title ? `${label}, ${title}` : undefined}
                >
                  <span>{label}</span>
                  {on ? <small>{velocity}</small> : described && <small className="label">{described}</small>}
                </div>
              );
            }
            case "button":
              return (
                <span
                  key={label}
                  className={`${className}${state.buttons[control.button] ? " on" : ""}${seen}`}
                  title={title}
                  aria-label={title ? `${label}, ${title}` : undefined}
                >
                  {label}
                </span>
              );
            case "xyTouch":
              return (
                <span key={label} className={`${className}${state.xyTouched ? " on" : ""}${seen}`}>
                  {label}
                </span>
              );
            case "knob":
              return (
                <div key={label} className={`${className}${seen}`} title={title}>
                  <span className="muted">{label}</span>
                  <meter min={0} max={127} value={state.knobs[control.knob] ?? 0} />
                  <strong>{state.knobs[control.knob] ?? "–"}</strong>
                </div>
              );
            case "encoder":
              return (
                <div key={label} className={`${className}${seen}`} title={title}>
                  <span className="muted">{label}</span>
                  <strong>{state.encoder}</strong>
                </div>
              );
            case "xy":
              return (
                <div key={label} className={`${className}${seen}`} title={title}>
                  {state.xy && (
                    <div
                      className="xy-dot"
                      style={{ left: `${(state.xy.x / 127) * 100}%`, bottom: `${(state.xy.y / 127) * 100}%` }}
                    />
                  )}
                  <span className="muted">{state.xy ? `${state.xy.x}, ${state.xy.y}` : label}</span>
                </div>
              );
          }
        }}
      />
    </section>
  );
}
