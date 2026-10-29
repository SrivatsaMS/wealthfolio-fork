//! Shared valuation of alternative assets whose value is calculated from their
//! terms (fixed deposits, and insurance policies valued at premiums paid).
//!
//! Holdings, current net worth and net-worth history all call these functions,
//! so the three never disagree. Each returns `None` when the asset is not a
//! calculated product, or when its terms do not validate; callers then fall
//! back to the asset's valuation quotes exactly as before.

use chrono::{Months, NaiveDate};
use log::warn;
use rust_decimal::Decimal;
use serde_json::Value;

use super::fixed_deposit::{is_fixed_deposit, FixedDepositTerms, FIXED_DEPOSIT_FIELDS};
use super::insurance_policy::{
    is_insurance_policy, PolicyTerms, PremiumPayment, INSURANCE_POLICY_FIELDS,
};
use super::{Asset, AssetKind};
use crate::errors::Result;

/// The calculated product an asset holds, with its validated terms.
#[derive(Debug, Clone)]
pub enum CalculatedProduct {
    FixedDeposit(FixedDepositTerms),
    InsurancePolicy(PolicyTerms),
}

impl CalculatedProduct {
    /// Reads the product from an asset; `None` for other assets.
    /// Invalid stored terms are logged and treated as "not calculated" so a bad
    /// record cannot break holdings or net worth.
    pub fn of(asset: &Asset) -> Option<Self> {
        if asset.kind != AssetKind::Other {
            return None;
        }
        let metadata = asset.metadata.as_ref()?;
        let parsed = if is_fixed_deposit(Some(metadata)) {
            FixedDepositTerms::from_metadata(metadata).map(Self::FixedDeposit)
        } else if is_insurance_policy(Some(metadata)) {
            PolicyTerms::from_metadata(metadata).map(Self::InsurancePolicy)
        } else {
            return None;
        };
        parsed
            .map_err(|e| warn!("Ignoring invalid terms on asset {}: {}", asset.id, e))
            .ok()
    }

    pub fn is_policy(&self) -> bool {
        matches!(self, Self::InsurancePolicy(_))
    }

    /// Whether the value comes from the terms (and not from valuation quotes).
    pub fn is_valued_from_terms(&self) -> bool {
        match self {
            Self::FixedDeposit(_) => true,
            Self::InsurancePolicy(terms) => {
                terms.valuation == super::insurance_policy::PolicyValuation::PremiumsPaid
            }
        }
    }

    /// Value on `date` in the asset's currency, rounded to cents.
    pub fn value_on(&self, payments: &[PremiumPayment], date: NaiveDate) -> Option<Decimal> {
        let value = match self {
            Self::FixedDeposit(terms) => terms.value_on(date),
            Self::InsurancePolicy(terms) => terms.value_on(payments, date),
        }?;
        Decimal::from_f64_retain(value).map(|v| v.round_dp(2))
    }

    /// Dated values for a history chart between `start` and `end` (inclusive):
    /// every date the value changes, month ends for accruing deposits, and `end`.
    pub fn history_points(
        &self,
        payments: &[PremiumPayment],
        start: NaiveDate,
        end: NaiveDate,
    ) -> Vec<(NaiveDate, Decimal)> {
        let mut dates: Vec<NaiveDate> = match self {
            Self::FixedDeposit(terms) => {
                let mut dates = vec![terms.start_date, terms.maturity_date];
                dates.extend(terms.closed_date);
                // Monthly points while interest accrues.
                let mut cursor = terms.start_date;
                let mut step = 1;
                while cursor < terms.maturity_date.min(end) {
                    let Some(next) = terms.start_date.checked_add_months(Months::new(step)) else {
                        break;
                    };
                    cursor = next;
                    dates.push(cursor);
                    step += 1;
                }
                dates
            }
            Self::InsurancePolicy(terms) => terms.value_change_dates(payments),
        };
        dates.push(start);
        dates.push(end);
        dates.retain(|d| *d >= start && *d <= end);
        dates.sort();
        dates.dedup();
        dates
            .into_iter()
            .filter_map(|d| self.value_on(payments, d).map(|v| (d, v)))
            .collect()
    }
}

/// Checks the terms of a fixed deposit or policy when an asset is created or
/// its details change. Other assets pass unchanged.
pub fn validate_product_terms(kind: &AssetKind, metadata: Option<&Value>) -> Result<()> {
    let Some(metadata) = metadata else {
        return Ok(());
    };
    let product = is_fixed_deposit(Some(metadata)) || is_insurance_policy(Some(metadata));
    if product && *kind != AssetKind::Other {
        return Err(super::fixed_deposit::invalid(
            "Fixed deposits and insurance policies must use the OTHER asset kind",
        ));
    }
    if is_fixed_deposit(Some(metadata)) {
        FixedDepositTerms::from_metadata(metadata)?;
    } else if is_insurance_policy(Some(metadata)) {
        PolicyTerms::from_metadata(metadata)?;
    } else if metadata.as_object().is_some_and(|fields| {
        fields.keys().any(|k| {
            FIXED_DEPOSIT_FIELDS.contains(&k.as_str())
                || INSURANCE_POLICY_FIELDS.contains(&k.as_str())
        })
    }) {
        return Err(super::fixed_deposit::invalid(
            "Deposit and policy terms need sub_type 'fixed_deposit' or 'insurance_policy'",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn asset(kind: AssetKind, metadata: Value) -> Asset {
        Asset {
            id: "A1".into(),
            kind,
            metadata: Some(metadata),
            ..Default::default()
        }
    }

    fn fd() -> Value {
        json!({
            "sub_type": "fixed_deposit",
            "fd_principal": "100000",
            "fd_interest_rate": "7",
            "fd_interest_method": "compound",
            "fd_compounding": "quarterly",
            "fd_start_date": "2025-01-01",
            "fd_maturity_date": "2026-01-01",
        })
    }

    #[test]
    fn values_fixed_deposits_and_ignores_other_assets() {
        let product = CalculatedProduct::of(&asset(AssetKind::Other, fd())).unwrap();
        assert_eq!(
            product.value_on(&[], d(2026, 1, 1)),
            Some(Decimal::new(10718590, 2))
        );
        assert!(CalculatedProduct::of(&asset(AssetKind::Property, fd())).is_none());
        assert!(
            CalculatedProduct::of(&asset(AssetKind::Other, json!({"sub_type": "art"}))).is_none()
        );
        // Broken stored terms fall back to quotes rather than failing.
        let mut broken = fd();
        broken["fd_principal"] = json!("oops");
        assert!(CalculatedProduct::of(&asset(AssetKind::Other, broken)).is_none());
    }

    #[test]
    fn history_points_cover_accrual_and_maturity() {
        let product = CalculatedProduct::of(&asset(AssetKind::Other, fd())).unwrap();
        let points = product.history_points(&[], d(2024, 6, 1), d(2026, 6, 1));
        assert_eq!(points.first().map(|p| p.0), Some(d(2025, 1, 1)));
        assert!(points.iter().any(|p| p.0 == d(2025, 7, 1)));
        assert_eq!(points.last().map(|p| p.0), Some(d(2026, 6, 1)));
        assert!(points.windows(2).all(|w| w[0].1 <= w[1].1));
    }

    #[test]
    fn validation_rejects_bad_terms_and_wrong_kind() {
        assert!(validate_product_terms(&AssetKind::Other, Some(&fd())).is_ok());
        assert!(validate_product_terms(&AssetKind::Property, Some(&fd())).is_err());
        let mut bad = fd();
        bad["fd_maturity_date"] = json!("2024-01-01");
        assert!(validate_product_terms(&AssetKind::Other, Some(&bad)).is_err());
        assert!(
            validate_product_terms(&AssetKind::Other, Some(&json!({"fd_principal": "5"}))).is_err()
        );
        assert!(
            validate_product_terms(&AssetKind::Vehicle, Some(&json!({"sub_type": "car"}))).is_ok()
        );
    }
}
