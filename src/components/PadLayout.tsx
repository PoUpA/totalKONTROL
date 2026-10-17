import type { ReactNode } from "react";
import { useI18n } from "../i18n";
import { BUTTON_LABELS, type ButtonId, type ControlRef } from "../types";

export type RenderControl = (control: ControlRef, label: string, className: string) => ReactNode;

// Groups of buttons as laid out on the hardware, from top to bottom.
const PARAMETER_BUTTONS: ButtonId[][] = [
  ["x", "y", "pedal"],
  ["noteCc", "midiCh", "swType"],
  ["relVal", "velocity", "port"],
];
const PAD_MODE_BUTTONS: ButtonId[] = ["hold", "flam", "roll"];
const STRIP_BUTTONS: ButtonId[] = ["scene", "message", "fixedVelocity", "progChange"];

const PAD_ROWS = [
  [1, 2, 3, 4],
  [5, 6, 7, 8],
  [9, 10, 11, 12],
  [13, 14, 15, 16],
];

type Props = {
  /** Text of the 3-character LED display. */
  display: string;
  render: RenderControl;
};

/** Picture of the padKONTROL front panel; each control is drawn by `render`. */
export function PadLayout({ display, render }: Props) {
  const { t } = useI18n();
  const button = (id: ButtonId) => render({ kind: "button", button: id }, BUTTON_LABELS[id], "btn");

  return (
    <div className="pk">
      <div className="pk-left">
        <div className="pk-display" aria-label={t("control.display")}>
          {display.padEnd(3, " ").slice(0, 3)}
        </div>
        <div className="pk-encoder-row">
          {render({ kind: "encoder" }, t("control.encoder"), "dial encoder")}
          {button("setting")}
        </div>
        <div className="pk-parameters">{PARAMETER_BUTTONS.flat().map(button)}</div>
        <div className="pk-xy-row">
          {render({ kind: "xy" }, t("control.xy"), "xy")}
          <div className="pk-column">
            {PAD_MODE_BUTTONS.map(button)}
            {/* Not a hardware button: the finger touching the XY pad. */}
            {render({ kind: "xyTouch" }, t("control.xyTouchShort"), "btn touch")}
          </div>
        </div>
      </div>

      <div className="pk-strip">{STRIP_BUTTONS.map(button)}</div>

      <div className="pk-right">
        <div className="pk-knobs">
          {button("knob1Assign")}
          {render({ kind: "knob", knob: 1 }, t("control.knobShort", { knob: 1 }), "dial knob")}
          {button("knob2Assign")}
          {render({ kind: "knob", knob: 2 }, t("control.knobShort", { knob: 2 }), "dial knob")}
        </div>
        <div className="pk-pads">{PAD_ROWS.flat().map((pad) => render({ kind: "pad", pad }, String(pad), "pad"))}</div>
      </div>
    </div>
  );
}
