use super::*;
use serde_json::json;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn meta(overrides: Value) -> Value {
    let mut base = json!({
        "sub_type": "insurance_policy",
        "policy_insurer": "LIC",
        "policy_number": "123456789",
        "policy_type": "endowment",
        "policy_premium_amount": "10000",
        "policy_premium_frequency": "yearly",
        "policy_commencement_date": "2020-01-01",
        "policy_term_years": "10",
        "policy_premium_term_years": "10",
    });
    for (k, v) in overrides.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

fn terms(overrides: Value) -> PolicyTerms {
    PolicyTerms::from_metadata(&meta(overrides)).unwrap()
}

fn err(overrides: Value) -> String {
    PolicyTerms::from_metadata(&meta(overrides))
        .unwrap_err()
        .to_string()
}

fn pay(id: &str, date: NaiveDate, amount: f64, installment: Option<u32>) -> PremiumPayment {
    PremiumPayment {
        activity_id: id.into(),
        account_id: "acc".into(),
        date,
        amount,
        installment,
    }
}

#[test]
fn monthly_schedule() {
    let t = terms(json!({
        "policy_premium_frequency": "monthly",
        "policy_premium_term_years": "2",
        "policy_commencement_date": "2024-01-31",
        "policy_term_years": "5",
    }));
    assert_eq!(t.total_premiums(), 24);
    // Month-end dates do not drift after February.
    assert_eq!(t.due_date(2), Some(d(2024, 2, 29)));
    assert_eq!(t.due_date(3), Some(d(2024, 3, 31)));
    assert_eq!(t.due_date(24), Some(d(2025, 12, 31)));
    assert_eq!(t.due_date(25), None);
}

#[test]
fn quarterly_half_yearly_and_yearly_schedules() {
    let q =
        terms(json!({"policy_premium_frequency": "quarterly", "policy_premium_term_years": "3"}));
    assert_eq!(q.total_premiums(), 12);
    assert_eq!(q.due_date(2), Some(d(2020, 4, 1)));
    let h =
        terms(json!({"policy_premium_frequency": "half_yearly", "policy_premium_term_years": "3"}));
    assert_eq!(h.total_premiums(), 6);
    assert_eq!(h.due_date(6), Some(d(2022, 7, 1)));
    let y = terms(json!({}));
    assert_eq!(y.total_premiums(), 10);
    assert_eq!(y.schedule().last(), Some(&d(2029, 1, 1)));
}

#[test]
fn single_premium_and_custom_frequency() {
    let s = terms(json!({"policy_premium_frequency": "single", "policy_premium_term_years": ""}));
    assert_eq!(s.total_premiums(), 1);
    assert_eq!(s.schedule(), vec![d(2020, 1, 1)]);
    let c = terms(json!({
        "policy_premium_frequency": "custom",
        "policy_frequency_months": "4",
        "policy_premium_term_years": "2",
    }));
    assert_eq!(c.frequency, PremiumFrequency::Custom(4));
    assert_eq!(c.total_premiums(), 6);
    assert!(err(json!({"policy_premium_frequency": "custom"})).contains("number of months"));
    assert!(
        err(json!({"policy_premium_frequency": "custom", "policy_frequency_months": "0"}))
            .contains("between 1 and 120")
    );
}

#[test]
fn policy_and_premium_terms_are_validated() {
    assert!(err(json!({"policy_premium_term_years": "12"})).contains("cannot be longer"));
    assert!(err(json!({"policy_premium_term_years": ""})).contains("required for recurring"));
    assert!(err(json!({"policy_premium_amount": "0"})).contains("greater than zero"));
    assert!(err(json!({"policy_maturity_date": "2019-01-01"})).contains("after the commencement"));
    assert!(
        err(json!({"policy_premium_start_date": "2019-06-01"})).contains("before the commencement")
    );
    assert!(err(json!({"policy_premiums_paid_before": "11"})).contains("cannot exceed"));
    assert!(err(json!({"policy_premiums_paid_before": "1.5"})).contains("whole number"));
    // Maturity follows the policy term when not entered.
    assert_eq!(terms(json!({})).maturity_date, Some(d(2030, 1, 1)));
    // A shorter premium term than policy term (limited-pay plan).
    let limited = terms(json!({"policy_premium_term_years": "6"}));
    assert_eq!(limited.total_premiums(), 6);
}

#[test]
fn next_payment_and_totals() {
    let t = terms(json!({"policy_premiums_paid_before": "5"}));
    let payments = vec![pay("a1", d(2025, 1, 3), 10000.0, Some(6))];
    let s = t.summary(&payments, d(2026, 6, 1));
    assert_eq!(s.premiums_paid, 6);
    assert_eq!(s.total_premiums_paid, 60000.0);
    assert_eq!(s.total_scheduled_premiums, 10);
    assert_eq!(s.total_scheduled_amount, 100000.0);
    assert_eq!(s.premiums_remaining, 4);
    assert_eq!(s.next_premium_due, Some(d(2026, 1, 1)));
    // 2026-01-01 is due and unpaid on 2026-06-01.
    assert_eq!(s.premiums_overdue, 1);
    assert_eq!(s.outstanding_amount, 10000.0);
    assert_eq!(s.last_premium_date, Some(d(2029, 1, 1)));
    assert_eq!(s.upcoming.first().map(|u| u.installment), Some(7));
}

#[test]
fn duplicate_payments_do_not_count_twice() {
    let t = terms(json!({"policy_premiums_paid_before": "2"}));
    let payments = vec![
        pay("a1", d(2022, 1, 2), 10000.0, Some(3)),
        // Same instalment recorded again (e.g. a retried request on another device).
        pay("a2", d(2022, 1, 5), 10000.0, Some(3)),
        // Same activity reported twice.
        pay("a1", d(2022, 1, 2), 10000.0, Some(3)),
        // Instalment already covered by pre-tracking premiums.
        pay("a3", d(2021, 1, 2), 10000.0, Some(2)),
    ];
    let s = t.summary(&payments, d(2022, 6, 1));
    assert_eq!(s.premiums_paid, 3);
    assert_eq!(s.total_premiums_paid, 30000.0);
    assert_eq!(s.payments.len(), 1);
    assert_eq!(s.payments[0].activity_id, "a1");
    assert_eq!(
        PolicyPremiumTag::idempotency_key("POL1", 3),
        "policy-premium:POL1:3"
    );
}

#[test]
fn untagged_payments_settle_earliest_unpaid_instalments() {
    let t = terms(json!({}));
    let payments = vec![
        pay("a1", d(2020, 1, 1), 10000.0, None),
        pay("a2", d(2021, 1, 1), 10000.0, None),
    ];
    let s = t.summary(&payments, d(2021, 2, 1));
    assert_eq!(s.premiums_paid, 2);
    assert_eq!(s.next_premium_due, Some(d(2022, 1, 1)));
    assert_eq!(s.premiums_overdue, 0);
}

#[test]
fn valuation_at_premiums_paid_over_time() {
    let t = terms(json!({"policy_premiums_paid_before": "2"}));
    let payments = vec![pay("a1", d(2022, 1, 10), 10000.0, Some(3))];
    assert_eq!(t.value_on(&payments, d(2019, 12, 31)), None);
    assert_eq!(t.value_on(&payments, d(2020, 6, 1)), Some(10000.0));
    assert_eq!(t.value_on(&payments, d(2021, 6, 1)), Some(20000.0));
    assert_eq!(t.value_on(&payments, d(2022, 1, 9)), Some(20000.0));
    assert_eq!(t.value_on(&payments, d(2022, 1, 10)), Some(30000.0));
    assert!(t.value_change_dates(&payments).contains(&d(2022, 1, 10)));
    // Manual valuation defers to valuation quotes.
    let manual = terms(json!({"policy_valuation": "manual"}));
    assert_eq!(manual.value_on(&payments, d(2022, 6, 1)), None);
    // A matured policy has paid out.
    let matured = terms(json!({"policy_status": "matured", "policy_premiums_paid_before": "10"}));
    assert_eq!(matured.value_on(&[], d(2029, 12, 31)), Some(100000.0));
    assert_eq!(matured.value_on(&[], d(2030, 1, 1)), Some(0.0));
    let s = matured.summary(&[], d(2030, 2, 1));
    assert_eq!(s.next_premium_due, None);
    assert_eq!(s.premiums_remaining, 0);
}

#[test]
fn maturity_estimates_are_labelled_and_separate() {
    let t = terms(json!({
        "policy_assumed_return": "5",
        "policy_sum_assured": "100000",
        "policy_bonus": "25000",
        "policy_expected_maturity": "145000",
    }));
    let s = t.summary(&[], d(2022, 1, 1));
    let calculated = s.calculated_maturity_estimate.unwrap();
    assert_eq!(calculated.source, MaturityEstimateSource::AssumedReturn);
    assert_eq!(calculated.amount, 132096.2);
    let user = s.user_maturity_estimate.unwrap();
    assert_eq!(user.source, MaturityEstimateSource::PolicyDocument);
    assert_eq!(user.amount, 145000.0);
    assert_eq!(s.sum_assured_with_bonus, Some(125000.0));
    // No assumed return: no calculated estimate.
    let none = terms(json!({}));
    assert!(none
        .summary(&[], d(2022, 1, 1))
        .calculated_maturity_estimate
        .is_none());
    // No maturity date and no term: no calculated estimate either.
    let whole_life = terms(json!({
        "policy_term_years": "",
        "policy_assumed_return": "5",
    }));
    assert_eq!(whole_life.maturity_date, None);
    assert!(whole_life.calculated_maturity_estimate().is_none());
}

#[test]
fn reads_premium_tags() {
    let metadata = json!({"policy_premium": {"policy_id": "P1", "installment": 4}});
    let tag = PolicyPremiumTag::read(Some(&metadata)).unwrap();
    assert_eq!(tag.policy_id, "P1");
    assert_eq!(tag.installment, Some(4));
    assert!(PolicyPremiumTag::read(Some(&json!({"loan_payment": {}}))).is_none());
    assert!(is_insurance_policy(Some(&meta(json!({})))));
}
