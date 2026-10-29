/**
 * Fixed deposit and insurance policy terms as stored in alternative-asset
 * metadata. Keys and values mirror `crates/core/src/assets/fixed_deposit.rs`
 * and `insurance_policy.rs`; all validation and calculation happens in Rust.
 */

export const FIXED_DEPOSIT_SUBTYPE = "fixed_deposit";
export const INSURANCE_POLICY_SUBTYPE = "insurance_policy";

export type ProductSubtype = typeof FIXED_DEPOSIT_SUBTYPE | typeof INSURANCE_POLICY_SUBTYPE;

export function productSubtype(
  metadata: Record<string, unknown> | undefined,
): ProductSubtype | null {
  const value = metadata?.sub_type;
  return value === FIXED_DEPOSIT_SUBTYPE || value === INSURANCE_POLICY_SUBTYPE ? value : null;
}

export interface FixedDepositFormValues {
  name: string;
  currency: string;
  issuer: string;
  principal: string;
  interestRate: string;
  interestMethod: "simple" | "compound";
  compounding: "monthly" | "quarterly" | "half_yearly" | "yearly";
  dayCount: "act365" | "act360" | "act_act" | "thirty360";
  payout: "cumulative" | "periodic";
  startDate: string;
  maturityDate: string;
  actualMaturityAmount: string;
  taxRate: string;
  status: "active" | "closed";
  closedDate: string;
  notes: string;
}

export interface PolicyFormValues {
  name: string;
  currency: string;
  insurer: string;
  policyNumber: string;
  policyType: string;
  premiumAmount: string;
  frequency: "monthly" | "quarterly" | "half_yearly" | "yearly" | "single" | "custom";
  frequencyMonths: string;
  commencementDate: string;
  premiumStartDate: string;
  policyTermYears: string;
  premiumTermYears: string;
  maturityDate: string;
  sumAssured: string;
  bonus: string;
  expectedMaturity: string;
  assumedReturn: string;
  premiumsPaidBefore: string;
  status: "in_force" | "paid_up" | "lapsed" | "matured" | "surrendered";
  closedDate: string;
  valuation: "premiums_paid" | "manual";
  notes: string;
}

const str = (metadata: Record<string, unknown> | undefined, key: string): string => {
  const value = metadata?.[key];
  return typeof value === "string" ? value : typeof value === "number" ? String(value) : "";
};

const oneOf = <T extends string>(value: string, allowed: readonly T[], fallback: T): T =>
  (allowed as readonly string[]).includes(value) ? (value as T) : fallback;

export function emptyFixedDeposit(currency: string, today: string): FixedDepositFormValues {
  return {
    name: "",
    currency,
    issuer: "",
    principal: "",
    interestRate: "",
    interestMethod: "compound",
    compounding: "quarterly",
    dayCount: "act365",
    payout: "cumulative",
    startDate: today,
    maturityDate: "",
    actualMaturityAmount: "",
    taxRate: "",
    status: "active",
    closedDate: "",
    notes: "",
  };
}

export function fixedDepositFromMetadata(
  name: string,
  currency: string,
  notes: string | null | undefined,
  metadata: Record<string, unknown> | undefined,
): FixedDepositFormValues {
  return {
    name,
    currency,
    issuer: str(metadata, "fd_issuer"),
    principal: str(metadata, "fd_principal"),
    interestRate: str(metadata, "fd_interest_rate"),
    interestMethod: oneOf(str(metadata, "fd_interest_method"), ["simple", "compound"], "compound"),
    compounding: oneOf(
      str(metadata, "fd_compounding"),
      ["monthly", "quarterly", "half_yearly", "yearly"],
      "quarterly",
    ),
    dayCount: oneOf(
      str(metadata, "fd_day_count"),
      ["act365", "act360", "act_act", "thirty360"],
      "act365",
    ),
    payout: oneOf(str(metadata, "fd_interest_payout"), ["cumulative", "periodic"], "cumulative"),
    startDate: str(metadata, "fd_start_date"),
    maturityDate: str(metadata, "fd_maturity_date"),
    actualMaturityAmount: str(metadata, "fd_actual_maturity_amount"),
    taxRate: str(metadata, "fd_tax_rate"),
    status: oneOf(str(metadata, "fd_status"), ["active", "closed"], "active"),
    closedDate: str(metadata, "fd_closed_date"),
    notes: notes ?? "",
  };
}

/**
 * Metadata for a fixed deposit. Empty strings remove a key on update (the
 * server treats "" as "delete"), so optional fields can be cleared.
 */
export function fixedDepositMetadata(values: FixedDepositFormValues): Record<string, string> {
  return {
    sub_type: FIXED_DEPOSIT_SUBTYPE,
    fd_issuer: values.issuer.trim(),
    fd_principal: values.principal.trim(),
    fd_interest_rate: values.interestRate.trim(),
    fd_interest_method: values.interestMethod,
    fd_compounding: values.interestMethod === "compound" ? values.compounding : "",
    fd_day_count: values.dayCount,
    fd_interest_payout: values.payout,
    fd_start_date: values.startDate,
    fd_maturity_date: values.maturityDate,
    fd_actual_maturity_amount: values.actualMaturityAmount.trim(),
    fd_tax_rate: values.taxRate.trim(),
    fd_status: values.status,
    fd_closed_date: values.status === "closed" ? values.closedDate : "",
    // Shown as the cost basis by the generic holdings views.
    purchase_price: values.principal.trim(),
    purchase_date: values.startDate,
  };
}

export function emptyPolicy(currency: string, today: string): PolicyFormValues {
  return {
    name: "",
    currency,
    insurer: "LIC",
    policyNumber: "",
    policyType: "endowment",
    premiumAmount: "",
    frequency: "yearly",
    frequencyMonths: "",
    commencementDate: today,
    premiumStartDate: "",
    policyTermYears: "",
    premiumTermYears: "",
    maturityDate: "",
    sumAssured: "",
    bonus: "",
    expectedMaturity: "",
    assumedReturn: "",
    premiumsPaidBefore: "0",
    status: "in_force",
    closedDate: "",
    valuation: "premiums_paid",
    notes: "",
  };
}

export function policyFromMetadata(
  name: string,
  currency: string,
  notes: string | null | undefined,
  metadata: Record<string, unknown> | undefined,
): PolicyFormValues {
  return {
    name,
    currency,
    insurer: str(metadata, "policy_insurer"),
    policyNumber: str(metadata, "policy_number"),
    policyType: str(metadata, "policy_type"),
    premiumAmount: str(metadata, "policy_premium_amount"),
    frequency: oneOf(
      str(metadata, "policy_premium_frequency"),
      ["monthly", "quarterly", "half_yearly", "yearly", "single", "custom"],
      "yearly",
    ),
    frequencyMonths: str(metadata, "policy_frequency_months"),
    commencementDate: str(metadata, "policy_commencement_date"),
    premiumStartDate: str(metadata, "policy_premium_start_date"),
    policyTermYears: str(metadata, "policy_term_years"),
    premiumTermYears: str(metadata, "policy_premium_term_years"),
    maturityDate: str(metadata, "policy_maturity_date"),
    sumAssured: str(metadata, "policy_sum_assured"),
    bonus: str(metadata, "policy_bonus"),
    expectedMaturity: str(metadata, "policy_expected_maturity"),
    assumedReturn: str(metadata, "policy_assumed_return"),
    premiumsPaidBefore: str(metadata, "policy_premiums_paid_before") || "0",
    status: oneOf(
      str(metadata, "policy_status"),
      ["in_force", "paid_up", "lapsed", "matured", "surrendered"],
      "in_force",
    ),
    closedDate: str(metadata, "policy_closed_date"),
    valuation: oneOf(
      str(metadata, "policy_valuation"),
      ["premiums_paid", "manual"],
      "premiums_paid",
    ),
    notes: notes ?? "",
  };
}

export function policyMetadata(values: PolicyFormValues): Record<string, string> {
  const single = values.frequency === "single";
  return {
    sub_type: INSURANCE_POLICY_SUBTYPE,
    policy_insurer: values.insurer.trim(),
    policy_number: values.policyNumber.trim(),
    policy_type: values.policyType.trim(),
    policy_premium_amount: values.premiumAmount.trim(),
    policy_premium_frequency: values.frequency,
    policy_frequency_months: values.frequency === "custom" ? values.frequencyMonths.trim() : "",
    policy_commencement_date: values.commencementDate,
    policy_premium_start_date: values.premiumStartDate,
    policy_term_years: values.policyTermYears.trim(),
    policy_premium_term_years: single ? "" : values.premiumTermYears.trim(),
    policy_maturity_date: values.maturityDate,
    policy_sum_assured: values.sumAssured.trim(),
    policy_bonus: values.bonus.trim(),
    policy_expected_maturity: values.expectedMaturity.trim(),
    policy_assumed_return: values.assumedReturn.trim(),
    policy_premiums_paid_before: values.premiumsPaidBefore.trim() || "0",
    policy_status: values.status,
    policy_closed_date:
      values.status === "matured" || values.status === "surrendered" ? values.closedDate : "",
    policy_valuation: values.valuation,
  };
}

/** Create requests drop empty keys; the server stores only what is set. */
export function withoutEmpty(metadata: Record<string, string>): Record<string, string> {
  return Object.fromEntries(Object.entries(metadata).filter(([, v]) => v !== ""));
}

/** Client-side checks that give immediate feedback; the server validates again. */
export function fixedDepositErrors(values: FixedDepositFormValues): string[] {
  const errors: string[] = [];
  const principal = Number(values.principal);
  const rate = Number(values.interestRate);
  if (!values.name.trim()) errors.push("name");
  if (!values.principal.trim() || !Number.isFinite(principal) || principal <= 0)
    errors.push("principal");
  if (!values.interestRate.trim() || !Number.isFinite(rate) || rate < 0 || rate > 100)
    errors.push("interestRate");
  if (!values.startDate) errors.push("startDate");
  if (!values.maturityDate || (values.startDate && values.maturityDate <= values.startDate))
    errors.push("maturityDate");
  if (values.actualMaturityAmount.trim()) {
    const actual = Number(values.actualMaturityAmount);
    if (!Number.isFinite(actual) || actual < principal) errors.push("actualMaturityAmount");
  }
  if (values.taxRate.trim()) {
    const tax = Number(values.taxRate);
    if (!Number.isFinite(tax) || tax < 0 || tax > 100) errors.push("taxRate");
  }
  return errors;
}

export function policyErrors(values: PolicyFormValues): string[] {
  const errors: string[] = [];
  const premium = Number(values.premiumAmount);
  const whole = (v: string) => v.trim() === "" || /^\d+$/.test(v.trim());
  if (!values.name.trim()) errors.push("name");
  if (!values.premiumAmount.trim() || !Number.isFinite(premium) || premium <= 0)
    errors.push("premiumAmount");
  if (!values.commencementDate) errors.push("commencementDate");
  if (values.premiumStartDate && values.premiumStartDate < values.commencementDate)
    errors.push("premiumStartDate");
  if (!whole(values.policyTermYears)) errors.push("policyTermYears");
  if (!whole(values.premiumTermYears)) errors.push("premiumTermYears");
  if (values.frequency !== "single" && !values.premiumTermYears.trim())
    errors.push("premiumTermYears");
  if (
    values.policyTermYears.trim() &&
    values.premiumTermYears.trim() &&
    Number(values.premiumTermYears) > Number(values.policyTermYears)
  )
    errors.push("premiumTermYears");
  if (values.frequency === "custom") {
    const months = Number(values.frequencyMonths);
    if (!Number.isInteger(months) || months < 1 || months > 120) errors.push("frequencyMonths");
  }
  if (values.maturityDate && values.maturityDate <= values.commencementDate)
    errors.push("maturityDate");
  if (!whole(values.premiumsPaidBefore)) errors.push("premiumsPaidBefore");
  return [...new Set(errors)];
}

/** Activity metadata and idempotency key for a premium payment. */
export function premiumPaymentTag(policyId: string, installment: number | null) {
  return {
    metadata: { policy_premium: { policy_id: policyId, installment } },
    // Same key on every device and retry, so the database refuses duplicates.
    idempotencyKey: installment != null ? `policy-premium:${policyId}:${installment}` : undefined,
  };
}
