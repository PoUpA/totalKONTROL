import { useEffect, useState, type InputHTMLAttributes } from "react";

type Props = Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type"> & {
  value: number;
  min?: number;
  max?: number;
  /** Decimal numbers; integers otherwise. */
  decimal?: boolean;
  onChange: (value: number) => void;
};

/**
 * Number field that can be cleared and retyped: what is typed stays as is
 * while it is not a valid number in range, and only valid numbers reach
 * `onChange`. Leaving the field puts the current value back.
 */
export function NumberInput({ value, min, max, decimal = false, onChange, onBlur, ...rest }: Props) {
  const [text, setText] = useState(String(value));
  // Another control, an undo or a reload changed the value: show it.
  useEffect(() => {
    if (parse(text) !== value) setText(String(value));
  }, [value]);

  function parse(raw: string): number | null {
    if (raw.trim() === "") return null;
    const number = decimal ? Number.parseFloat(raw) : Number.parseInt(raw, 10);
    if (!Number.isFinite(number)) return null;
    if ((min !== undefined && number < min) || (max !== undefined && number > max)) return null;
    return number;
  }

  return (
    <input
      {...rest}
      type="number"
      min={min}
      max={max}
      step={decimal ? "any" : undefined}
      value={text}
      onChange={(e) => {
        setText(e.target.value);
        const number = parse(e.target.value);
        if (number !== null && number !== value) onChange(number);
      }}
      onBlur={(e) => {
        setText(String(value));
        onBlur?.(e);
      }}
    />
  );
}
