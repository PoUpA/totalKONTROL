import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BankPanel } from "./components/BankPanel";
import { ConnectionPanel } from "./components/ConnectionPanel";
import { ControlView, initialControlState, type ControlState } from "./components/ControlView";
import { ProfileEditor } from "./components/editor/ProfileEditor";
import { LANGUAGES, sendLangToCore, useI18n, type Lang } from "./i18n";
import { Monitor } from "./components/Monitor";
import type {
  ConnectionInfo,
  ConnectionState,
  ConnectOptions,
  ControlEvent,
  EngineState,
  LedId,
  MidiMessage,
  MidiPorts,
  MonitorEntry,
  ProfileSummary,
} from "./types";
import "./App.css";

const MAX_MESSAGES = 300;
// The XY pad can send hundreds of messages per second. The live view batches
// them once per screen frame, so it stays smooth; the monitor, a long table,
// only refreshes a few times per second.
const MONITOR_INTERVAL_MS = 100;
// Screen frames stop while the window is hidden in the tray: past this many
// waiting events, the live view takes them at once rather than piling them up.
const MAX_PENDING_CONTROLS = 500;

/** Without a profile, the live view counts like a relative encoder: -99 to 999. */
const clampCounter = (value: number) => Math.min(999, Math.max(-99, value));

/** `encoderPosition`: the engine's position, so the view matches the display. */
function applyEvent(state: ControlState, event: ControlEvent, encoderPosition: number | null): ControlState {
  switch (event.kind) {
    case "pad": {
      const pads = { ...state.pads };
      if (event.pressed) pads[event.pad] = event.velocity;
      else delete pads[event.pad];
      return { ...state, pads, seen: state.seen.add(`pad${event.pad}`) };
    }
    case "button":
      return {
        ...state,
        buttons: { ...state.buttons, [event.button]: event.pressed },
        seen: state.seen.add(event.button),
      };
    case "knob":
      return {
        ...state,
        knobs: { ...state.knobs, [event.knob]: event.value },
        seen: state.seen.add(`knob${event.knob}`),
      };
    case "xy":
      return { ...state, xy: { x: event.x, y: event.y }, seen: state.seen.add("xy") };
    case "xyTouch":
      return { ...state, xyTouched: event.pressed, seen: state.seen.add("xyTouch") };
    case "encoder":
      return {
        ...state,
        encoder: encoderPosition ?? clampCounter(state.encoder + event.delta),
        seen: state.seen.add("encoder"),
      };
  }
}

function App() {
  const [ports, setPorts] = useState<MidiPorts | null>(null);
  const [connection, setConnection] = useState<ConnectionInfo | null>(null);
  // The padKONTROL was unplugged: the app reconnects when it comes back.
  const [waiting, setWaiting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [messages, setMessages] = useState<MonitorEntry[]>([]);
  const [paused, setPaused] = useState(false);
  const [controls, setControls] = useState<ControlState>(initialControlState);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [profilesDir, setProfilesDir] = useState("");
  const [engine, setEngine] = useState<EngineState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<"control" | "profiles">("control");
  // LEDs lit on the padKONTROL, keyed like ControlState.seen.
  const [leds, setLeds] = useState<Set<string>>(() => new Set());
  const { lang, setLang, t } = useI18n();

  const pendingControls = useRef<MidiMessage[]>([]);
  const pendingMonitor = useRef<MonitorEntry[]>([]);
  const nextId = useRef(0);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;

  const refresh = useCallback(async () => {
    // Profiles first: they do not depend on the MIDI service, which can hang.
    try {
      setProfiles(await invoke<ProfileSummary[]>("list_profiles"));
      setProfilesDir(await invoke<string>("profiles_dir"));
      const status = await invoke<ConnectionState>("connection_status");
      setConnection(status.connection);
      setWaiting(status.waiting);
      setEngine(await invoke<EngineState | null>("engine_state"));
      setPorts(await invoke<MidiPorts>("list_midi_ports"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // The Rust core learns the language first, so the ports note and the
  // profile errors come back in it; a language change reloads them.
  useEffect(() => {
    sendLangToCore(lang).then(refresh);
  }, [lang, refresh]);

  const refreshProfiles = useCallback(async () => {
    setProfiles(await invoke<ProfileSummary[]>("list_profiles"));
  }, []);

  useEffect(() => {
    const unlisten = listen<EngineState | null>("engine-state", (e) => {
      setEngine(e.payload);
      // Each bank has its own encoder position: show the new bank's.
      const state = e.payload;
      if (state) setControls((c) => ({ ...c, encoder: state.encoder }));
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  // The padKONTROL was unplugged, or came back and the app reconnected.
  useEffect(() => {
    const unlisten = listen<ConnectionState>("connection-state", (e) => {
      setConnection(e.payload.connection);
      setWaiting(e.payload.waiting);
      setEngine(e.payload.engine);
      if (e.payload.connection) setControls(initialControlState());
      else setLeds(new Set());
    });
    const unlistenLeds = listen<LedId[]>("leds", (e) =>
      setLeds(new Set(e.payload.map((led) => (led.kind === "pad" ? `pad${led.pad}` : led.button)))),
    );
    // A reconnection that worked around a problem, such as a profile that
    // no longer loads.
    const unlistenProblem = listen<string>("connection-problem", (e) => setError(e.payload));
    return () => {
      unlisten.then((stop) => stop());
      unlistenProblem.then((stop) => stop());
      unlistenLeds.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    const applyControls = () => {
      const batch = pendingControls.current;
      if (batch.length === 0) return;
      pendingControls.current = [];
      setControls((state) =>
        batch.reduce((s, m) => (m.event ? applyEvent(s, m.event, m.encoderPosition) : s), {
          ...state,
          seen: new Set(state.seen),
        }),
      );
    };

    const unlisten = listen<MidiMessage>("midi-message", (e) => {
      if (e.payload.event) {
        pendingControls.current.push(e.payload);
        if (pendingControls.current.length >= MAX_PENDING_CONTROLS) applyControls();
      }
      if (!pausedRef.current) {
        const pending = pendingMonitor.current;
        pending.push({ ...e.payload, id: nextId.current++ });
        // The monitor only shows the latest messages.
        if (pending.length > MAX_MESSAGES) pending.splice(0, pending.length - MAX_MESSAGES);
      }
    });

    let frame = 0;
    const flushControls = () => {
      applyControls();
      frame = requestAnimationFrame(flushControls);
    };
    frame = requestAnimationFrame(flushControls);

    const timer = setInterval(() => {
      const batch = pendingMonitor.current;
      if (batch.length === 0) return;
      pendingMonitor.current = [];
      setMessages((list) => batch.reverse().concat(list).slice(0, MAX_MESSAGES));
    }, MONITOR_INTERVAL_MS);

    return () => {
      cancelAnimationFrame(frame);
      clearInterval(timer);
      unlisten.then((stop) => stop());
    };
  }, []);

  // Stable callbacks, so the memoized cards do not re-render on every frame.

  /** Runs a command while disabling the buttons, and displays its error. */
  const run = useCallback(async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const connect = useCallback(
    (options: ConnectOptions, profile: string | null) =>
      run(async () => {
        setConnection(await invoke<ConnectionInfo>("connect_pad", { options, profile }));
        setWaiting(false);
        setEngine(await invoke<EngineState | null>("engine_state"));
        setControls(initialControlState());
      }),
    [run],
  );

  const panic = useCallback(() => run(() => invoke("panic")), [run]);

  const disconnect = useCallback(
    () =>
      run(async () => {
        await invoke("disconnect_pad");
        setConnection(null);
        setWaiting(false);
        setEngine(null);
      }),
    [run],
  );

  /** New engine state; the live view takes the active bank's encoder position. */
  const applyEngine = useCallback((state: EngineState) => {
    setEngine(state);
    setControls((c) => ({ ...c, encoder: state.encoder }));
  }, []);

  const selectBank = useCallback(
    (bank: number) => run(async () => applyEngine(await invoke<EngineState>("select_bank", { bank }))),
    [run, applyEngine],
  );

  const selectProfile = useCallback(
    (file: string) => run(async () => applyEngine(await invoke<EngineState>("set_profile", { file }))),
    [run, applyEngine],
  );

  const togglePause = useCallback(() => setPaused((p) => !p), []);
  const clearMonitor = useCallback(() => setMessages([]), []);

  return (
    <main className={tab === "profiles" ? "container wide" : "container"}>
      <header className="app-header">
        <h1>totalKONTROL</h1>
        <div className="header-tools">
          <nav className="tabs">
            <button className={tab === "control" ? "tab active" : "tab"} onClick={() => setTab("control")}>
              {t("app.tabs.control")}
            </button>
            <button className={tab === "profiles" ? "tab active" : "tab"} onClick={() => setTab("profiles")}>
              {t("app.tabs.profiles")}
            </button>
          </nav>
          <select
            className="language"
            value={lang}
            onChange={(e) => setLang(e.target.value as Lang)}
            aria-label={t("app.language")}
          >
            {LANGUAGES.map((l) => (
              <option key={l.code} value={l.code}>
                {l.label}
              </option>
            ))}
          </select>
        </div>
      </header>
      {error && <p className="error banner">{error}</p>}

      {/* Shared by both tabs: the editor's learn mode needs a connection too. */}
      <ConnectionPanel
        ports={ports}
        profiles={profiles}
        connection={connection}
        waiting={waiting}
        busy={busy}
        onRefresh={refresh}
        onConnect={connect}
        onDisconnect={disconnect}
        onPanic={panic}
        compact={tab === "profiles"}
      />

      {/* Both tabs stay mounted, so an unsaved profile survives a tab switch. */}
      <div className="tab-panel" hidden={tab !== "profiles"}>
        <ProfileEditor
          profiles={profiles}
          dir={profilesDir}
          engine={engine}
          canLearn={connection?.nativeMode ?? false}
          onProfilesChanged={refreshProfiles}
          onEngineState={setEngine}
        />
      </div>

      <div className="tab-panel" hidden={tab !== "control"}>
        {engine && (
          <BankPanel
            state={engine}
            profiles={profiles}
            busy={busy}
            onSelectBank={selectBank}
            onSelectProfile={selectProfile}
          />
        )}
        <ControlView
          state={controls}
          display={engine ? engine.display.toUpperCase() : connection?.nativeMode ? "TK" : "---"}
          leds={leds}
        />
        <Monitor messages={messages} paused={paused} onTogglePause={togglePause} onClear={clearMonitor} />
      </div>
    </main>
  );
}

export default App;
