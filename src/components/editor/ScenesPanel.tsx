import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "../../i18n";
import type { Bank } from "../../types";

/** What a scene could not take from a bank (scenes::Left in the core). */
type Left =
  { kind: "pad"; pad: number } | { kind: "knob"; knob: number } | { kind: "xy" } | { kind: "controlChannels" };

type Props = {
  /** Connected with an output to the padKONTROL. */
  available: boolean;
  /** Open profile, as saved, and its bank on screen. */
  file: string | null;
  bank: number;
  bankName: string;
  dirty: boolean;
  onProfileCreated: (file: string) => Promise<void>;
};

const SCENES = Array.from({ length: 16 }, (_, i) => i);

/** The padKONTROL's 16 scenes: what it plays without the app, stored in it. */
export function ScenesPanel({ available, file, bank, bankName, dirty, onProfileCreated }: Props) {
  const { t } = useI18n();
  const [scenes, setScenes] = useState<Bank[] | null>(null);
  const [backups, setBackups] = useState<string[]>([]);
  const [backup, setBackup] = useState("");
  const [scene, setScene] = useState(0);
  const [confirm, setConfirm] = useState<"restore" | "write" | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ error: boolean; text: string } | null>(null);
  // A confirmation is for what was on screen when asked.
  useEffect(() => setConfirm(null), [available, file, bank, dirty, scene, backup]);

  const refreshBackups = async () => {
    const list = await invoke<string[]>("list_scene_backups");
    setBackups(list);
    setBackup((current) => (list.includes(current) ? current : (list[0] ?? "")));
  };
  useEffect(() => {
    refreshBackups().catch(() => {});
  }, []);

  const run = async (action: () => Promise<string | null>) => {
    setBusy(true);
    setMessage(null);
    setConfirm(null);
    try {
      const done = await action();
      if (done) setMessage({ error: false, text: done });
    } catch (e) {
      setMessage({ error: true, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const describeLeft = (left: Left) => {
    switch (left.kind) {
      case "pad":
        return t("scenes.leftPad", { pad: left.pad });
      case "knob":
        return t("scenes.leftKnob", { knob: left.knob });
      case "xy":
        return t("scenes.leftXy");
      case "controlChannels":
        return t("scenes.leftChannels");
    }
  };

  const summary = (b: Bank) => {
    const pads = Object.values(b.pads ?? {});
    const notes = pads.filter((p) => p.type === "note").length;
    const ccs = pads.filter((p) => p.type === "cc").length;
    return t("scenes.summary", { notes, ccs });
  };

  return (
    <fieldset className="display-settings scenes">
      <legend>{t("scenes.title")}</legend>
      <p className="hint">{t("scenes.hint")}</p>
      {!available && <p className="muted">{t("scenes.needsConnection")}</p>}

      <div className="actions">
        <button
          disabled={!available || busy}
          onClick={() =>
            run(async () => {
              setScenes(await invoke<Bank[]>("read_scenes"));
              return t("scenes.read");
            })
          }
        >
          {t("scenes.readButton")}
        </button>
        {scenes && (
          <>
            <button
              disabled={busy}
              onClick={() =>
                run(async () => {
                  const created = await invoke<string>("scenes_to_profile", { name: t("scenes.profileName") });
                  await onProfileCreated(created);
                  return t("scenes.profileCreated");
                })
              }
            >
              {t("scenes.toProfile")}
            </button>
            <button
              disabled={busy}
              onClick={() =>
                run(async () => {
                  const saved = await invoke<string>("backup_scenes");
                  await refreshBackups();
                  return t("scenes.backedUp", { file: saved });
                })
              }
            >
              {t("scenes.backup")}
            </button>
          </>
        )}
      </div>

      {scenes && (
        <ol className="scene-list">
          {scenes.map((s, i) => (
            <li key={i}>
              <strong>{i + 1}</strong> {summary(s)}
            </li>
          ))}
        </ol>
      )}

      <div className="scene-row">
        <label>
          {t("scenes.writeBank", { bank: bankName })}
          <select value={scene} onChange={(e) => setScene(Number(e.target.value))} disabled={busy}>
            {SCENES.map((s) => (
              <option key={s} value={s}>
                {t("scenes.sceneNumber", { scene: s + 1 })}
              </option>
            ))}
          </select>
        </label>
        {confirm === "write" ? (
          <>
            <button
              className="danger"
              disabled={!available || busy}
              onClick={() =>
                run(async () => {
                  const left = await invoke<Left[]>("write_bank_to_scene", { file, bank, scene });
                  const done = t("scenes.written", { scene: scene + 1 });
                  return left.length > 0 ? `${done} ${left.map(describeLeft).join(" ")}` : done;
                })
              }
            >
              {t("scenes.confirmWrite", { scene: scene + 1 })}
            </button>
            <button onClick={() => setConfirm(null)}>{t("common.cancel")}</button>
          </>
        ) : (
          <button disabled={!available || busy || !file || dirty} onClick={() => setConfirm("write")}>
            {t("scenes.write")}
          </button>
        )}
        {dirty && <span className="muted">{t("scenes.saveFirst")}</span>}
      </div>

      <div className="scene-row">
        <label>
          {t("scenes.backups")}
          <select value={backup} onChange={(e) => setBackup(e.target.value)} disabled={busy || backups.length === 0}>
            {backups.length === 0 && <option value="">{t("scenes.noBackup")}</option>}
            {backups.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
        </label>
        {confirm === "restore" ? (
          <>
            <button
              className="danger"
              disabled={!available || busy}
              onClick={() =>
                run(async () => {
                  const previous = await invoke<string>("restore_scenes", { file: backup });
                  await refreshBackups();
                  return t("scenes.restored", { file: previous });
                })
              }
            >
              {t("scenes.confirmRestore")}
            </button>
            <button onClick={() => setConfirm(null)}>{t("common.cancel")}</button>
          </>
        ) : (
          <button disabled={!available || busy || backup === ""} onClick={() => setConfirm("restore")}>
            {t("scenes.restore")}
          </button>
        )}
      </div>

      {busy && <p className="muted">{t("scenes.working")}</p>}
      {message && (
        <p className={message.error ? "error" : "ok"} role="status">
          {message.text}
        </p>
      )}
    </fieldset>
  );
}
