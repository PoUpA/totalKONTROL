import { useEffect, useRef, useState } from "react";
import { useI18n } from "../../i18n";

// Key names of the Rust core (src-tauri/src/keys.rs), by KeyboardEvent.key.
const NAMED: Record<string, string> = {
  " ": "space",
  "+": "plus",
  Enter: "enter",
  Tab: "tab",
  Escape: "esc",
  Backspace: "backspace",
  Delete: "delete",
  Insert: "insert",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  CapsLock: "capslock",
  PrintScreen: "printscreen",
  Pause: "pause",
  AudioVolumeUp: "volumeup",
  AudioVolumeDown: "volumedown",
  AudioVolumeMute: "mute",
  MediaPlayPause: "playpause",
  MediaTrackNext: "next",
  MediaTrackPrevious: "previous",
};

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "OS", "AltGraph"]);

function modifiers(e: KeyboardEvent): string[] {
  const names: string[] = [];
  if (e.ctrlKey) names.push("ctrl");
  if (e.shiftKey) names.push("shift");
  if (e.altKey) names.push("alt");
  if (e.metaKey) names.push("meta");
  return names;
}

/** Name of the key pressed, or null for a modifier or a key the core does not know. */
function keyName(e: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const numpad = /^Numpad(\d)$/.exec(e.code);
  if (numpad) return `num${numpad[1]}`;
  if (/^F([1-9]|1\d|20)$/.test(e.key)) return e.key.toLowerCase();
  if (NAMED[e.key]) return NAMED[e.key];
  return e.key.length === 1 ? e.key.toLowerCase() : null;
}

/** Keyboard shortcut, typed as text ("ctrl+shift+s") or recorded from the keyboard. */
export function KeysField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const [recording, setRecording] = useState(false);
  // The latest onChange, so the listeners below stay put while recording.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!recording) return;
    // A modifier pressed and released alone records the modifiers.
    let held: string[] = [];
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const key = keyName(e);
      if (key) {
        onChangeRef.current([...modifiers(e), key].join("+"));
        setRecording(false);
      } else if (MODIFIER_KEYS.has(e.key)) {
        held = modifiers(e);
      }
    };
    const up = (e: KeyboardEvent) => {
      e.preventDefault();
      if (MODIFIER_KEYS.has(e.key) && held.length > 0) {
        onChangeRef.current(held.join("+"));
        setRecording(false);
      }
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up, true);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up, true);
    };
  }, [recording]);

  return (
    <label>
      {label}
      <span className="field-row">
        <input
          className="text-input"
          value={recording ? t("action.keysRecording") : value}
          readOnly={recording}
          placeholder="ctrl+shift+s"
          onChange={(e) => onChange(e.target.value)}
        />
        <button
          type="button"
          className={recording ? "primary" : undefined}
          onClick={() => setRecording((r) => !r)}
          onBlur={() => setRecording(false)}
        >
          {recording ? t("action.keysStop") : t("action.keysRecord")}
        </button>
      </span>
    </label>
  );
}
