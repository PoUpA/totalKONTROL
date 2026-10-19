import { memo, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  controlOfEvent,
  type ControlRef,
  type EngineState,
  type MidiMessage,
  type Profile,
  type ProfileSummary,
} from "../../types";
import { useI18n } from "../../i18n";
import { ActionForm } from "./ActionForm";
import { DisplaySettingsForm, withDisplay } from "./DisplaySettingsForm";
import { RepeatSettingsForm, withRepeat } from "./RepeatSettingsForm";
import { VelocitySettingsForm, withVelocity } from "./VelocitySettingsForm";
import { FrontPanel } from "./FrontPanel";
import { ScenesPanel } from "./ScenesPanel";
import { addBank, controlPath, getAction, removeBank, setAction, swapBanks } from "./profile";

type Props = {
  profiles: ProfileSummary[];
  dir: string;
  engine: EngineState | null;
  /** True when connected in native mode: the padKONTROL can pick controls. */
  canLearn: boolean;
  /** True when connected with an output to the padKONTROL: its scenes can be read and written. */
  canUseScenes: boolean;
  onProfilesChanged: () => Promise<void>;
  onEngineState: (state: EngineState) => void;
  /** Whether the open profile has unsaved changes, for quitting. */
  onDirtyChange: (dirty: boolean) => void;
};

const VALIDATION_DELAY_MS = 250;

function ProfileEditorView({
  profiles,
  dir,
  engine,
  canLearn,
  canUseScenes,
  onProfilesChanged,
  onEngineState,
  onDirtyChange,
}: Props) {
  const [file, setFile] = useState<string | null>(null);
  const [draft, setDraft] = useState<Profile | null>(null);
  const [saved, setSaved] = useState("");
  const [loadErrors, setLoadErrors] = useState<string[]>([]);
  const [errors, setErrors] = useState<string[]>([]);
  const [bank, setBank] = useState(0);
  const [selected, setSelected] = useState<ControlRef | null>(null);
  const [learn, setLearn] = useState(false);
  // What to do once the unsaved changes are discarded: open or create a profile.
  const [pending, setPending] = useState<(() => void) | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [confirmDeleteBank, setConfirmDeleteBank] = useState(false);
  const [validating, setValidating] = useState(false);
  // Latest load and validation asked for: older answers are dropped.
  const openRequest = useRef(0);
  const validationRequest = useRef(0);
  const [newName, setNewName] = useState("");
  const [presets, setPresets] = useState<{ id: string; name: string }[]>([]);
  const [preset, setPreset] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ error: boolean; text: string } | null>(null);
  const { t, lang } = useI18n();

  const dirty = draft !== null && JSON.stringify(draft) !== saved;
  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  const open = useCallback(async (next: string) => {
    const request = ++openRequest.current;
    setFile(next);
    setPending(null);
    setConfirmDelete(false);
    setConfirmDeleteBank(false);
    setSelected(null);
    setBank(0);
    setMessage(null);
    try {
      const profile = await invoke<Profile>("load_profile", { file: next });
      if (request !== openRequest.current) return;
      setDraft(profile);
      setSaved(JSON.stringify(profile));
      setLoadErrors([]);
    } catch (e) {
      if (request !== openRequest.current) return;
      setDraft(null);
      setSaved("");
      setLoadErrors(Array.isArray(e) ? e.map(String) : [String(e)]);
    }
  }, []);

  useEffect(() => {
    invoke<{ id: string; name: string }[]>("list_presets")
      .then((list) => {
        setPresets(list);
        setPreset(list[0]?.id ?? "");
      })
      .catch(() => {});
  }, []);

  // Opens the running profile, or the first one, when nothing is open yet.
  useEffect(() => {
    if (file !== null && profiles.some((p) => p.file === file)) return;
    const first = profiles.find((p) => p.file === engine?.file) ?? profiles[0];
    if (first) open(first.file);
    else {
      setFile(null);
      setDraft(null);
    }
  }, [profiles, engine?.file, file, open]);

  // Live validation by the Rust core, the same rules as on save. Runs again
  // when the language changes, since the core writes the messages. The delay
  // also lets the core learn the new language first.
  // Saving waits for the result: the errors shown may be those of an older draft.
  useEffect(() => {
    if (!draft) return;
    const request = ++validationRequest.current;
    setValidating(true);
    const timer = setTimeout(async () => {
      let result: string[];
      try {
        result = await invoke<string[]>("validate_profile", { profile: draft });
      } catch (e) {
        result = [String(e)];
      }
      if (request !== validationRequest.current) return;
      setErrors(result);
      setValidating(false);
    }, VALIDATION_DELAY_MS);
    return () => clearTimeout(timer);
  }, [draft, lang]);

  // Learn mode: touching a control on the padKONTROL selects it.
  useEffect(() => {
    if (!learn || !canLearn) return;
    const unlisten = listen<MidiMessage>("midi-message", (e) => {
      if (e.payload.event) setSelected(controlOfEvent(e.payload.event));
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, [learn, canLearn]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setMessage(null);
    try {
      await action();
    } catch (e) {
      setMessage({ error: true, text: Array.isArray(e) ? e.join("\n") : String(e) });
    } finally {
      setBusy(false);
    }
  };

  /** Runs `action` at once, or after the user agrees to drop the unsaved changes. */
  const guarded = (action: () => void) => {
    if (dirty) setPending(() => action);
    else action();
  };

  const requestOpen = (next: string) => {
    if (next !== file) guarded(() => open(next));
  };

  const save = () =>
    run(async () => {
      if (!file || !draft) return;
      const state = await invoke<EngineState | null>("save_profile", { file, profile: draft });
      setSaved(JSON.stringify(draft));
      if (state) onEngineState(state);
      await onProfilesChanged();
      setMessage({ error: false, text: state ? t("editor.savedApplied") : t("editor.saved") });
    });

  const create = () =>
    guarded(() =>
      run(async () => {
        const created = await invoke<string>("create_profile", {
          name: newName.trim() || t("editor.defaultProfileName"),
        });
        setNewName("");
        await onProfilesChanged();
        await open(created);
      }),
    );

  /** Adds a profile (`add` returns its file name, or null when cancelled) and opens it. */
  const addAndOpen = (add: () => Promise<string | null>) =>
    guarded(() =>
      run(async () => {
        const added = await add();
        if (!added) return;
        await onProfilesChanged();
        await open(added);
      }),
    );

  const fromPreset = () => addAndOpen(() => invoke<string>("create_from_preset", { id: preset }));
  const importProfile = () => addAndOpen(() => invoke<string | null>("import_profile"));

  const exportProfile = () =>
    run(async () => {
      if (!file) return;
      const path = await invoke<string | null>("export_profile", { file });
      if (path) setMessage({ error: false, text: t("editor.exported", { path }) });
    });

  const duplicate = () =>
    run(async () => {
      if (!file) return;
      const copy = await invoke<string>("duplicate_profile", { file });
      await onProfilesChanged();
      await open(copy);
    });

  const remove = () =>
    run(async () => {
      if (!file) return;
      await invoke("delete_profile", { file });
      setFile(null);
      setDraft(null);
      await onProfilesChanged();
    });

  const update = (f: (profile: Profile) => Profile) => setDraft((d) => (d ? f(d) : d));
  const updateBank = (f: (b: Profile["banks"][number]) => Profile["banks"][number]) =>
    update((p) => ({ ...p, banks: p.banks.map((b, i) => (i === bank ? f(b) : b)) }));

  const currentBank = draft?.banks[bank];
  const selectedPath = selected ? controlPath(bank, selected) : null;
  // "banks[0].pads.1" must not match "banks[0].pads.10".
  const controlErrors = selectedPath
    ? errors.filter((e) => e.startsWith(selectedPath + " ") || e.startsWith(selectedPath + "."))
    : [];
  const otherErrors = errors.filter((e) => !controlErrors.includes(e));

  return (
    <div className="editor">
      <aside className="card profile-list">
        <h2>{t("editor.profiles")}</h2>
        <ul className="list">
          {profiles.map((p) => (
            <li key={p.file}>
              <button
                type="button"
                className={p.file === file ? "profile-item active" : "profile-item"}
                onClick={() => requestOpen(p.file)}
              >
                <strong>{p.name ?? p.file}</strong>
                <small className={p.errors.length > 0 ? "error" : "muted"}>
                  {p.errors.length > 0 ? t("editor.invalid") : p.file}
                  {p.file === engine?.file && ` · ${t("editor.active")}`}
                </small>
              </button>
            </li>
          ))}
        </ul>
        <div className="new-profile">
          <input
            placeholder={t("editor.newProfilePlaceholder")}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && create()}
          />
          <button onClick={create} disabled={busy}>
            {t("editor.create")}
          </button>
        </div>
        <div className="new-profile">
          <select value={preset} onChange={(e) => setPreset(e.target.value)} aria-label={t("editor.preset")}>
            {presets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <button onClick={fromPreset} disabled={busy || preset === ""}>
            {t("editor.fromPreset")}
          </button>
        </div>
        <div className="actions">
          <button onClick={importProfile} disabled={busy}>
            {t("editor.import")}
          </button>
          <button onClick={exportProfile} disabled={busy || !file} title={dirty ? t("editor.exportSaved") : undefined}>
            {t("editor.export")}
          </button>
          <button onClick={() => run(() => invoke("open_profiles_dir"))} disabled={busy}>
            {t("editor.openFolder")}
          </button>
        </div>
        <p className="muted path">{dir}</p>
      </aside>

      <section className="card editor-main">
        {pending && (
          <div className="banner warn" role="alert">
            {t("editor.unsaved")}
            <div className="actions">
              <button
                onClick={() => {
                  setPending(null);
                  pending();
                }}
              >
                {t("editor.discard")}
              </button>
              <button onClick={() => setPending(null)}>{t("common.cancel")}</button>
            </div>
          </div>
        )}

        {loadErrors.length > 0 && (
          <div>
            <h2>{file}</h2>
            <p>{t("editor.cannotOpen")}</p>
            <ul className="errors">
              {loadErrors.map((e) => (
                <li key={e}>{e}</li>
              ))}
            </ul>
          </div>
        )}

        {draft && currentBank && (
          <>
            <div className="card-header">
              <input
                className="title-input"
                value={draft.name}
                onChange={(e) => update((p) => ({ ...p, name: e.target.value }))}
                aria-label={t("editor.profileName")}
              />
              <div className="actions">
                <button className="primary" onClick={save} disabled={busy || !dirty || validating || errors.length > 0}>
                  {t("editor.save")}
                </button>
                <button onClick={() => setDraft(JSON.parse(saved))} disabled={busy || !dirty}>
                  {t("editor.revert")}
                </button>
                <button onClick={duplicate} disabled={busy || dirty}>
                  {t("editor.duplicate")}
                </button>
                {confirmDelete ? (
                  <>
                    <button className="danger" onClick={remove} disabled={busy}>
                      {t("editor.confirmDelete")}
                    </button>
                    <button onClick={() => setConfirmDelete(false)}>{t("common.cancel")}</button>
                  </>
                ) : (
                  <button onClick={() => setConfirmDelete(true)} disabled={busy}>
                    {t("editor.delete")}
                  </button>
                )}
              </div>
            </div>

            {message && <p className={message.error ? "error" : "ok"}>{message.text}</p>}

            <div className="banks">
              {draft.banks.map((b, i) => (
                <button
                  key={i}
                  className={i === bank ? "bank active" : "bank"}
                  aria-pressed={i === bank}
                  onClick={() => {
                    setBank(i);
                    setConfirmDeleteBank(false);
                  }}
                >
                  <small>{i}</small> {b.name}
                  {(draft.startBank ?? 0) === i && " ★"}
                </button>
              ))}
              <button
                onClick={() => {
                  update((p) => addBank(p, t));
                  setBank(draft.banks.length);
                }}
              >
                {t("editor.addBank")}
              </button>
            </div>

            <div className="bank-settings">
              <label>
                {t("editor.bankName")}
                <input value={currentBank.name} onChange={(e) => updateBank((b) => ({ ...b, name: e.target.value }))} />
              </label>
              <label>
                {t("editor.displayText")}
                <input
                  className="display-input"
                  maxLength={3}
                  value={currentBank.display ?? ""}
                  placeholder={currentBank.name.replace(/\s/g, "").slice(0, 3).toUpperCase()}
                  onChange={(e) =>
                    updateBank((b) => {
                      const { display: _, ...rest } = b;
                      return e.target.value === "" ? rest : { ...rest, display: e.target.value.toUpperCase() };
                    })
                  }
                />
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={(draft.startBank ?? 0) === bank}
                  disabled={(draft.startBank ?? 0) === bank}
                  onChange={() => update((p) => ({ ...p, startBank: bank === 0 ? undefined : bank }))}
                />
                {t("editor.startBank")}
              </label>
              <div className="actions">
                <button
                  onClick={() => {
                    update((p) => swapBanks(p, bank, bank - 1));
                    setBank(bank - 1);
                  }}
                  disabled={bank === 0}
                  aria-label={t("editor.moveBankLeft")}
                  title={t("editor.moveBankLeft")}
                >
                  ←
                </button>
                <button
                  onClick={() => {
                    update((p) => swapBanks(p, bank, bank + 1));
                    setBank(bank + 1);
                  }}
                  disabled={bank === draft.banks.length - 1}
                  aria-label={t("editor.moveBankRight")}
                  title={t("editor.moveBankRight")}
                >
                  →
                </button>
                {confirmDeleteBank ? (
                  <>
                    <button
                      className="danger"
                      onClick={() => {
                        update((p) => removeBank(p, bank));
                        setBank(Math.max(0, bank - 1));
                        setSelected(null);
                        setConfirmDeleteBank(false);
                      }}
                    >
                      {t("editor.confirmDeleteBank", { bank: currentBank.name })}
                    </button>
                    <button onClick={() => setConfirmDeleteBank(false)}>{t("common.cancel")}</button>
                  </>
                ) : (
                  <button onClick={() => setConfirmDeleteBank(true)} disabled={draft.banks.length === 1}>
                    {t("editor.deleteBank")}
                  </button>
                )}
              </div>
            </div>

            <div className="profile-settings">
              <DisplaySettingsForm
                settings={draft.display ?? {}}
                onChange={(display) => update((p) => withDisplay(p, display))}
              />
              <VelocitySettingsForm
                settings={draft.velocity ?? {}}
                onChange={(velocity) => update((p) => withVelocity(p, velocity))}
              />
              <RepeatSettingsForm
                settings={draft.repeat ?? {}}
                onChange={(repeat) => update((p) => withRepeat(p, repeat))}
              />
            </div>

            <div className="editor-body">
              <div>
                <label className="checkbox learn">
                  <input
                    type="checkbox"
                    checked={learn && canLearn}
                    onChange={(e) => setLearn(e.target.checked)}
                    disabled={!canLearn}
                  />
                  {t("editor.learn")}
                  {!canLearn && <span className="muted"> {t("editor.learnNeedsNative")}</span>}
                </label>
                <FrontPanel
                  banks={draft.banks}
                  bankIndex={bank}
                  selected={selected}
                  errors={errors}
                  onSelect={setSelected}
                />
              </div>
              <div className="inspector">
                {selected ? (
                  <ActionForm
                    control={selected}
                    action={getAction(currentBank, selected)}
                    banks={draft.banks}
                    profiles={profiles}
                    errors={controlErrors}
                    onChange={(action) => updateBank((b) => setAction(b, selected, action))}
                  />
                ) : (
                  <p className="muted">{t("editor.pickControl")}</p>
                )}
              </div>
            </div>

            {otherErrors.length > 0 && (
              <ul className="errors">
                {otherErrors.map((e) => (
                  <li key={e}>{e}</li>
                ))}
              </ul>
            )}

            <ScenesPanel
              available={canUseScenes}
              file={file}
              bank={bank}
              bankName={currentBank.name}
              dirty={dirty}
              onProfileCreated={async (created) => {
                await onProfilesChanged();
                await open(created);
              }}
            />
          </>
        )}
      </section>
    </div>
  );
}

/** Memoized: mounted while hidden, it must not re-render on every frame of the live view. */
export const ProfileEditor = memo(ProfileEditorView);
