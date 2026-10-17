import { memo } from "react";
import { useI18n } from "../i18n";
import { describeEvent, type MidiMessage, type MonitorEntry } from "../types";

type Props = {
  messages: MonitorEntry[];
  paused: boolean;
  onTogglePause: () => void;
  onClear: () => void;
};

const DIRECTIONS: Record<MidiMessage["direction"], string> = {
  in: "padKONTROL",
  out: "→ DAW",
  feedback: "← DAW",
  osc: "→ OSC",
  oscIn: "← OSC",
  key: "⌨",
};

const time = (ms: number, locale: string) =>
  new Date(ms).toLocaleTimeString(locale, { hour12: false }) + "." + String(ms % 1000).padStart(3, "0");

/** One row; rows already shown are not rendered again when new ones arrive. */
const MonitorRow = memo(function MonitorRow({ entry }: { entry: MonitorEntry }) {
  const { t, locale } = useI18n();
  return (
    <tr className={`dir-${entry.direction}${entry.direction === "in" && !entry.event ? " unknown" : ""}`}>
      <td>{time(entry.receivedAt, locale)}</td>
      <td className="dir">{DIRECTIONS[entry.direction]}</td>
      <td>{entry.event ? describeEvent(entry.event, t) : "–"}</td>
      <td>{entry.description}</td>
      <td className="hex">{entry.hex}</td>
    </tr>
  );
});

export const Monitor = memo(function Monitor({ messages, paused, onTogglePause, onClear }: Props) {
  const { t } = useI18n();
  return (
    <section className="card">
      <div className="card-header">
        <h2>{t("monitor.title")}</h2>
        <div className="actions">
          <button onClick={onTogglePause}>{paused ? t("monitor.resume") : t("monitor.pause")}</button>
          <button onClick={onClear}>{t("monitor.clear")}</button>
        </div>
      </div>

      {messages.length === 0 ? (
        <p className="muted">{t("monitor.empty")}</p>
      ) : (
        <div className="monitor">
          <table>
            <thead>
              <tr>
                <th>{t("monitor.time")}</th>
                <th>{t("monitor.direction")}</th>
                <th>{t("monitor.control")}</th>
                <th>{t("monitor.message")}</th>
                <th>{t("monitor.bytes")}</th>
              </tr>
            </thead>
            <tbody>
              {messages.map((m) => (
                <MonitorRow key={m.id} entry={m} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
});
