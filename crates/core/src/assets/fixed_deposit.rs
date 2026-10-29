//! Fixed deposits: terms stored on an alternative asset and the calculations
//! that value it.
//!
//! A fixed deposit is an alternative asset of kind `OTHER` whose metadata has
//! `sub_type = "fixed_deposit"` and the `fd_*` terms below. Terms are stored as
//! strings, like every other alternative-asset detail, so they are edited
//! through the existing "update asset details" path and travel with the asset
//! through export, backup and device sync without a schema change.
//!
//! The module reads neither the clock nor storage. Callers pass the date.
//!
//! Valuation contract (used by holdings and net worth):
//! - before the start date the deposit has no value;
//! - between start and maturity it is worth principal plus interest accrued so
//!   far (for interest-payout deposits, the principal only, because the interest
//!   is paid out to a bank account);
//! - from maturity it is worth the maturity amount (the issuer's figure when the
//!   user entered one, otherwise the calculated estimate), until it is closed;
//! - a closed deposit is worth nothing from its closing date.
//!
//! The full maturity amount is never added on top of the principal: the value
//! on any date is a single figure, so the investment is not counted twice.

use chrono::{Datelike, NaiveDate};
use rust_decimal::prelude::RoundingStrategy;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{Error, ValidationError};

/// `metadata.sub_type` of a fixed deposit.
pub const FIXED_DEPOSIT_SUBTYPE: &str = "fixed_deposit";

pub const FD_ISSUER_KEY: &str = "fd_issuer";
pub const FD_PRINCIPAL_KEY: &str = "fd_principal";
pub const FD_RATE_KEY: &str = "fd_interest_rate";
pub const FD_METHOD_KEY: &str = "fd_interest_method";
pub const FD_COMPOUNDING_KEY: &str = "fd_compounding";
pub const FD_DAY_COUNT_KEY: &str = "fd_day_count";
pub const FD_START_KEY: &str = "fd_start_date";
pub const FD_MATURITY_KEY: &str = "fd_maturity_date";
pub const FD_ACTUAL_MATURITY_KEY: &str = "fd_actual_maturity_amount";
pub const FD_TAX_RATE_KEY: &str = "fd_tax_rate";
pub const FD_PAYOUT_KEY: &str = "fd_interest_payout";
pub const FD_STATUS_KEY: &str = "fd_status";
pub const FD_CLOSED_DATE_KEY: &str = "fd_closed_date";

/// Every metadata key a fixed deposit owns.
pub const FIXED_DEPOSIT_FIELDS: &[&str] = &[
    FD_ISSUER_KEY,
    FD_PRINCIPAL_KEY,
    FD_RATE_KEY,
    FD_METHOD_KEY,
    FD_COMPOUNDING_KEY,
    FD_DAY_COUNT_KEY,
    FD_START_KEY,
    FD_MATURITY_KEY,
    FD_ACTUAL_MATURITY_KEY,
    FD_TAX_RATE_KEY,
    FD_PAYOUT_KEY,
    FD_STATUS_KEY,
    FD_CLOSED_DATE_KEY,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepositInterestMethod {
    Simple,
    Compound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompoundingFrequency {
    Monthly,
    Quarterly,
    HalfYearly,
    Yearly,
}

impl CompoundingFrequency {
    pub const fn periods_per_year(self) -> f64 {
        match self {
            Self::Monthly => 12.0,
            Self::Quarterly => 4.0,
            Self::HalfYearly => 2.0,
            Self::Yearly => 1.0,
        }
    }
}

/// Day-count convention that turns the deposit's dates into years.
///
/// Banks differ; Indian bank FDs normally use actual/365. The stored setting
/// lets a deposit match its certificate. Lender business-day adjustments and
/// the rule some banks use of paying simple interest on very short tenors are
/// not modelled: enter the issuer's maturity amount for an exact figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayCount {
    /// Actual days / 365.
    Act365,
    /// Actual days / 360.
    Act360,
    /// Actual days, each calendar year divided by its own length (365 or 366).
    ActAct,
    /// 30E/360: every month counts as 30 days.
    Thirty360,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterestPayout {
    /// Interest is reinvested and paid at maturity.
    Cumulative,
    /// Interest is paid out periodically; the deposit stays at its principal.
    Periodic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepositStatus {
    /// Before the start date.
    Upcoming,
    Active,
    Matured,
    Closed,
}

/// A fixed deposit's validated terms.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedDepositTerms {
    pub issuer: Option<String>,
    pub principal: f64,
    /// Annual rate in percent (7.1 means 7.1% a year).
    pub annual_rate_pct: f64,
    pub interest_method: DepositInterestMethod,
    /// Required for compound interest; ignored for simple interest.
    pub compounding: Option<CompoundingFrequency>,
    pub day_count: DayCount,
    pub start_date: NaiveDate,
    pub maturity_date: NaiveDate,
    /// Maturity amount from the issuer or the deposit certificate.
    pub actual_maturity_amount: Option<f64>,
    /// Tax withheld on interest, in percent (e.g. TDS of 10%).
    pub tax_rate_pct: Option<f64>,
    pub payout: InterestPayout,
    /// Set when the user closed the deposit (withdrawn or paid out).
    pub closed: bool,
    pub closed_date: Option<NaiveDate>,
}

/// What the asset pages show for a fixed deposit on a given date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedDepositSummary {
    pub issuer: Option<String>,
    pub principal: f64,
    pub annual_rate_pct: f64,
    pub interest_method: DepositInterestMethod,
    pub compounding: Option<CompoundingFrequency>,
    pub day_count: DayCount,
    pub payout: InterestPayout,
    pub start_date: NaiveDate,
    pub maturity_date: NaiveDate,
    pub term_days: i64,
    /// Days from `as_of` to maturity; zero once matured.
    pub days_to_maturity: i64,
    /// Calculated from the terms; an estimate, not the issuer's figure.
    pub expected_maturity_amount: f64,
    pub expected_interest: f64,
    /// As entered from the issuer; shown separately from the estimate.
    pub actual_maturity_amount: Option<f64>,
    pub actual_interest: Option<f64>,
    /// Tax on the interest that is used for valuation, when a tax rate is set.
    pub estimated_tax: Option<f64>,
    pub interest_after_tax: Option<f64>,
    /// The figure holdings and net worth use on `as_of`.
    pub current_value: f64,
    pub accrued_interest: f64,
    pub status: DepositStatus,
    pub as_of: NaiveDate,
}

/// Rounds to currency precision (2 dp, half away from zero).
pub fn round_money(value: f64) -> f64 {
    // The shortest decimal that round-trips the float, so 1.005 rounds as 1.005
    // rather than as its binary approximation 1.00499…
    value
        .to_string()
        .parse::<Decimal>()
        .ok()
        .map(|d| d.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
        .and_then(|d| d.to_string().parse::<f64>().ok())
        .unwrap_or(value)
}

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(ValidationError::InvalidInput(message.into()))
}

/// Reads a metadata value stored as a string or a JSON number.
pub(crate) fn meta_str<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub(crate) fn meta_number(metadata: &Value, key: &str) -> Result<Option<f64>, Error> {
    let Some(value) = metadata.get(key) else {
        return Ok(None);
    };
    let parsed = match value {
        Value::Null => return Ok(None),
        Value::Number(n) => n.as_f64(),
        Value::String(s) if s.trim().is_empty() => return Ok(None),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    match parsed {
        Some(n) if n.is_finite() => Ok(Some(n)),
        _ => Err(invalid(format!("'{key}' must be a number"))),
    }
}

pub(crate) fn meta_date(metadata: &Value, key: &str) -> Result<Option<NaiveDate>, Error> {
    match meta_str(metadata, key) {
        None => Ok(None),
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| invalid(format!("'{key}' must be a date (YYYY-MM-DD)"))),
    }
}

fn meta_enum<T: for<'de> Deserialize<'de>>(
    metadata: &Value,
    key: &str,
) -> Result<Option<T>, Error> {
    match meta_str(metadata, key) {
        None => Ok(None),
        Some(s) => serde_json::from_value(Value::String(s.to_string()))
            .map(Some)
            .map_err(|_| invalid(format!("'{key}' has an unsupported value '{s}'"))),
    }
}

/// Whether the metadata describes a fixed deposit.
pub fn is_fixed_deposit(metadata: Option<&Value>) -> bool {
    metadata
        .and_then(|m| m.get("sub_type"))
        .and_then(Value::as_str)
        == Some(FIXED_DEPOSIT_SUBTYPE)
}

impl FixedDepositTerms {
    /// Reads and validates the terms stored on an asset.
    pub fn from_metadata(metadata: &Value) -> Result<Self, Error> {
        let principal = meta_number(metadata, FD_PRINCIPAL_KEY)?
            .ok_or_else(|| invalid("Principal amount is required"))?;
        if principal <= 0.0 {
            return Err(invalid("Principal amount must be greater than zero"));
        }
        let annual_rate_pct = meta_number(metadata, FD_RATE_KEY)?
            .ok_or_else(|| invalid("Annual interest rate is required"))?;
        if !(0.0..=100.0).contains(&annual_rate_pct) {
            return Err(invalid("Annual interest rate must be between 0% and 100%"));
        }
        let interest_method =
            meta_enum(metadata, FD_METHOD_KEY)?.unwrap_or(DepositInterestMethod::Compound);
        let compounding: Option<CompoundingFrequency> = meta_enum(metadata, FD_COMPOUNDING_KEY)?;
        let compounding = match interest_method {
            DepositInterestMethod::Compound => {
                Some(compounding.ok_or_else(|| invalid("Compounding frequency is required"))?)
            }
            DepositInterestMethod::Simple => None,
        };
        let day_count = meta_enum(metadata, FD_DAY_COUNT_KEY)?.unwrap_or(DayCount::Act365);
        let start_date = meta_date(metadata, FD_START_KEY)?
            .ok_or_else(|| invalid("Deposit start date is required"))?;
        let maturity_date = meta_date(metadata, FD_MATURITY_KEY)?
            .ok_or_else(|| invalid("Maturity date is required"))?;
        if maturity_date <= start_date {
            return Err(invalid("Maturity date must be after the start date"));
        }
        let actual_maturity_amount = meta_number(metadata, FD_ACTUAL_MATURITY_KEY)?;
        if actual_maturity_amount.is_some_and(|a| a < principal) {
            return Err(invalid(
                "Maturity amount cannot be less than the principal amount",
            ));
        }
        let tax_rate_pct = meta_number(metadata, FD_TAX_RATE_KEY)?;
        if tax_rate_pct.is_some_and(|t| !(0.0..=100.0).contains(&t)) {
            return Err(invalid("Tax rate must be between 0% and 100%"));
        }
        let payout = meta_enum(metadata, FD_PAYOUT_KEY)?.unwrap_or(InterestPayout::Cumulative);
        let closed = match meta_str(metadata, FD_STATUS_KEY) {
            None | Some("active") => false,
            Some("closed") => true,
            Some(other) => {
                return Err(invalid(format!(
                    "'{FD_STATUS_KEY}' has an unsupported value '{other}'"
                )))
            }
        };
        let closed_date = meta_date(metadata, FD_CLOSED_DATE_KEY)?;
        if closed_date.is_some_and(|d| d < start_date) {
            return Err(invalid("Closing date cannot be before the start date"));
        }
        Ok(Self {
            issuer: meta_str(metadata, FD_ISSUER_KEY).map(str::to_string),
            principal,
            annual_rate_pct,
            interest_method,
            compounding,
            day_count,
            start_date,
            maturity_date,
            actual_maturity_amount,
            tax_rate_pct,
            payout,
            closed,
            closed_date,
        })
    }

    fn rate(&self) -> f64 {
        self.annual_rate_pct / 100.0
    }

    /// Calculated value of principal plus interest after `years`.
    fn grown(&self, years: f64) -> f64 {
        match (self.interest_method, self.compounding) {
            (DepositInterestMethod::Compound, Some(freq)) => {
                let n = freq.periods_per_year();
                self.principal * (1.0 + self.rate() / n).powf(n * years)
            }
            // Simple interest, and periodic payouts, earn on the principal only.
            _ => self.principal * (1.0 + self.rate() * years),
        }
    }

    /// The calculated maturity amount (an estimate from the terms).
    pub fn expected_maturity_amount(&self) -> f64 {
        let years = year_fraction(self.day_count, self.start_date, self.maturity_date);
        match self.payout {
            // Paid-out interest is not compounded on the deposit.
            InterestPayout::Periodic => round_money(self.principal * (1.0 + self.rate() * years)),
            InterestPayout::Cumulative => round_money(self.grown(years)),
        }
    }

    /// The date the deposit stops counting, if closed.
    fn closing_date(&self) -> Option<NaiveDate> {
        self.closed
            .then(|| self.closed_date.unwrap_or(self.maturity_date))
    }

    pub fn status(&self, as_of: NaiveDate) -> DepositStatus {
        if self.closing_date().is_some_and(|d| as_of >= d) {
            DepositStatus::Closed
        } else if as_of < self.start_date {
            DepositStatus::Upcoming
        } else if as_of >= self.maturity_date {
            DepositStatus::Matured
        } else {
            DepositStatus::Active
        }
    }

    /// The value holdings and net worth use on `as_of`; `None` before the start.
    pub fn value_on(&self, as_of: NaiveDate) -> Option<f64> {
        match self.status(as_of) {
            DepositStatus::Upcoming => None,
            DepositStatus::Closed => Some(0.0),
            DepositStatus::Matured => Some(match self.payout {
                InterestPayout::Periodic => self.principal,
                InterestPayout::Cumulative => self
                    .actual_maturity_amount
                    .unwrap_or_else(|| self.expected_maturity_amount()),
            }),
            DepositStatus::Active => Some(round_money(match self.payout {
                InterestPayout::Periodic => self.principal,
                InterestPayout::Cumulative => {
                    let years = year_fraction(self.day_count, self.start_date, as_of);
                    let accrued = self.grown(years) - self.principal;
                    // An issuer figure scales the calculated accrual so the value
                    // reaches the issuer's amount on the maturity date.
                    let scale = match self.actual_maturity_amount {
                        Some(actual) => {
                            let expected = self.expected_maturity_amount() - self.principal;
                            if expected > 0.0 {
                                (actual - self.principal) / expected
                            } else {
                                1.0
                            }
                        }
                        None => 1.0,
                    };
                    self.principal + accrued * scale
                }
            })),
        }
    }

    pub fn summary(&self, as_of: NaiveDate) -> FixedDepositSummary {
        let expected_maturity_amount = self.expected_maturity_amount();
        let expected_interest = round_money(expected_maturity_amount - self.principal);
        let actual_interest = self
            .actual_maturity_amount
            .map(|a| round_money(a - self.principal));
        let interest_for_tax = actual_interest.unwrap_or(expected_interest);
        let estimated_tax = self
            .tax_rate_pct
            .map(|t| round_money(interest_for_tax * t / 100.0));
        let current_value = self.value_on(as_of).unwrap_or(0.0);
        let accrued_interest = match self.status(as_of) {
            DepositStatus::Active | DepositStatus::Matured => {
                round_money((current_value - self.principal).max(0.0))
            }
            _ => 0.0,
        };
        FixedDepositSummary {
            issuer: self.issuer.clone(),
            principal: self.principal,
            annual_rate_pct: self.annual_rate_pct,
            interest_method: self.interest_method,
            compounding: self.compounding,
            day_count: self.day_count,
            payout: self.payout,
            start_date: self.start_date,
            maturity_date: self.maturity_date,
            term_days: (self.maturity_date - self.start_date).num_days(),
            days_to_maturity: (self.maturity_date - as_of).num_days().max(0),
            expected_maturity_amount,
            expected_interest,
            actual_maturity_amount: self.actual_maturity_amount,
            actual_interest,
            estimated_tax,
            interest_after_tax: estimated_tax.map(|t| round_money(interest_for_tax - t)),
            current_value,
            accrued_interest,
            status: self.status(as_of),
            as_of,
        }
    }
}

fn is_leap(year: i32) -> bool {
    NaiveDate::from_ymd_opt(year, 2, 29).is_some()
}

/// Years between two dates under a day-count convention.
pub fn year_fraction(day_count: DayCount, start: NaiveDate, end: NaiveDate) -> f64 {
    if end <= start {
        return 0.0;
    }
    let days = (end - start).num_days() as f64;
    match day_count {
        DayCount::Act365 => days / 365.0,
        DayCount::Act360 => days / 360.0,
        DayCount::ActAct => {
            let mut total = 0.0;
            let mut cursor = start;
            while cursor < end {
                let next_year = NaiveDate::from_ymd_opt(cursor.year() + 1, 1, 1)
                    .expect("1 January exists in every year");
                let segment_end = next_year.min(end);
                let year_len = if is_leap(cursor.year()) { 366.0 } else { 365.0 };
                total += (segment_end - cursor).num_days() as f64 / year_len;
                cursor = segment_end;
            }
            total
        }
        DayCount::Thirty360 => {
            let d1 = start.day().min(30) as i64;
            let d2 = end.day().min(30) as i64;
            let days = 360 * (end.year() - start.year()) as i64
                + 30 * (end.month() as i64 - start.month() as i64)
                + (d2 - d1);
            days as f64 / 360.0
        }
    }
}

#[cfg(test)]
#[path = "fixed_deposit_tests.rs"]
mod tests;
