import { useI18n } from "../../i18n";
import { NumberInput } from "../NumberInput";
import type { DisplaySettings, Profile } from "../../types";

const DEFAULT_DURATION = 1000;

/** Copy of the profile with its display settings, left out when all default. */
export function withDisplay(profile: Profile, display: DisplaySettings): Profile {
  const clean: DisplaySettings = {};
  if (display.knobs === false) clean.knobs = false;
  if (display.encoder === false) clean.encoder = false;
  if (display.pads && display.pads !== "none") clean.pads = display.pads;
  if (display.duration !== undefined && display.duration !== DEFAULT_DURATION) clean.duration = display.duration;
  const { display: _, ...rest } = profile;
  return Object.keys(clean).length > 0 ? { ...rest, display: clean } : rest;
}

type Props = {
  settings: DisplaySettings;
  onChange: (settings: DisplaySettings) => void;
};

/** Profile-wide settings of the 3-character display. */
export function DisplaySettingsForm({ settings, onChange }: Props) {
  const { t } = useI18n();
  return (
    <fieldset className="display-settings">
      <legend>{t("display.title")}</legend>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={settings.knobs ?? true}
          onChange={(e) => onChange({ ...settings, knobs: e.target.checked })}
        />
        {t("display.knobs")}
      </label>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={settings.encoder ?? true}
          onChange={(e) => onChange({ ...settings, encoder: e.target.checked })}
        />
        {t("display.encoder")}
      </label>
      <label>
        {t("display.pads")}
        <select
          value={settings.pads ?? "none"}
          onChange={(e) => onChange({ ...settings, pads: e.target.value as DisplaySettings["pads"] })}
        >
          <option value="none">{t("display.padsNone")}</option>
          <option value="note">{t("display.padsNote")}</option>
          <option value="velocity">{t("display.padsVelocity")}</option>
        </select>
      </label>
      <label>
        {t("display.duration")}
        <NumberInput
          min={200}
          max={10000}
          step={100}
          value={settings.duration ?? DEFAULT_DURATION}
          onChange={(duration) => onChange({ ...settings, duration })}
        />
      </label>
    </fieldset>
  );
}
