//! Insurance policies (LIC and similar): premium schedules, payment tracking and
//! maturity estimates.
//!
//! A policy is an alternative asset of kind `OTHER` with
//! `sub_type = "insurance_policy"` and the `policy_*` terms below, stored as
//! strings like the other alternative-asset details.
//!
//! Premium payments are ordinary WITHDRAWAL activities from a cash account,
//! tagged in their metadata with `{"policy_premium": {"policy_id", "installment"}}`
//! and given a deterministic idempotency key, so the database refuses a second
//! copy of the same instalment created by a retry or a second device.
//!
//! LIC maturity benefits depend on the plan, guaranteed additions and bonuses
//! that are declared later. This module therefore never presents a calculated
//! figure as the policy's benefit:
//! - `user` estimate: the amount the user copied from the policy documents;
//! - `calculated` estimate: premiums grown at a user-supplied assumed return,
//!   clearly labelled as an assumption;
//! - `sum assured + declared bonuses`: a reference figure only.
//!
//! Valuation: by default a policy is worth the premiums paid so far (its cost);
//! a user who tracks the surrender value can switch to manual valuations. Cash
//! leaves the paying account when a premium is recorded and the policy grows by
//! the same amount, so a premium is never counted twice.

use std::collections::HashSet;

use chrono::{Months, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::fixed_deposit::{invalid, meta_date, meta_number, meta_str, round_money};
use crate::errors::Error;

/// Time zone that dates premium payments (the settings time zone).
pub type PolicyTimezone = chrono_tz::Tz;

/// `metadata.sub_type` of an insurance policy.
pub const INSURANCE_POLICY_SUBTYPE: &str = "insurance_policy";
/// Activity metadata key that tags a withdrawal as a premium payment.
pub const POLICY_PREMIUM_TAG_KEY: &str = "policy_premium";

pub const POLICY_INSURER_KEY: &str = "policy_insurer";
pub const POLICY_NUMBER_KEY: &str = "policy_number";
pub const POLICY_TYPE_KEY: &str = "policy_type";
pub const POLICY_PREMIUM_KEY: &str = "policy_premium_amount";
pub const POLICY_FREQUENCY_KEY: &str = "policy_premium_frequency";
pub const POLICY_FREQUENCY_MONTHS_KEY: &str = "policy_frequency_months";
pub const POLICY_PREMIUM_START_KEY: &str = "policy_premium_start_date";
pub const POLICY_COMMENCEMENT_KEY: &str = "policy_commencement_date";
pub const POLICY_TERM_KEY: &str = "policy_term_years";
pub const POLICY_PREMIUM_TERM_KEY: &str = "policy_premium_term_years";
pub const POLICY_MATURITY_KEY: &str = "policy_maturity_date";
pub const POLICY_SUM_ASSURED_KEY: &str = "policy_sum_assured";
pub const POLICY_BONUS_KEY: &str = "policy_bonus";
pub const POLICY_EXPECTED_MATURITY_KEY: &str = "policy_expected_maturity";
pub const POLICY_ASSUMED_RETURN_KEY: &str = "policy_assumed_return";
pub const POLICY_PAID_BEFORE_KEY: &str = "policy_premiums_paid_before";
pub const POLICY_STATUS_KEY: &str = "policy_status";
pub const POLICY_CLOSED_DATE_KEY: &str = "policy_closed_date";
pub const POLICY_VALUATION_KEY: &str = "policy_valuation";

/// Every metadata key a policy owns.
pub const INSURANCE_POLICY_FIELDS: &[&str] = &[
    POLICY_INSURER_KEY,
    POLICY_NUMBER_KEY,
    POLICY_TYPE_KEY,
    POLICY_PREMIUM_KEY,
    POLICY_FREQUENCY_KEY,
    POLICY_FREQUENCY_MONTHS_KEY,
    POLICY_PREMIUM_START_KEY,
    POLICY_COMMENCEMENT_KEY,
    POLICY_TERM_KEY,
    POLICY_PREMIUM_TERM_KEY,
    POLICY_MATURITY_KEY,
    POLICY_SUM_ASSURED_KEY,
    POLICY_BONUS_KEY,
    POLICY_EXPECTED_MATURITY_KEY,
    POLICY_ASSUMED_RETURN_KEY,
    POLICY_PAID_BEFORE_KEY,
    POLICY_STATUS_KEY,
    POLICY_CLOSED_DATE_KEY,
    POLICY_VALUATION_KEY,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "months")]
pub enum PremiumFrequency {
    Monthly,
    Quarterly,
    HalfYearly,
    Yearly,
    Single,
    /// Every N months (1–120).
    Custom(u32),
}

impl PremiumFrequency {
    pub const fn months(self) -> Option<u32> {
        match self {
            Self::Monthly => Some(1),
            Self::Quarterly => Some(3),
            Self::HalfYearly => Some(6),
            Self::Yearly => Some(12),
            Self::Single => None,
            Self::Custom(m) => Some(m),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyStatus {
    InForce,
    PaidUp,
    Lapsed,
    Matured,
    Surrendered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyValuation {
    /// Worth the premiums paid so far.
    PremiumsPaid,
    /// Worth the latest manual valuation (e.g. the surrender value).
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaturityEstimateSource {
    /// Copied by the user from the policy documents.
    PolicyDocument,
    /// Premiums grown at the user's assumed return. An assumption, not a benefit.
    AssumedReturn,
}

/// The `policy_premium` tag on a premium withdrawal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyPremiumTag {
    pub policy_id: String,
    /// 1-based instalment number, when the payment settles a scheduled premium.
    #[serde(default)]
    pub installment: Option<u32>,
}

impl PolicyPremiumTag {
    pub fn read(metadata: Option<&Value>) -> Option<Self> {
        serde_json::from_value(metadata?.get(POLICY_PREMIUM_TAG_KEY)?.clone()).ok()
    }

    /// Idempotency key for a scheduled instalment; equal on every device.
    pub fn idempotency_key(policy_id: &str, installment: u32) -> String {
        format!("policy-premium:{policy_id}:{installment}")
    }
}

/// A recorded premium payment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiumPayment {
    pub activity_id: String,
    pub account_id: String,
    pub date: NaiveDate,
    pub amount: f64,
    pub installment: Option<u32>,
}

/// A policy's validated terms.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyTerms {
    pub insurer: Option<String>,
    pub policy_number: Option<String>,
    pub policy_type: Option<String>,
    pub premium_amount: f64,
    pub frequency: PremiumFrequency,
    pub premium_start_date: NaiveDate,
    pub commencement_date: NaiveDate,
    pub policy_term_years: Option<u32>,
    pub premium_term_years: Option<u32>,
    pub maturity_date: Option<NaiveDate>,
    pub sum_assured: Option<f64>,
    pub bonus: Option<f64>,
    pub expected_maturity: Option<f64>,
    pub assumed_return_pct: Option<f64>,
    pub premiums_paid_before: u32,
    pub status: PolicyStatus,
    pub closed_date: Option<NaiveDate>,
    pub valuation: PolicyValuation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaturityEstimate {
    pub amount: f64,
    pub source: MaturityEstimateSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledPremium {
    pub installment: u32,
    pub due_date: NaiveDate,
    pub amount: f64,
    pub paid: bool,
}

/// What the policy page shows on a given date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySummary {
    pub insurer: Option<String>,
    pub policy_number: Option<String>,
    pub policy_type: Option<String>,
    pub premium_amount: f64,
    pub frequency: PremiumFrequency,
    pub status: PolicyStatus,
    pub valuation: PolicyValuation,
    pub commencement_date: NaiveDate,
    pub premium_start_date: NaiveDate,
    pub maturity_date: Option<NaiveDate>,
    pub last_premium_date: Option<NaiveDate>,
    pub total_scheduled_premiums: u32,
    pub total_scheduled_amount: f64,
    pub premiums_paid: u32,
    pub premiums_remaining: u32,
    /// Due on or before `as_of` and not yet paid.
    pub premiums_overdue: u32,
    pub total_premiums_paid: f64,
    pub outstanding_amount: f64,
    pub next_premium_due: Option<NaiveDate>,
    pub sum_assured: Option<f64>,
    pub bonus: Option<f64>,
    /// Sum assured plus declared bonuses; a reference figure, not a promise.
    pub sum_assured_with_bonus: Option<f64>,
    pub user_maturity_estimate: Option<MaturityEstimate>,
    pub calculated_maturity_estimate: Option<MaturityEstimate>,
    pub assumed_return_pct: Option<f64>,
    /// Value used for holdings when valued at premiums paid.
    pub premiums_paid_value: f64,
    pub upcoming: Vec<ScheduledPremium>,
    pub payments: Vec<PremiumPayment>,
    pub as_of: NaiveDate,
}

pub fn is_insurance_policy(metadata: Option<&Value>) -> bool {
    metadata
        .and_then(|m| m.get("sub_type"))
        .and_then(Value::as_str)
        == Some(INSURANCE_POLICY_SUBTYPE)
}

fn whole_number(metadata: &Value, key: &str, label: &str) -> Result<Option<u32>, Error> {
    match meta_number(metadata, key)? {
        None => Ok(None),
        Some(n) if n >= 0.0 && n.fract() == 0.0 && n <= 1200.0 => Ok(Some(n as u32)),
        Some(_) => Err(invalid(format!("{label} must be a whole number"))),
    }
}

fn add_months(date: NaiveDate, months: u32) -> Option<NaiveDate> {
    date.checked_add_months(Months::new(months))
}

impl PolicyTerms {
    pub fn from_metadata(metadata: &Value) -> Result<Self, Error> {
        let premium_amount = meta_number(metadata, POLICY_PREMIUM_KEY)?
            .ok_or_else(|| invalid("Premium amount is required"))?;
        if premium_amount <= 0.0 {
            return Err(invalid("Premium amount must be greater than zero"));
        }
        let frequency = match meta_str(metadata, POLICY_FREQUENCY_KEY) {
            None => return Err(invalid("Premium payment frequency is required")),
            Some("monthly") => PremiumFrequency::Monthly,
            Some("quarterly") => PremiumFrequency::Quarterly,
            Some("half_yearly") => PremiumFrequency::HalfYearly,
            Some("yearly") => PremiumFrequency::Yearly,
            Some("single") => PremiumFrequency::Single,
            Some("custom") => {
                let months = whole_number(
                    metadata,
                    POLICY_FREQUENCY_MONTHS_KEY,
                    "Custom frequency in months",
                )?
                .ok_or_else(|| invalid("Custom frequency needs a number of months"))?;
                if !(1..=120).contains(&months) {
                    return Err(invalid("Custom frequency must be between 1 and 120 months"));
                }
                PremiumFrequency::Custom(months)
            }
            Some(other) => return Err(invalid(format!("Unsupported premium frequency '{other}'"))),
        };
        let commencement_date = meta_date(metadata, POLICY_COMMENCEMENT_KEY)?
            .ok_or_else(|| invalid("Policy commencement date is required"))?;
        let premium_start_date =
            meta_date(metadata, POLICY_PREMIUM_START_KEY)?.unwrap_or(commencement_date);
        if premium_start_date < commencement_date {
            return Err(invalid(
                "Premium start date cannot be before the commencement date",
            ));
        }
        let policy_term_years = whole_number(metadata, POLICY_TERM_KEY, "Policy term")?;
        let premium_term_years =
            whole_number(metadata, POLICY_PREMIUM_TERM_KEY, "Premium payment term")?;
        if policy_term_years == Some(0) || premium_term_years == Some(0) {
            return Err(invalid(
                "Policy and premium terms must be at least one year",
            ));
        }
        if let (Some(policy), Some(premium)) = (policy_term_years, premium_term_years) {
            if premium > policy {
                return Err(invalid(
                    "Premium payment term cannot be longer than the policy term",
                ));
            }
        }
        if frequency != PremiumFrequency::Single && premium_term_years.is_none() {
            return Err(invalid(
                "Premium payment term is required for recurring premiums",
            ));
        }
        let maturity_date = match meta_date(metadata, POLICY_MATURITY_KEY)? {
            Some(date) => Some(date),
            None => match policy_term_years {
                Some(years) => Some(
                    add_months(commencement_date, years * 12)
                        .ok_or_else(|| invalid("Policy term is too long"))?,
                ),
                None => None,
            },
        };
        if maturity_date.is_some_and(|m| m <= commencement_date) {
            return Err(invalid("Maturity date must be after the commencement date"));
        }
        let non_negative = |key: &str, label: &str| -> Result<Option<f64>, Error> {
            match meta_number(metadata, key)? {
                Some(v) if v < 0.0 => Err(invalid(format!("{label} cannot be negative"))),
                other => Ok(other),
            }
        };
        let sum_assured = non_negative(POLICY_SUM_ASSURED_KEY, "Sum assured")?;
        let bonus = non_negative(POLICY_BONUS_KEY, "Bonus")?;
        let expected_maturity =
            non_negative(POLICY_EXPECTED_MATURITY_KEY, "Expected maturity benefit")?;
        let assumed_return_pct = meta_number(metadata, POLICY_ASSUMED_RETURN_KEY)?;
        if assumed_return_pct.is_some_and(|r| !(0.0..=50.0).contains(&r)) {
            return Err(invalid("Assumed return must be between 0% and 50%"));
        }
        let premiums_paid_before =
            whole_number(metadata, POLICY_PAID_BEFORE_KEY, "Premiums already paid")?.unwrap_or(0);
        let status: PolicyStatus = match meta_str(metadata, POLICY_STATUS_KEY) {
            None => PolicyStatus::InForce,
            Some(s) => serde_json::from_value(Value::String(s.to_string()))
                .map_err(|_| invalid(format!("Unsupported policy status '{s}'")))?,
        };
        let closed_date = meta_date(metadata, POLICY_CLOSED_DATE_KEY)?;
        let valuation = match meta_str(metadata, POLICY_VALUATION_KEY) {
            None | Some("premiums_paid") => PolicyValuation::PremiumsPaid,
            Some("manual") => PolicyValuation::Manual,
            Some(other) => return Err(invalid(format!("Unsupported valuation method '{other}'"))),
        };
        let terms = Self {
            insurer: meta_str(metadata, POLICY_INSURER_KEY).map(str::to_string),
            policy_number: meta_str(metadata, POLICY_NUMBER_KEY).map(str::to_string),
            policy_type: meta_str(metadata, POLICY_TYPE_KEY).map(str::to_string),
            premium_amount,
            frequency,
            premium_start_date,
            commencement_date,
            policy_term_years,
            premium_term_years,
            maturity_date,
            sum_assured,
            bonus,
            expected_maturity,
            assumed_return_pct,
            premiums_paid_before,
            status,
            closed_date,
            valuation,
        };
        if terms.premiums_paid_before > terms.total_premiums() {
            return Err(invalid(
                "Premiums already paid cannot exceed the number of scheduled premiums",
            ));
        }
        Ok(terms)
    }

    /// Number of scheduled premiums over the premium payment term.
    pub fn total_premiums(&self) -> u32 {
        match (self.frequency.months(), self.premium_term_years) {
            (None, _) => 1,
            (Some(months), Some(years)) => ((years * 12) / months).max(1),
            (Some(_), None) => 1,
        }
    }

    /// Due date of instalment `n` (1-based).
    pub fn due_date(&self, installment: u32) -> Option<NaiveDate> {
        if installment == 0 || installment > self.total_premiums() {
            return None;
        }
        match self.frequency.months() {
            None => Some(self.premium_start_date),
            // From the start date each time, so month-end dates do not drift.
            Some(months) => add_months(self.premium_start_date, (installment - 1) * months),
        }
    }

    pub fn schedule(&self) -> Vec<NaiveDate> {
        (1..=self.total_premiums())
            .filter_map(|n| self.due_date(n))
            .collect()
    }

    /// The recorded payments that count, one per instalment. A payment tagged
    /// with an instalment that is already paid (by an earlier payment or by the
    /// premiums paid before tracking) is a duplicate and does not count.
    pub fn counted_payments(&self, payments: &[PremiumPayment]) -> Vec<PremiumPayment> {
        let mut sorted: Vec<PremiumPayment> = payments
            .iter()
            .filter(|p| p.amount.is_finite() && p.amount > 0.0)
            .cloned()
            .collect();
        sorted.sort_by(|a, b| a.date.cmp(&b.date).then(a.activity_id.cmp(&b.activity_id)));
        let mut seen: HashSet<u32> = (1..=self.premiums_paid_before).collect();
        let mut seen_ids = HashSet::new();
        sorted
            .into_iter()
            .filter(|p| seen_ids.insert(p.activity_id.clone()))
            .filter(|p| match p.installment {
                Some(n) => seen.insert(n),
                None => true,
            })
            .collect()
    }

    /// Premiums paid as of `as_of`, counting pre-tracking premiums on their due dates.
    pub fn premiums_paid_on(&self, payments: &[PremiumPayment], as_of: NaiveDate) -> (u32, f64) {
        let before: Vec<NaiveDate> = (1..=self.premiums_paid_before)
            .filter_map(|n| self.due_date(n))
            .filter(|d| *d <= as_of)
            .collect();
        let recorded: Vec<PremiumPayment> = self
            .counted_payments(payments)
            .into_iter()
            .filter(|p| p.date <= as_of)
            .collect();
        let count = before.len() as u32 + recorded.len() as u32;
        let amount = before.len() as f64 * self.premium_amount
            + recorded.iter().map(|p| p.amount).sum::<f64>();
        (count, round_money(amount))
    }

    /// Premiums grown at the assumed return to the maturity date (annual
    /// compounding, actual/365). `None` without an assumed return or maturity date.
    pub fn calculated_maturity_estimate(&self) -> Option<MaturityEstimate> {
        let rate = self.assumed_return_pct? / 100.0;
        let maturity = self.maturity_date?;
        let amount: f64 = self
            .schedule()
            .into_iter()
            .filter(|d| *d <= maturity)
            .map(|due| {
                let years = (maturity - due).num_days() as f64 / 365.0;
                self.premium_amount * (1.0 + rate).powf(years)
            })
            .sum();
        Some(MaturityEstimate {
            amount: round_money(amount),
            source: MaturityEstimateSource::AssumedReturn,
        })
    }

    fn closing_date(&self) -> Option<NaiveDate> {
        match self.status {
            PolicyStatus::Matured => self.closed_date.or(self.maturity_date),
            PolicyStatus::Surrendered => self.closed_date,
            _ => None,
        }
    }

    /// Value on `as_of` when valued at premiums paid; `None` for manually
    /// valued policies (their valuation quotes apply) and before the policy starts.
    pub fn value_on(&self, payments: &[PremiumPayment], as_of: NaiveDate) -> Option<f64> {
        if self.valuation == PolicyValuation::Manual || as_of < self.commencement_date {
            return None;
        }
        if self.closing_date().is_some_and(|d| as_of >= d) {
            return Some(0.0);
        }
        Some(self.premiums_paid_on(payments, as_of).1)
    }

    /// Dates on which the premiums-paid value changes, for history charts.
    pub fn value_change_dates(&self, payments: &[PremiumPayment]) -> Vec<NaiveDate> {
        let mut dates: Vec<NaiveDate> = (1..=self.premiums_paid_before)
            .filter_map(|n| self.due_date(n))
            .chain(self.counted_payments(payments).iter().map(|p| p.date))
            .chain(std::iter::once(self.commencement_date))
            .chain(self.closing_date())
            .collect();
        dates.sort();
        dates.dedup();
        dates
    }

    pub fn summary(&self, payments: &[PremiumPayment], as_of: NaiveDate) -> PolicySummary {
        let counted = self.counted_payments(payments);
        let total = self.total_premiums();
        let paid = (self.premiums_paid_before + counted.len() as u32).min(total);
        let total_premiums_paid = round_money(
            self.premiums_paid_before as f64 * self.premium_amount
                + counted.iter().map(|p| p.amount).sum::<f64>(),
        );
        let paid_installments: HashSet<u32> = (1..=self.premiums_paid_before)
            .chain(counted.iter().filter_map(|p| p.installment))
            .collect();
        // Untagged payments settle the earliest unpaid instalments.
        let mut untagged = counted.iter().filter(|p| p.installment.is_none()).count();
        let schedule: Vec<ScheduledPremium> = (1..=total)
            .filter_map(|n| {
                let due_date = self.due_date(n)?;
                let mut is_paid = paid_installments.contains(&n);
                if !is_paid && untagged > 0 {
                    untagged -= 1;
                    is_paid = true;
                }
                Some(ScheduledPremium {
                    installment: n,
                    due_date,
                    amount: self.premium_amount,
                    paid: is_paid,
                })
            })
            .collect();
        let accepting_premiums =
            matches!(self.status, PolicyStatus::InForce | PolicyStatus::Lapsed);
        let unpaid: Vec<&ScheduledPremium> = schedule.iter().filter(|s| !s.paid).collect();
        let next_premium_due = accepting_premiums
            .then(|| unpaid.first().map(|s| s.due_date))
            .flatten();
        let premiums_overdue = if accepting_premiums {
            unpaid.iter().filter(|s| s.due_date <= as_of).count() as u32
        } else {
            0
        };
        let premiums_remaining = if accepting_premiums {
            unpaid.len() as u32
        } else {
            0
        };
        PolicySummary {
            insurer: self.insurer.clone(),
            policy_number: self.policy_number.clone(),
            policy_type: self.policy_type.clone(),
            premium_amount: self.premium_amount,
            frequency: self.frequency,
            status: self.status,
            valuation: self.valuation,
            commencement_date: self.commencement_date,
            premium_start_date: self.premium_start_date,
            maturity_date: self.maturity_date,
            last_premium_date: self.due_date(total),
            total_scheduled_premiums: total,
            total_scheduled_amount: round_money(total as f64 * self.premium_amount),
            premiums_paid: paid,
            premiums_remaining,
            premiums_overdue,
            total_premiums_paid,
            outstanding_amount: round_money(premiums_overdue as f64 * self.premium_amount),
            next_premium_due,
            sum_assured: self.sum_assured,
            bonus: self.bonus,
            sum_assured_with_bonus: self
                .sum_assured
                .map(|s| round_money(s + self.bonus.unwrap_or(0.0))),
            user_maturity_estimate: self.expected_maturity.map(|amount| MaturityEstimate {
                amount,
                source: MaturityEstimateSource::PolicyDocument,
            }),
            calculated_maturity_estimate: self.calculated_maturity_estimate(),
            assumed_return_pct: self.assumed_return_pct,
            premiums_paid_value: self
                .value_on(payments, as_of)
                .unwrap_or(total_premiums_paid),
            upcoming: unpaid
                .into_iter()
                .filter(|_| accepting_premiums)
                .take(12)
                .cloned()
                .collect(),
            payments: counted,
            as_of,
        }
    }
}

#[cfg(test)]
#[path = "insurance_policy_tests.rs"]
mod tests;
