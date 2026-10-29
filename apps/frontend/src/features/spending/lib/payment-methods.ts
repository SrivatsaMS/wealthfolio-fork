import type { TFunction } from "i18next";

import {
  PAYMENT_METHOD_METADATA_KEY,
  PAYMENT_METHODS,
  UNSPECIFIED_PAYMENT_METHOD,
  type PaymentMethod,
} from "../types/payment-method";

export function isPaymentMethod(value: unknown): value is PaymentMethod {
  return typeof value === "string" && (PAYMENT_METHODS as readonly string[]).includes(value);
}

/** The payment method stored on an activity's metadata, if any. */
export function readPaymentMethod(metadata: unknown): PaymentMethod | null {
  let parsed = metadata;
  if (typeof metadata === "string") {
    try {
      parsed = JSON.parse(metadata);
    } catch {
      return null;
    }
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
  const value = (parsed as Record<string, unknown>)[PAYMENT_METHOD_METADATA_KEY];
  return isPaymentMethod(value) ? value : null;
}

/**
 * Adds the payment method to an activity metadata patch.
 *
 * Activity updates merge metadata keys, so the key is only sent when it
 * changed (or on create); `null` clears a previously stored method.
 * `base` keeps whatever else the form already decided to send.
 */
export function withPaymentMethod(
  base: Record<string, unknown> | undefined,
  next: PaymentMethod | null,
  previous: PaymentMethod | null,
  isCreate: boolean,
): Record<string, unknown> | undefined {
  if (isCreate ? next === null : next === previous) return base;
  return { ...(base ?? {}), [PAYMENT_METHOD_METADATA_KEY]: next };
}

export function paymentMethodLabel(t: TFunction, method: string): string {
  if (method === UNSPECIFIED_PAYMENT_METHOD) return t("spending:paymentMethod.unspecified");
  return isPaymentMethod(method) ? t(`spending:paymentMethod.${method}`) : method;
}

export function paymentMethodOptions(t: TFunction, includeUnspecified = false) {
  const options: { value: string; label: string }[] = PAYMENT_METHODS.map((method) => ({
    value: method,
    label: paymentMethodLabel(t, method),
  }));
  if (includeUnspecified) {
    options.push({
      value: UNSPECIFIED_PAYMENT_METHOD,
      label: paymentMethodLabel(t, UNSPECIFIED_PAYMENT_METHOD),
    });
  }
  return options;
}

function csvCell(value: string | number | null | undefined): string {
  const text = value == null ? "" : String(value);
  return /[",\n\r]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

export interface PaymentMethodCsvRow {
  date: string;
  account: string;
  payee: string;
  category: string;
  paymentMethod: string;
  amount: number | string;
  currency: string;
}

/** CSV export of transactions with their payment method. */
export function paymentMethodRowsToCsv(rows: PaymentMethodCsvRow[]): string {
  const header = ["date", "account", "payee", "category", "payment_method", "amount", "currency"];
  const lines = rows.map((r) =>
    [r.date, r.account, r.payee, r.category, r.paymentMethod, r.amount, r.currency]
      .map(csvCell)
      .join(","),
  );
  return [header.join(","), ...lines].join("\n");
}
