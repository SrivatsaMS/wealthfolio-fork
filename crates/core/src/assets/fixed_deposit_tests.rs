use super::*;
use serde_json::json;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn fd(overrides: Value) -> Value {
    let mut base = json!({
        "sub_type": "fixed_deposit",
        "fd_issuer": "State Bank of India",
        "fd_principal": "100000",
        "fd_interest_rate": "7",
        "fd_interest_method": "compound",
        "fd_compounding": "quarterly",
        "fd_start_date": "2025-01-01",
        "fd_maturity_date": "2026-01-01",
    });
    for (k, v) in overrides.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

fn terms(overrides: Value) -> FixedDepositTerms {
    FixedDepositTerms::from_metadata(&fd(overrides)).unwrap()
}

fn err(overrides: Value) -> String {
    FixedDepositTerms::from_metadata(&fd(overrides))
        .unwrap_err()
        .to_string()
}

#[test]
fn simple_interest_one_year() {
    let t = terms(json!({"fd_interest_method": "simple"}));
    assert_eq!(t.expected_maturity_amount(), 107000.00);
    assert_eq!(t.summary(d(2025, 6, 1)).expected_interest, 7000.00);
}

#[test]
fn simple_interest_odd_tenor_and_day_counts() {
    // 400 days at 6.8%
    let base = json!({
        "fd_interest_method": "simple",
        "fd_interest_rate": "6.8",
        "fd_maturity_date": "2026-02-05",
    });
    assert_eq!(terms(base.clone()).expected_maturity_amount(), 107452.05);
    let mut act360 = base.clone();
    act360["fd_day_count"] = json!("act360");
    assert_eq!(terms(act360).expected_maturity_amount(), 107555.56);
}

#[test]
fn compound_interest_frequencies() {
    assert_eq!(terms(json!({})).expected_maturity_amount(), 107185.90);
    assert_eq!(
        terms(json!({"fd_compounding": "monthly"})).expected_maturity_amount(),
        107229.01
    );
    assert_eq!(
        terms(json!({
            "fd_compounding": "half_yearly",
            "fd_interest_rate": "6.5",
            "fd_start_date": "2025-01-01",
            "fd_maturity_date": "2027-01-01",
        }))
        .expected_maturity_amount(),
        113647.59
    );
}

#[test]
fn five_year_deposit_depends_on_day_count() {
    let five_years = json!({
        "fd_compounding": "yearly",
        "fd_start_date": "2025-01-01",
        "fd_maturity_date": "2030-01-01",
    });
    // 1826 days include 29 Feb 2028: act/365 sees slightly more than five years.
    assert_eq!(
        terms(five_years.clone()).expected_maturity_amount(),
        140281.17
    );
    let mut actact = five_years;
    actact["fd_day_count"] = json!("act_act");
    assert_eq!(terms(actact).expected_maturity_amount(), 140255.17);
}

#[test]
fn thirty_360_counts_months_as_thirty_days() {
    assert_eq!(
        year_fraction(DayCount::Thirty360, d(2025, 1, 31), d(2025, 7, 31)),
        0.5
    );
    assert_eq!(
        year_fraction(DayCount::ActAct, d(2024, 1, 1), d(2025, 1, 1)),
        1.0
    );
}

#[test]
fn rejects_invalid_dates_and_amounts() {
    assert!(err(json!({"fd_maturity_date": "2024-12-31"})).contains("after the start date"));
    assert!(err(json!({"fd_maturity_date": "2025-01-01"})).contains("after the start date"));
    assert!(err(json!({"fd_start_date": "01/01/2025"})).contains("YYYY-MM-DD"));
    assert!(err(json!({"fd_principal": "0"})).contains("greater than zero"));
    assert!(err(json!({"fd_principal": "-5"})).contains("greater than zero"));
    assert!(err(json!({"fd_principal": "abc"})).contains("must be a number"));
    assert!(err(json!({"fd_interest_rate": "-1"})).contains("between 0%"));
    assert!(err(json!({"fd_interest_rate": "150"})).contains("between 0%"));
    assert!(err(json!({"fd_compounding": ""})).contains("Compounding frequency"));
    assert!(err(json!({"fd_compounding": "daily"})).contains("unsupported value"));
    assert!(err(json!({"fd_actual_maturity_amount": "90000"})).contains("less than the principal"));
    assert!(err(json!({"fd_tax_rate": "101"})).contains("Tax rate"));
}

#[test]
fn user_entered_maturity_is_kept_separate_from_estimate() {
    let t = terms(json!({"fd_actual_maturity_amount": "107190.25", "fd_tax_rate": "10"}));
    let s = t.summary(d(2025, 7, 1));
    assert_eq!(s.expected_maturity_amount, 107185.90);
    assert_eq!(s.actual_maturity_amount, Some(107190.25));
    assert_eq!(s.actual_interest, Some(7190.25));
    // Tax is estimated on the issuer's interest when one is entered.
    assert_eq!(s.estimated_tax, Some(719.03));
    assert_eq!(s.interest_after_tax, Some(6471.22));
    // At maturity the issuer's amount is the value.
    assert_eq!(t.value_on(d(2026, 1, 1)), Some(107190.25));
}

#[test]
fn valuation_accrues_without_double_counting() {
    let t = terms(json!({}));
    assert_eq!(t.value_on(d(2024, 12, 31)), None, "no value before start");
    assert_eq!(t.value_on(d(2025, 1, 1)), Some(100000.00));
    // 181 days at 7% compounded quarterly.
    assert_eq!(t.value_on(d(2025, 7, 1)), Some(103501.10));
    let at_maturity = t.value_on(d(2026, 1, 1)).unwrap();
    assert_eq!(at_maturity, 107185.90);
    // After maturity the value stays at the maturity amount, never principal + maturity.
    assert_eq!(t.value_on(d(2027, 1, 1)), Some(at_maturity));
    let s = t.summary(d(2025, 7, 1));
    assert_eq!(s.accrued_interest, 3501.10);
    assert_eq!(s.status, DepositStatus::Active);
    assert_eq!(s.days_to_maturity, 184);
    assert_eq!(s.term_days, 365);
}

#[test]
fn issuer_amount_scales_accrual_to_reach_it_at_maturity() {
    let t = terms(json!({"fd_actual_maturity_amount": "108000"}));
    let mid = t.value_on(d(2025, 7, 1)).unwrap();
    assert!(mid > 103501.10 && mid < 108000.0);
    assert_eq!(t.value_on(d(2026, 1, 1)), Some(108000.0));
}

#[test]
fn periodic_payout_stays_at_principal() {
    let t = terms(json!({"fd_interest_payout": "periodic"}));
    assert_eq!(t.value_on(d(2025, 7, 1)), Some(100000.0));
    assert_eq!(t.value_on(d(2026, 6, 1)), Some(100000.0));
    // Total interest received over the term is still shown.
    assert_eq!(t.expected_maturity_amount(), 107000.0);
}

#[test]
fn closed_deposit_is_worth_nothing_from_closing_date() {
    let t = terms(json!({"fd_status": "closed", "fd_closed_date": "2025-09-01"}));
    assert_eq!(t.status(d(2025, 8, 31)), DepositStatus::Active);
    assert_eq!(t.value_on(d(2025, 9, 1)), Some(0.0));
    assert_eq!(t.status(d(2025, 9, 1)), DepositStatus::Closed);
    // Closed without a date: counted until maturity.
    let t = terms(json!({"fd_status": "closed"}));
    assert_eq!(t.value_on(d(2026, 1, 1)), Some(0.0));
    assert!(t.value_on(d(2025, 12, 31)).unwrap() > 100000.0);
}

#[test]
fn rounding_uses_currency_precision() {
    assert_eq!(round_money(1.005), 1.01);
    assert_eq!(round_money(2.675), 2.68);
    assert_eq!(round_money(-1.005), -1.01);
    let t = terms(json!({"fd_principal": "1234.56", "fd_interest_rate": "7.35"}));
    let amount = t.expected_maturity_amount();
    assert_eq!(amount, round_money(amount));
}

#[test]
fn numbers_may_be_json_numbers() {
    let t = FixedDepositTerms::from_metadata(&fd(json!({
        "fd_principal": 50000,
        "fd_interest_rate": 6.5,
    })))
    .unwrap();
    assert_eq!(t.principal, 50000.0);
    assert!(is_fixed_deposit(Some(&fd(json!({})))));
    assert!(!is_fixed_deposit(Some(&json!({"sub_type": "gold"}))));
}
