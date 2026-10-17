import { useI18n } from "../../i18n";
import type { Profile, VelocitySettings } from "../../types";

type Curve = NonNullable<VelocitySettings["curve"]>;

// Same shaping as VelocitySettings::apply in the Rust core.
const EXPONENTS: Record<Curve, number> = { soft: 0.5, linear: 1, hard: 2 };

function shape(raw: number, settings: Required<VelocitySettings>): number {
  const x = (Math.min(127, Math.max(1, raw)) - 1) / 126;
  return settings.min + (settings.max - settings.min) * x ** EXPONENTS[settings.curve];
}

const withDefaults = (s: VelocitySettings): Required<VelocitySettings> => ({
  curve: s.curve ?? "linear",
  min: s.min ?? 1,
  max: s.max ?? 127,
});

/** Copy of the profile with its velocity settings, left out when all default. */
export function withVelocity(profile: Profile, velocity: VelocitySettings): Profile {
  const clean: VelocitySettings = {};
  if (velocity.curve && velocity.curve !== "linear") clean.curve = velocity.curve;
  if (velocity.min !== undefined && velocity.min !== 1) clean.min = velocity.min;
  if (velocity.max !== undefined && velocity.max !== 127) clean.max = velocity.max;
  const { velocity: _, ...rest } = profile;
  return Object.keys(clean).length > 0 ? { ...rest, velocity: clean } : rest;
}

/** Hit velocity (x) → velocity sent (y), drawn in a 0-127 square. */
function CurvePreview({ settings }: { settings: Required<VelocitySettings> }) {
  const points = Array.from({ length: 33 }, (_, i) => {
    const raw = 1 + (126 * i) / 32;
    return `${raw.toFixed(1)},${(127 - shape(raw, settings)).toFixed(1)}`;
  }).join(" ");
  return (
    <svg className="velocity-curve" viewBox="0 0 127 127" role="img" aria-hidden="true">
      <line x1="0" y1="127" x2="127" y2="0" className="velocity-curve-linear" />
      <polyline points={points} className="velocity-curve-line" />
    </svg>
  );
}

type Props = {
  settings: VelocitySettings;
  onChange: (settings: VelocitySettings) => void;
};

/** Profile-wide shaping of the pads' hit velocity. */
export function VelocitySettingsForm({ settings, onChange }: Props) {
  const { t } = useI18n();
  const full = withDefaults(settings);
  const number = (key: "min" | "max", label: string) => (
    <label>
      {label}
      <input
        type="number"
        min={1}
        max={127}
        value={full[key]}
        onChange={(e) => {
          const value = Number.parseInt(e.target.value, 10);
          if (value >= 1 && value <= 127) onChange({ ...settings, [key]: value });
        }}
      />
    </label>
  );

  return (
    <fieldset className="display-settings velocity-settings">
      <legend>{t("velocity.title")}</legend>
      <CurvePreview settings={full} />
      <label>
        {t("velocity.curve")}
        <select value={full.curve} onChange={(e) => onChange({ ...settings, curve: e.target.value as Curve })}>
          <option value="soft">{t("velocity.soft")}</option>
          <option value="linear">{t("velocity.linear")}</option>
          <option value="hard">{t("velocity.hard")}</option>
        </select>
      </label>
      {number("min", t("velocity.min"))}
      {number("max", t("velocity.max"))}
    </fieldset>
  );
}
