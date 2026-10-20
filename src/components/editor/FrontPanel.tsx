import { useI18n } from "../../i18n";
import type { Bank, ControlRef } from "../../types";
import { PadLayout } from "../PadLayout";
import { getAction, hasError, sameControl, summarize } from "./profile";

type Props = {
  banks: Bank[];
  bankIndex: number;
  selected: ControlRef | null;
  errors: string[];
  onSelect: (control: ControlRef) => void;
};

/** Clickable front panel, with each control's mapping in the bank. */
export function FrontPanel({ banks, bankIndex, selected, errors, onSelect }: Props) {
  const bank = banks[bankIndex];
  const { t } = useI18n();

  return (
    <PadLayout
      display={(bank.display ?? bank.name.replace(/\s/g, "")).toUpperCase()}
      render={(control, label, className) => {
        const action = getAction(bank, control);
        const summary = summarize(action, banks, t);
        const shown = action?.label || summary;
        const classes = [
          className,
          "editable",
          summary ? "mapped" : "",
          sameControl(control, selected) ? "selected" : "",
          hasError(errors, bankIndex, control) ? "invalid" : "",
        ].join(" ");
        return (
          <button
            key={label}
            type="button"
            className={classes}
            onClick={() => onSelect(control)}
            title={action?.label ? `${action.label} · ${summary}` : summary}
            aria-pressed={sameControl(control, selected)}
          >
            <span>{label}</span>
            {shown && <small>{shown}</small>}
          </button>
        );
      }}
    />
  );
}
