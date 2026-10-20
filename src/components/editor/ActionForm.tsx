import { useEffect, useState } from "react";
import type {
  Bank,
  ProfileSummary,
  ButtonAction,
  ControlRef,
  EncoderAction,
  Encoding,
  KnobAction,
  Led,
  Mode,
  OscValue,
  XyAction,
} from "../../types";
import { useI18n, type Translate } from "../../i18n";
import { NumberInput } from "../NumberInput";
import { KeysField } from "./KeysField";
import { controlLabel, noteName, type AnyAction } from "./profile";

type Props = {
  control: ControlRef;
  action: AnyAction | undefined;
  banks: Bank[];
  /** Profiles a switchProfile action can pick. */
  profiles: ProfileSummary[];
  errors: string[];
  onChange: (action: AnyAction | undefined) => void;
};

/** Integer field. Keeps the last valid value while the user types. */
function NumberField({
  label,
  value,
  min,
  max,
  onChange,
  hint,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  hint?: string;
}) {
  return (
    <label>
      {label}
      <span className="field-row">
        <NumberInput min={min} max={max} value={value} onChange={onChange} />
        {hint && <span className="muted">{hint}</span>}
      </span>
    </label>
  );
}

/** Decimal field, for OSC ranges. */
function DecimalField({ label, value, onChange }: { label: string; value: number; onChange: (value: number) => void }) {
  return (
    <label>
      {label}
      <span className="field-row">
        <NumberInput decimal value={value} onChange={onChange} />
      </span>
    </label>
  );
}

function TextField({
  label,
  value,
  onChange,
  placeholder,
  hint,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  hint?: string;
}) {
  return (
    <label>
      {label}
      <input
        className="text-input"
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
      />
      {hint && <span className="muted hint">{hint}</span>}
    </label>
  );
}

function SelectField<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
}) {
  return (
    <label>
      {label}
      <select value={value} onChange={(e) => onChange(e.target.value as T)}>
        {options.map(([v, text]) => (
          <option key={v} value={v}>
            {text}
          </option>
        ))}
      </select>
    </label>
  );
}

/** OSC arguments as typed: `1 0.5 true "some text" $velocity`. */
export function parseOscArgs(text: string): OscValue[] {
  const tokens = text.match(/"[^"]*"|\S+/g) ?? [];
  return tokens.map((token) => {
    if (token.startsWith('"') && token.endsWith('"') && token.length >= 2) return token.slice(1, -1);
    if (token === "true") return true;
    if (token === "false") return false;
    if (/^-?\d+$/.test(token)) return Number.parseInt(token, 10);
    if (/^-?(\d+\.\d*|\.\d+)$/.test(token)) return Number.parseFloat(token);
    return token;
  });
}

export function formatOscArgs(args: OscValue[]): string {
  return args.map((a) => (typeof a === "string" && /\s|^$/.test(a) ? `"${a}"` : String(a))).join(" ");
}

/** Text field holding OSC arguments. Keeps what is typed while it parses. */
function OscArgsField({
  label,
  args,
  onChange,
  hint,
}: {
  label: string;
  args: OscValue[];
  onChange: (args: OscValue[]) => void;
  hint?: string;
}) {
  const [text, setText] = useState(() => formatOscArgs(args));
  const formatted = formatOscArgs(args);
  // Another control or an undo replaced the arguments: show them.
  useEffect(() => {
    if (formatOscArgs(parseOscArgs(text)) !== formatted) setText(formatted);
  }, [formatted]);
  return (
    <TextField
      label={label}
      value={text}
      hint={hint}
      onChange={(next) => {
        setText(next);
        onChange(parseOscArgs(next));
      }}
    />
  );
}

const modeOptions = (t: Translate): [Mode, string][] => [
  ["momentary", t("action.modeMomentary")],
  ["toggle", t("action.modeToggle")],
];

const ledOptions = (t: Translate): [Led | "", string][] => [
  ["", t("action.ledDefault")],
  ["none", t("action.ledNone")],
  ["press", t("action.ledPress")],
  ["toggle", t("action.ledToggle")],
  ["follow", t("action.ledFollow")],
];

const encodingOptions = (t: Translate): [Encoding, string][] => [
  ["relative", t("action.relative")],
  ["absolute", t("action.absolute")],
];

/** Copy of `object` with `key` set, or removed when it equals the default. */
function withField<T extends object, K extends keyof T>(object: T, key: K, value: T[K], fallback?: T[K]): T {
  const next = { ...object };
  if (value === undefined || value === fallback) delete next[key];
  else next[key] = value;
  return next;
}

const DEFAULT_OSC_ADDRESS = "/totalkontrol";

type ButtonType = ButtonAction["type"] | "";

function newButtonAction(type: ButtonType, control: ControlRef, previous?: ButtonAction): ButtonAction | undefined {
  const channel = previous && "channel" in previous ? previous.channel : 1;
  switch (type) {
    case "note":
      return { type, channel, note: control.kind === "pad" ? 35 + control.pad : 60 };
    case "cc":
      return { type, channel, cc: 20 };
    case "programChange":
      return { type, channel, program: 0 };
    case "switchBank":
      return { type, bank: 0 };
    case "switchProfile":
      return { type, profile: "" };
    case "key":
      return { type, keys: "space" };
    case "roll":
    case "flam":
      return { type };
    case "osc":
      return control.kind === "pad"
        ? { type, address: `${DEFAULT_OSC_ADDRESS}/pad/${control.pad}`, args: ["$velocity"] }
        : {
            type,
            address: `${DEFAULT_OSC_ADDRESS}/${control.kind === "button" ? control.button : "xy/touch"}`,
            args: [1],
          };
    case "":
      return undefined;
  }
}

function ButtonFields({
  control,
  action,
  banks,
  profiles,
  onChange,
}: {
  control: ControlRef;
  action: ButtonAction | undefined;
  banks: Bank[];
  profiles: ProfileSummary[];
  onChange: (action: ButtonAction | undefined) => void;
}) {
  const { t } = useI18n();
  const isPad = control.kind === "pad";
  const type: ButtonType = action?.type ?? "";

  return (
    <>
      <SelectField<ButtonType>
        label={t("action.type")}
        value={type}
        options={[
          ["", t("common.noneFem")],
          ["note", t("action.note")],
          ["cc", t("action.cc")],
          ["programChange", t("action.programChange")],
          ["switchBank", t("action.switchBank")],
          ["switchProfile", t("action.switchProfile")],
          ["osc", t("action.osc")],
          ["key", t("action.key")],
          ["roll", t("action.roll")],
          ["flam", t("action.flam")],
        ]}
        onChange={(next) => onChange(newButtonAction(next, control, action))}
      />

      {action && "channel" in action && (
        <NumberField
          label={t("action.channel")}
          value={action.channel}
          min={1}
          max={16}
          onChange={(channel) => onChange({ ...action, channel })}
        />
      )}

      {action?.type === "note" && (
        <>
          <NumberField
            label={t("action.note")}
            value={action.note}
            min={0}
            max={127}
            hint={noteName(action.note)}
            onChange={(note) => onChange({ ...action, note })}
          />
          {isPad && (
            <SelectField
              label={t("action.velocity")}
              value={typeof action.velocity === "number" ? "fixed" : "pad"}
              options={[
                ["pad", t("action.velocityFromHit")],
                ["fixed", t("action.velocityFixed")],
              ]}
              onChange={(kind) => onChange(withField(action, "velocity", kind === "fixed" ? 127 : undefined))}
            />
          )}
          {(!isPad || typeof action.velocity === "number") && (
            <NumberField
              label={isPad ? t("action.fixedVelocity") : t("action.velocity")}
              value={typeof action.velocity === "number" ? action.velocity : 127}
              min={1}
              max={127}
              onChange={(velocity) => onChange(withField(action, "velocity", velocity, isPad ? undefined : 127))}
            />
          )}
        </>
      )}

      {action?.type === "cc" && (
        <>
          <NumberField
            label={t("action.ccNumber")}
            value={action.cc}
            min={0}
            max={127}
            onChange={(cc) => onChange({ ...action, cc })}
          />
          <NumberField
            label={t("action.onValue")}
            value={action.onValue ?? 127}
            min={0}
            max={127}
            onChange={(v) => onChange(withField(action, "onValue", v, 127))}
          />
          <NumberField
            label={t("action.offValue")}
            value={action.offValue ?? 0}
            min={0}
            max={127}
            onChange={(v) => onChange(withField(action, "offValue", v, 0))}
          />
        </>
      )}

      {action?.type === "programChange" && (
        <NumberField
          label={t("action.program")}
          value={action.program}
          min={0}
          max={127}
          onChange={(program) => onChange({ ...action, program })}
        />
      )}

      {action?.type === "switchBank" && (
        <>
          <SelectField
            label={t("action.bank")}
            value={String(action.bank)}
            options={banks.map((b, i) => [String(i), `${i} · ${b.name}`])}
            onChange={(bank) => onChange({ ...action, bank: Number(bank) })}
          />
          <label className="checkbox">
            <input
              type="checkbox"
              checked={action.momentary ?? false}
              onChange={(e) => onChange(withField(action, "momentary", e.target.checked, false))}
            />
            {t("action.momentaryBank")}
          </label>
        </>
      )}

      {action?.type === "switchProfile" && (
        <SelectField
          label={t("action.profile")}
          value={action.profile}
          options={[
            ["", t("action.pickProfile")],
            ...profiles.map((p): [string, string] => [p.file, p.name ?? p.file]),
          ]}
          onChange={(profile) => onChange({ ...action, profile })}
        />
      )}

      {action?.type === "key" && (
        <>
          <KeysField label={t("action.keys")} value={action.keys} onChange={(keys) => onChange({ ...action, keys })} />
          <label className="checkbox">
            <input
              type="checkbox"
              checked={action.hold ?? false}
              onChange={(e) => onChange(withField(action, "hold", e.target.checked, false))}
            />
            {t("action.keysHold")}
          </label>
          <p className="hint">{t("action.keysHint")}</p>
        </>
      )}

      {(action?.type === "roll" || action?.type === "flam") && (
        <p className="hint">{t(action.type === "roll" ? "action.rollHint" : "action.flamHint")}</p>
      )}

      {action?.type === "osc" && (
        <>
          <TextField
            label={t("action.oscAddress")}
            value={action.address}
            placeholder="/live/clip"
            onChange={(address) => onChange({ ...action, address })}
          />
          <OscArgsField
            label={t("action.oscArgs")}
            args={action.args ?? []}
            hint={t("action.oscArgsHint")}
            onChange={(args) => onChange(withField(action, "args", args.length > 0 ? args : undefined))}
          />
          <label className="checkbox">
            <input
              type="checkbox"
              checked={action.releaseArgs !== undefined}
              onChange={(e) => onChange(withField(action, "releaseArgs", e.target.checked ? [0] : undefined))}
            />
            {t("action.oscSendOnRelease")}
          </label>
          {action.releaseArgs !== undefined && (
            <OscArgsField
              label={t("action.oscReleaseArgs")}
              args={action.releaseArgs}
              onChange={(releaseArgs) => onChange({ ...action, releaseArgs })}
            />
          )}
        </>
      )}

      {(action?.type === "note" || action?.type === "cc" || action?.type === "osc") && (
        <SelectField
          label={t("action.mode")}
          value={action.mode ?? "momentary"}
          options={modeOptions(t)}
          onChange={(mode) => onChange(withField(action, "mode", mode, "momentary"))}
        />
      )}

      {/* The XY pad has no LED. */}
      {action &&
        action.type !== "switchBank" &&
        action.type !== "switchProfile" &&
        action.type !== "roll" &&
        action.type !== "flam" &&
        control.kind !== "xyTouch" && (
          <SelectField<Led | "">
            label={t("action.led")}
            value={action.led ?? ""}
            options={ledOptions(t)}
            onChange={(led) => onChange(withField(action, "led", led === "" ? undefined : led))}
          />
        )}
    </>
  );
}

function KnobFields({
  control,
  action,
  onChange,
}: {
  control: ControlRef;
  action: KnobAction | undefined;
  onChange: (a: KnobAction | undefined) => void;
}) {
  const { t } = useI18n();
  const knob = control.kind === "knob" ? control.knob : 1;
  return (
    <>
      <SelectField
        label={t("action.type")}
        value={action?.type ?? ""}
        options={[
          ["", t("common.noneFem")],
          ["cc", t("action.cc")],
          ["osc", t("action.osc")],
        ]}
        onChange={(type) =>
          onChange(
            type === "cc"
              ? { type: "cc", channel: 1, cc: knob }
              : type === "osc"
                ? { type: "osc", address: `${DEFAULT_OSC_ADDRESS}/knob/${knob}` }
                : undefined,
          )
        }
      />
      {action?.type === "cc" && (
        <>
          <NumberField
            label={t("action.channel")}
            value={action.channel}
            min={1}
            max={16}
            onChange={(channel) => onChange({ ...action, channel })}
          />
          <NumberField
            label={t("action.ccNumber")}
            value={action.cc}
            min={0}
            max={127}
            onChange={(cc) => onChange({ ...action, cc })}
          />
          <NumberField
            label={t("action.min")}
            value={action.min ?? 0}
            min={0}
            max={127}
            onChange={(v) => onChange(withField(action, "min", v, 0))}
          />
          <NumberField
            label={t("action.max")}
            value={action.max ?? 127}
            min={0}
            max={127}
            onChange={(v) => onChange(withField(action, "max", v, 127))}
          />
        </>
      )}
      {action?.type === "osc" && (
        <>
          <TextField
            label={t("action.oscAddress")}
            value={action.address}
            onChange={(address) => onChange({ ...action, address })}
          />
          <DecimalField
            label={t("action.min")}
            value={action.min ?? 0}
            onChange={(v) => onChange(withField(action, "min", v, 0))}
          />
          <DecimalField
            label={t("action.max")}
            value={action.max ?? 127}
            onChange={(v) => onChange(withField(action, "max", v, 127))}
          />
          <label className="checkbox">
            <input
              type="checkbox"
              checked={action.float ?? false}
              onChange={(e) => onChange(withField(action, "float", e.target.checked, false))}
            />
            {t("action.oscFloat")}
          </label>
        </>
      )}
      {action && (
        <label className="checkbox">
          <input
            type="checkbox"
            checked={action.pickup ?? false}
            onChange={(e) => onChange(withField(action, "pickup", e.target.checked, false))}
          />
          {t("action.pickup")}
        </label>
      )}
    </>
  );
}

function XyFields({ action, onChange }: { action: XyAction | undefined; onChange: (a: XyAction | undefined) => void }) {
  const { t } = useI18n();
  return (
    <>
      <SelectField
        label={t("action.type")}
        value={action?.type ?? ""}
        options={[
          ["", t("common.noneFem")],
          ["xy", t("action.xyTwoCc")],
          ["osc", t("action.oscXy")],
        ]}
        onChange={(type) =>
          onChange(
            type === "xy"
              ? { type: "xy", channel: 1, ccX: 3, ccY: 4 }
              : type === "osc"
                ? { type: "osc", address: `${DEFAULT_OSC_ADDRESS}/xy` }
                : undefined,
          )
        }
      />
      {action?.type === "xy" && (
        <>
          <NumberField
            label={t("action.channel")}
            value={action.channel}
            min={1}
            max={16}
            onChange={(channel) => onChange({ ...action, channel })}
          />
          <NumberField
            label={t("action.ccX")}
            value={action.ccX}
            min={0}
            max={127}
            onChange={(ccX) => onChange({ ...action, ccX })}
          />
          <NumberField
            label={t("action.ccY")}
            value={action.ccY}
            min={0}
            max={127}
            onChange={(ccY) => onChange({ ...action, ccY })}
          />
        </>
      )}
      {action?.type === "osc" && (
        <TextField
          label={t("action.oscAddress")}
          value={action.address}
          onChange={(address) => onChange({ ...action, address })}
        />
      )}
    </>
  );
}

function EncoderFields({
  action,
  onChange,
}: {
  action: EncoderAction | undefined;
  onChange: (a: EncoderAction | undefined) => void;
}) {
  const { t } = useI18n();
  return (
    <>
      <SelectField
        label={t("action.type")}
        value={action?.type ?? ""}
        options={[
          ["", t("common.noneFem")],
          ["cc", t("action.cc")],
          ["osc", t("action.osc")],
          ["key", t("action.key")],
        ]}
        onChange={(type) =>
          onChange(
            type === "cc"
              ? { type: "cc", channel: 1, cc: 10 }
              : type === "osc"
                ? { type: "osc", address: `${DEFAULT_OSC_ADDRESS}/encoder` }
                : type === "key"
                  ? { type: "key", left: "left", right: "right" }
                  : undefined,
          )
        }
      />
      {action?.type === "cc" && (
        <>
          <NumberField
            label={t("action.channel")}
            value={action.channel}
            min={1}
            max={16}
            onChange={(channel) => onChange({ ...action, channel })}
          />
          <NumberField
            label={t("action.ccNumber")}
            value={action.cc}
            min={0}
            max={127}
            onChange={(cc) => onChange({ ...action, cc })}
          />
        </>
      )}
      {action?.type === "osc" && (
        <TextField
          label={t("action.oscAddress")}
          value={action.address}
          onChange={(address) => onChange({ ...action, address })}
        />
      )}
      {action?.type === "key" && (
        <>
          <KeysField
            label={t("action.keysLeft")}
            value={action.left}
            onChange={(left) => onChange({ ...action, left })}
          />
          <KeysField
            label={t("action.keysRight")}
            value={action.right}
            onChange={(right) => onChange({ ...action, right })}
          />
        </>
      )}
      {action && action.type !== "key" && (
        <>
          <SelectField
            label={t("action.encoding")}
            value={action.encoding ?? "relative"}
            options={encodingOptions(t)}
            onChange={(encoding) => onChange(withField(action, "encoding", encoding, "relative"))}
          />
          <label className="checkbox">
            <input
              type="checkbox"
              checked={action.acceleration ?? false}
              onChange={(e) => onChange(withField(action, "acceleration", e.target.checked, false))}
            />
            {t("action.acceleration")}
          </label>
        </>
      )}
    </>
  );
}

export function ActionForm({ control, action, banks, profiles, errors, onChange }: Props) {
  const { t } = useI18n();
  return (
    <div className="action-form">
      <h3>{controlLabel(control, t)}</h3>
      <div className="form-stack">
        {(control.kind === "pad" || control.kind === "button" || control.kind === "xyTouch") && (
          <ButtonFields
            control={control}
            action={action as ButtonAction | undefined}
            banks={banks}
            profiles={profiles}
            onChange={onChange}
          />
        )}
        {control.kind === "knob" && (
          <KnobFields control={control} action={action as KnobAction | undefined} onChange={onChange} />
        )}
        {control.kind === "xy" && <XyFields action={action as XyAction | undefined} onChange={onChange} />}
        {control.kind === "encoder" && (
          <EncoderFields action={action as EncoderAction | undefined} onChange={onChange} />
        )}
        {action && (
          <TextField
            label={t("action.label")}
            value={action.label ?? ""}
            placeholder={t("action.labelPlaceholder")}
            hint={t("action.labelHint")}
            onChange={(label) => onChange(withField(action, "label", label === "" ? undefined : label))}
          />
        )}
      </div>
      {errors.length > 0 && (
        <ul className="errors">
          {errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      )}
    </div>
  );
}
