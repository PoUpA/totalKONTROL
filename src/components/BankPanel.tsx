import { memo } from "react";
import { useI18n } from "../i18n";
import type { EngineState, ProfileSummary } from "../types";

type Props = {
  state: EngineState;
  profiles: ProfileSummary[];
  busy: boolean;
  onSelectBank: (bank: number) => void;
  onSelectProfile: (file: string) => void;
};

function BankPanelView({ state, profiles, busy, onSelectBank, onSelectProfile }: Props) {
  const valid = profiles.filter((p) => p.errors.length === 0);
  const current = valid.some((p) => p.file === state.file) ? state.file : "";
  const { t } = useI18n();

  return (
    <section className="card">
      <div className="card-header">
        <h2>{t("bank.activeProfile")}</h2>
        <select value={current} onChange={(e) => onSelectProfile(e.target.value)} disabled={busy}>
          {current === "" && <option value="">{state.profile}</option>}
          {valid.map((p) => (
            <option key={p.file} value={p.file}>
              {p.name}
            </option>
          ))}
        </select>
      </div>
      <div className="banks">
        {state.banks.map((name, i) => (
          <button
            key={i}
            className={i === state.bank ? "bank active" : "bank"}
            onClick={() => onSelectBank(i)}
            disabled={busy}
          >
            <small>{i}</small> {name}
          </button>
        ))}
      </div>
    </section>
  );
}

/** Memoized: the live view re-renders the app on every frame. */
export const BankPanel = memo(BankPanelView);
