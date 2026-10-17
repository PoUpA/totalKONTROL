// Pure helpers for editing a profile: reading and writing a control's action,
// summarising it, and keeping bank references right when banks move.

import {
  BUTTON_LABELS,
  type Bank,
  type ButtonAction,
  type ControlRef,
  type EncoderAction,
  type KnobAction,
  type Profile,
  type XyAction,
} from "../../types";
import type { Translate } from "../../i18n";

export type AnyAction = ButtonAction | KnobAction | XyAction | EncoderAction;

export function sameControl(a: ControlRef | null, b: ControlRef | null): boolean {
  return a !== null && b !== null && JSON.stringify(a) === JSON.stringify(b);
}

export function controlLabel(control: ControlRef, t: Translate): string {
  switch (control.kind) {
    case "pad":
      return t("control.pad", { pad: control.pad });
    case "button":
      return BUTTON_LABELS[control.button];
    case "knob":
      return t("control.knob", { knob: control.knob });
    case "xy":
      return t("control.xy");
    case "xyTouch":
      return t("control.xyTouch");
    case "encoder":
      return t("control.encoder");
  }
}

/** Path of the control in validation errors, e.g. `banks[0].pads.3`. */
export function controlPath(bank: number, control: ControlRef): string {
  switch (control.kind) {
    case "pad":
      return `banks[${bank}].pads.${control.pad}`;
    case "button":
      return `banks[${bank}].buttons.${control.button}`;
    case "knob":
      return `banks[${bank}].knobs.${control.knob}`;
    case "xy":
      return `banks[${bank}].xy`;
    case "xyTouch":
      return `banks[${bank}].xyTouch`;
    case "encoder":
      return `banks[${bank}].encoder`;
  }
}

export function hasError(errors: string[], bank: number, control: ControlRef): boolean {
  const path = controlPath(bank, control);
  return errors.some((e) => e.startsWith(path + " ") || e.startsWith(path + "."));
}

export function getAction(bank: Bank, control: ControlRef): AnyAction | undefined {
  switch (control.kind) {
    case "pad":
      return bank.pads?.[String(control.pad)];
    case "button":
      return bank.buttons?.[control.button];
    case "knob":
      return bank.knobs?.[String(control.knob)];
    case "xy":
      return bank.xy;
    case "xyTouch":
      return bank.xyTouch;
    case "encoder":
      return bank.encoder;
  }
}

/** Copy of the bank with the control's action replaced, or removed when undefined. */
export function setAction(bank: Bank, control: ControlRef, action: AnyAction | undefined): Bank {
  const withEntry = <T>(map: Record<string, T> | undefined, key: string, value: T | undefined) => {
    const next = { ...map };
    if (value === undefined) delete next[key];
    else next[key] = value;
    return Object.keys(next).length > 0 ? next : undefined;
  };
  switch (control.kind) {
    case "pad":
      return { ...bank, pads: withEntry(bank.pads, String(control.pad), action as ButtonAction | undefined) };
    case "button":
      return {
        ...bank,
        buttons: withEntry(bank.buttons as Record<string, ButtonAction>, control.button, action as ButtonAction),
      };
    case "knob":
      return { ...bank, knobs: withEntry(bank.knobs, String(control.knob), action as KnobAction | undefined) };
    case "xy":
      return { ...bank, xy: action as XyAction | undefined };
    case "xyTouch":
      return { ...bank, xyTouch: action as ButtonAction | undefined };
    case "encoder":
      return { ...bank, encoder: action as EncoderAction | undefined };
  }
}

const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/** MIDI note name, with middle C (60) as C3 like most DAWs. */
export function noteName(note: number): string {
  return `${NOTE_NAMES[note % 12]}${Math.floor(note / 12) - 2}`;
}

/** Short text shown on a control in the editor's front panel. */
export function summarize(action: AnyAction | undefined, banks: Bank[], t: Translate): string {
  if (!action) return "";
  const channel = "channel" in action ? t("summary.channel", { channel: action.channel }) : "";
  switch (action.type) {
    case "note":
      return `${noteName(action.note)} · ${channel}`;
    case "cc":
      return `CC ${action.cc} · ${channel}`;
    case "programChange":
      return `PC ${action.program} · ${channel}`;
    case "switchBank":
      return `→ ${banks[action.bank]?.name ?? t("summary.bank", { bank: action.bank })}`;
    case "xy":
      return `CC ${action.ccX}/${action.ccY} · ${channel}`;
    case "osc":
      return `OSC ${action.address}`;
    case "key":
      return "right" in action ? `⌨ ${action.left} / ${action.right}` : `⌨ ${action.keys}`;
    case "roll":
      return t("action.roll");
    case "flam":
      return t("action.flam");
  }
}

/** Applies `f` to every bank reference (switchBank actions and startBank). */
function remapBanks(profile: Profile, f: (bank: number) => number): Profile {
  const remap = (action: ButtonAction): ButtonAction =>
    action.type === "switchBank" ? { ...action, bank: f(action.bank) } : action;
  const remapAll = (map: Record<string, ButtonAction> | undefined) =>
    map && Object.fromEntries(Object.entries(map).map(([key, action]) => [key, remap(action)]));
  return {
    ...profile,
    startBank: profile.startBank === undefined ? undefined : f(profile.startBank),
    banks: profile.banks.map((bank) => ({
      ...bank,
      pads: remapAll(bank.pads),
      buttons: remapAll(bank.buttons as Record<string, ButtonAction> | undefined),
    })),
  };
}

export function addBank(profile: Profile, t: Translate): Profile {
  const name = t("editor.newBankName", { number: profile.banks.length + 1 });
  return { ...profile, banks: [...profile.banks, { name }] };
}

/** Removes a bank. References to later banks shift down; references to the
 *  removed bank are left pointing past the end, so validation flags them. */
export function removeBank(profile: Profile, index: number): Profile {
  const removed = { ...profile, banks: profile.banks.filter((_, i) => i !== index) };
  const count = removed.banks.length;
  const next = remapBanks(removed, (bank) => (bank > index ? bank - 1 : bank === index ? count : bank));
  if (next.startBank !== undefined && next.startBank >= count) next.startBank = 0;
  return next;
}

/** Swaps two banks and every reference to them. */
export function swapBanks(profile: Profile, a: number, b: number): Profile {
  const banks = [...profile.banks];
  [banks[a], banks[b]] = [banks[b], banks[a]];
  return remapBanks({ ...profile, banks }, (bank) => (bank === a ? b : bank === b ? a : bank));
}
