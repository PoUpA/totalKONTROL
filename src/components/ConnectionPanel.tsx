import { memo, useEffect, useRef, useState } from "react";
import { useI18n } from "../i18n";
import type {
  ConnectionInfo,
  ConnectOptions,
  MidiPort,
  MidiPorts,
  OscOptions,
  PortChoice,
  ProfileSummary,
} from "../types";

// v3: adds the DAW ports and the profile.
const STORAGE_KEY = "totalkontrol.connection.v3";

// With the Korg driver, controls arrive on "PORT A" and the padKONTROL
// listens for native mode on "CTRL". "MIDI IN / OUT" are the DIN jacks.
const INPUT_PREFERENCES = [/port a/i, /ctrl/i, /midiin2/i];
const OUTPUT_PREFERENCES = [/ctrl/i, /midiout2/i];
const DIN_JACK = /midi (in|out)$/i;
// The loopMIDI port recommended in the README, then any loopMIDI port.
const DAW_PREFERENCES = [/^totalkontrol$/i, /totalkontrol(?!.*(feedback|retour))/i, /loopmidi/i];
const FEEDBACK_PREFERENCES = [/feedback|retour/i];

const VIRTUAL = "virtual";
const PORT_PREFIX = "port:";

type Selection = {
  input: string;
  output: string;
  nativeMode: boolean;
  daw: string;
  feedback: string;
  profile: string;
  osc?: OscSettings;
  autoConnect?: boolean;
};

/** OSC settings as edited in the panel; `enabled` and `listen` turn output and input on or off. */
type OscSettings = Omit<OscOptions, "listenPort"> & { enabled: boolean; listen: boolean; listenPort: number };

const DEFAULT_OSC: OscSettings = {
  enabled: false,
  host: "127.0.0.1",
  port: 9000,
  prefix: "/totalkontrol",
  mirror: true,
  listen: false,
  listenPort: 9001,
};

function loadSelection(): Partial<Selection> {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
  } catch {
    return {};
  }
}

function saveSelection(selection: Selection) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(selection));
  } catch {
    // Convenience preference only: skip it if storage is unavailable.
  }
}

const isValid = (value: string | undefined, options: string[]) =>
  value !== undefined && (value === "" || options.includes(value));

/** The remembered port if it still exists, otherwise the most likely padKONTROL port. */
function pickPadPort(ports: MidiPort[], remembered: string | undefined, preferences: RegExp[]): string {
  if (
    isValid(
      remembered,
      ports.map((p) => p.name),
    )
  )
    return remembered!;
  const pads = ports.filter((p) => p.isPadKontrol);
  for (const preference of preferences) {
    const match = pads.find((p) => preference.test(p.name));
    if (match) return match.name;
  }
  return (pads.find((p) => !DIN_JACK.test(p.name)) ?? pads[0] ?? ports[0])?.name ?? "";
}

/** Select value for a DAW port: "", "virtual" or "port:<name>". */
function pickDawPort(
  ports: MidiPort[],
  remembered: string | undefined,
  preferences: RegExp[],
  virtualSupported: boolean,
): string {
  const values = ports.map((p) => PORT_PREFIX + p.name).concat(virtualSupported ? [VIRTUAL] : []);
  if (isValid(remembered, values)) return remembered!;
  const others = ports.filter((p) => !p.isPadKontrol);
  for (const preference of preferences) {
    const match = others.find((p) => preference.test(p.name));
    if (match) return PORT_PREFIX + match.name;
  }
  return virtualSupported ? VIRTUAL : "";
}

function toChoice(value: string): PortChoice | null {
  if (value === VIRTUAL) return "virtual";
  if (value.startsWith(PORT_PREFIX)) return { port: value.slice(PORT_PREFIX.length) };
  return null;
}

function DawSelect({
  label,
  value,
  onChange,
  ports,
  virtualSupported,
  virtualName,
  disabled,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  ports: MidiPort[];
  virtualSupported: boolean;
  virtualName: string;
  disabled: boolean;
}) {
  const { t } = useI18n();
  return (
    <label>
      {label}
      <select value={value} onChange={(e) => onChange(e.target.value)} disabled={disabled}>
        <option value="">{t("common.none")}</option>
        {virtualSupported && <option value={VIRTUAL}>{t("connection.virtualPort", { name: virtualName })}</option>}
        {ports
          .filter((p) => !p.isPadKontrol)
          .map((p) => (
            <option key={p.name} value={PORT_PREFIX + p.name}>
              {p.name}
            </option>
          ))}
      </select>
    </label>
  );
}

type Props = {
  ports: MidiPorts | null;
  profiles: ProfileSummary[];
  connection: ConnectionInfo | null;
  busy: boolean;
  onRefresh: () => void;
  onConnect: (options: ConnectOptions, profile: string | null) => void;
  onDisconnect: () => void;
  /** One-line version, with the port settings folded away. */
  compact?: boolean;
  /** The padKONTROL was unplugged: the app reconnects when it comes back. */
  waiting: boolean;
};

function ConnectionPanelView({
  ports,
  profiles,
  connection,
  waiting,
  busy,
  onRefresh,
  onConnect,
  onDisconnect,
  compact = false,
}: Props) {
  const [expanded, setExpanded] = useState(false);
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [nativeMode, setNativeMode] = useState(true);
  const [daw, setDaw] = useState("");
  const [feedback, setFeedback] = useState("");
  const [profile, setProfile] = useState("");
  const [osc, setOsc] = useState<OscSettings>(() => ({ ...DEFAULT_OSC, ...loadSelection().osc }));
  const [autoConnect, setAutoConnect] = useState(() => loadSelection().autoConnect ?? false);
  const autoConnectTried = useRef(false);
  const { t } = useI18n();
  const editOsc = (change: Partial<OscSettings>) => setOsc((o) => ({ ...o, ...change }));

  const validProfiles = profiles.filter((p) => p.errors.length === 0);

  useEffect(() => {
    if (!ports) return;
    const remembered = loadSelection();
    setInput(pickPadPort(ports.inputs, remembered.input, INPUT_PREFERENCES));
    setOutput(pickPadPort(ports.outputs, remembered.output, OUTPUT_PREFERENCES));
    setDaw(pickDawPort(ports.outputs, remembered.daw, DAW_PREFERENCES, ports.virtualSupported));
    setFeedback(pickDawPort(ports.inputs, remembered.feedback, FEEDBACK_PREFERENCES, ports.virtualSupported));
    if (remembered.nativeMode !== undefined) setNativeMode(remembered.nativeMode);
  }, [ports]);

  useEffect(() => {
    const remembered = loadSelection().profile;
    const files = validProfiles.map((p) => p.file);
    setProfile(isValid(remembered, files) ? remembered! : (files[0] ?? ""));
  }, [profiles]);

  const connected = connection !== null;
  const canConnect = input !== "" && (!nativeMode || output !== "");
  const virtualSupported = ports?.virtualSupported ?? false;

  const connect = () => {
    saveSelection({ input, output, nativeMode, daw, feedback, profile, osc, autoConnect });
    onConnect(
      {
        input,
        output: output === "" ? null : output,
        nativeMode,
        dawOutput: toChoice(daw),
        feedbackInput: toChoice(feedback),
        osc: osc.enabled
          ? {
              host: osc.host,
              port: osc.port,
              prefix: osc.prefix,
              mirror: osc.mirror,
              listenPort: osc.listen ? osc.listenPort : null,
            }
          : null,
      },
      nativeMode && profile !== "" ? profile : null,
    );
  };

  // At launch, once the ports are known and preselected: reconnect with the
  // last settings when asked to.
  useEffect(() => {
    if (autoConnectTried.current || !ports || input === "") return;
    autoConnectTried.current = true;
    if (autoConnect && !connected && !waiting && canConnect) connect();
  }, [ports, input]);

  const toggleAutoConnect = (on: boolean) => {
    setAutoConnect(on);
    saveSelection({ ...loadSelection(), autoConnect: on } as Selection);
  };

  const showForm = !compact || expanded;

  return (
    <section className={compact ? "card compact" : "card"}>
      <div className="card-header">
        <h2>{t("connection.title")}</h2>
        {compact && !expanded && (
          <span className="muted summary">
            {connected ? `${connection.input} · ${connection.dawOutput ?? t("connection.noDaw")}` : input}
          </span>
        )}
        <span className={connected ? "status ok" : waiting ? "status waiting" : "status"}>
          {connected
            ? connection.nativeMode
              ? t("connection.statusNative")
              : t("connection.statusConnected")
            : waiting
              ? t("connection.statusWaiting")
              : t("connection.statusDisconnected")}
        </span>
      </div>

      {showForm && (
        <>
          <h3>padKONTROL</h3>
          <div className="form-grid">
            <label>
              {t("connection.input")}
              <select value={input} onChange={(e) => setInput(e.target.value)} disabled={connected}>
                {ports?.inputs.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("connection.output")}
              <select value={output} onChange={(e) => setOutput(e.target.value)} disabled={connected}>
                <option value="">{t("common.noneFem")}</option>
                {ports?.outputs.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <label className="checkbox">
            <input
              type="checkbox"
              checked={nativeMode}
              onChange={(e) => setNativeMode(e.target.checked)}
              disabled={connected}
            />
            {t("connection.nativeMode")}
          </label>

          <h3>DAW</h3>
          <div className="form-grid">
            <DawSelect
              label={t("connection.dawOutput")}
              value={daw}
              onChange={setDaw}
              ports={ports?.outputs ?? []}
              virtualSupported={virtualSupported}
              virtualName={ports?.virtualOutputName ?? ""}
              disabled={connected}
            />
            <DawSelect
              label={t("connection.dawFeedback")}
              value={feedback}
              onChange={setFeedback}
              ports={ports?.inputs ?? []}
              virtualSupported={virtualSupported}
              virtualName={ports?.virtualFeedbackName ?? ""}
              disabled={connected}
            />
            <label>
              {t("connection.profile")}
              <select value={profile} onChange={(e) => setProfile(e.target.value)} disabled={connected || !nativeMode}>
                <option value="">{t("common.none")}</option>
                {validProfiles.map((p) => (
                  <option key={p.file} value={p.file}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {ports?.virtualNote && <p className="hint warn-text">{ports.virtualNote}</p>}
          {!virtualSupported && <p className="muted hint">{t("connection.loopMidiHint")}</p>}

          <h3>OSC</h3>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={osc.enabled}
              onChange={(e) => editOsc({ enabled: e.target.checked })}
              disabled={connected}
            />
            {t("connection.oscEnabled")}
          </label>
          {osc.enabled && (
            <>
              <div className="form-grid">
                <label>
                  {t("connection.oscHost")}
                  <input value={osc.host} onChange={(e) => editOsc({ host: e.target.value })} disabled={connected} />
                </label>
                <label>
                  {t("connection.oscPort")}
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    value={osc.port}
                    onChange={(e) => {
                      const port = Number.parseInt(e.target.value, 10);
                      if (port >= 1 && port <= 65535) editOsc({ port });
                    }}
                    disabled={connected}
                  />
                </label>
                <label>
                  {t("connection.oscPrefix")}
                  <input
                    value={osc.prefix}
                    onChange={(e) => editOsc({ prefix: e.target.value })}
                    disabled={connected}
                  />
                </label>
              </div>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={osc.mirror}
                  onChange={(e) => editOsc({ mirror: e.target.checked })}
                  disabled={connected}
                />
                {t("connection.oscMirror", { prefix: osc.prefix || "/" })}
              </label>
              <div className="osc-listen">
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={osc.listen}
                    onChange={(e) => editOsc({ listen: e.target.checked })}
                    disabled={connected}
                  />
                  {t("connection.oscListen")}
                </label>
                <input
                  type="number"
                  min={1}
                  max={65535}
                  value={osc.listenPort}
                  aria-label={t("connection.oscListenPort")}
                  onChange={(e) => {
                    const listenPort = Number.parseInt(e.target.value, 10);
                    if (listenPort >= 1 && listenPort <= 65535) editOsc({ listenPort });
                  }}
                  disabled={connected || !osc.listen}
                />
              </div>
              {osc.listen && (
                <p className="muted hint">{t("connection.oscListenHint", { prefix: osc.prefix || "/" })}</p>
              )}
            </>
          )}
        </>
      )}

      {showForm && (
        <label className="checkbox">
          <input type="checkbox" checked={autoConnect} onChange={(e) => toggleAutoConnect(e.target.checked)} />
          {t("connection.autoConnect")}
        </label>
      )}

      <div className="actions">
        {connected ? (
          <button className="primary" onClick={onDisconnect} disabled={busy}>
            {t("connection.disconnect")}
          </button>
        ) : waiting ? (
          <button onClick={onDisconnect} disabled={busy}>
            {t("connection.stopWaiting")}
          </button>
        ) : (
          <button className="primary" onClick={connect} disabled={busy || !canConnect}>
            {t("connection.connect")}
          </button>
        )}
        {showForm && (
          <button onClick={onRefresh} disabled={busy || connected}>
            {t("connection.refresh")}
          </button>
        )}
        {compact && (
          <button onClick={() => setExpanded((e) => !e)}>
            {expanded ? t("connection.hideSettings") : t("connection.portSettings")}
          </button>
        )}
      </div>
    </section>
  );
}

/** Memoized: the live view re-renders the app on every frame. */
export const ConnectionPanel = memo(ConnectionPanelView);
