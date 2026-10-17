import { useI18n } from "../../i18n";
import type { Profile, RepeatSettings } from "../../types";

// Same ranges and defaults as RepeatSettings in the Rust core.
const DIVISIONS = [4, 6, 8, 12, 16, 24, 32];
const DEFAULTS: Required<RepeatSettings> = { bpm: 120, roll: 16, flam: 30, xySpeed: true };

/** Copy of the profile with its roll and flam settings, left out when all default. */
export function withRepeat(profile: Profile, repeat: RepeatSettings): Profile {
  const clean: RepeatSettings = {};
  for (const key of Object.keys(DEFAULTS) as (keyof RepeatSettings)[]) {
    if (repeat[key] !== undefined && repeat[key] !== DEFAULTS[key]) Object.assign(clean, { [key]: repeat[key] });
  }
  const { repeat: _, ...rest } = profile;
  return Object.keys(clean).length > 0 ? { ...rest, repeat: clean } : rest;
}

type Props = {
  settings: RepeatSettings;
  onChange: (settings: RepeatSettings) => void;
};

/** Profile-wide settings of the roll and flam actions. */
export function RepeatSettingsForm({ settings, onChange }: Props) {
  const { t } = useI18n();
  const full = { ...DEFAULTS, ...settings };

  return (
    <fieldset className="display-settings">
      <legend>{t("repeat.title")}</legend>
      <label>
        {t("repeat.bpm")}
        <input
          type="number"
          min={20}
          max={300}
          step="any"
          value={full.bpm}
          onChange={(e) => {
            const bpm = Number.parseFloat(e.target.value);
            if (bpm >= 20 && bpm <= 300) onChange({ ...settings, bpm });
          }}
        />
      </label>
      <label>
        {t("repeat.roll")}
        <select value={full.roll} onChange={(e) => onChange({ ...settings, roll: Number(e.target.value) })}>
          {DIVISIONS.map((d) => (
            <option key={d} value={d}>
              1/{d}
              {d % 3 === 0 ? ` (${t("repeat.triplet")})` : ""}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("repeat.flam")}
        <input
          type="number"
          min={5}
          max={200}
          value={full.flam}
          onChange={(e) => {
            const flam = Number.parseInt(e.target.value, 10);
            if (flam >= 5 && flam <= 200) onChange({ ...settings, flam });
          }}
        />
      </label>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={full.xySpeed}
          onChange={(e) => onChange({ ...settings, xySpeed: e.target.checked })}
        />
        {t("repeat.xySpeed")}
      </label>
      <p className="hint">{t("repeat.clockHint")}</p>
    </fieldset>
  );
}
