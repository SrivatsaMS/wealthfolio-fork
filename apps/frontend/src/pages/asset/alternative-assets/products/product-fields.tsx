import type { ReactNode } from "react";

import {
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";

interface FieldProps {
  id: string;
  label: string;
  error?: string | null;
  hint?: string;
  children: ReactNode;
}

export function Field({ id, label, error, hint, children }: FieldProps) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {hint && !error && <p className="text-muted-foreground text-xs">{hint}</p>}
      {error && (
        <p id={`${id}-error`} className="text-destructive text-xs">
          {error}
        </p>
      )}
    </div>
  );
}

interface TextFieldProps {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  type?: "text" | "date" | "number";
  inputMode?: "decimal" | "numeric" | "text";
  placeholder?: string;
  error?: string | null;
  hint?: string;
}

export function TextField({
  id,
  label,
  value,
  onChange,
  type = "text",
  inputMode,
  placeholder,
  error,
  hint,
}: TextFieldProps) {
  return (
    <Field id={id} label={label} error={error} hint={hint}>
      <Input
        id={id}
        type={type}
        inputMode={inputMode}
        value={value}
        placeholder={placeholder}
        aria-invalid={!!error}
        aria-describedby={error ? `${id}-error` : undefined}
        onChange={(e) => onChange(e.target.value)}
      />
    </Field>
  );
}

interface SelectFieldProps<T extends string> {
  id: string;
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  hint?: string;
}

export function SelectField<T extends string>({
  id,
  label,
  value,
  options,
  onChange,
  hint,
}: SelectFieldProps<T>) {
  return (
    <Field id={id} label={label} hint={hint}>
      <Select
        value={value}
        onValueChange={(v: string | null) => {
          const next = options.find((o) => o.value === v);
          if (next) onChange(next.value);
        }}
      >
        <SelectTrigger id={id}>
          <SelectValue>{options.find((o) => o.value === value)?.label ?? value}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Field>
  );
}

/** Today in the user's local calendar, as YYYY-MM-DD. */
export function localToday(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** The server's validation message, without transport prefixes. */
export function errorMessage(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.replace(/^(Error: )?(Invalid input: )?/, "");
}
