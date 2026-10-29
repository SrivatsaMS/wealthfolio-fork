//! Payment method (UPI, cash, cards, bank transfer) for spending activities.
//!
//! The payment method is how money moved, not what it was spent on, so it is
//! kept apart from the category taxonomies: a restaurant bill is category
//! `Food` with payment method `upi`. It is stored on the activity itself as
//! `metadata.payment_method`, which the existing activity create/update paths
//! already accept (updates merge metadata keys), and which export, backup and
//! device sync already carry. Activities created before this feature simply
//! have no payment method and read as "unspecified"; no stored row changes.

use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wealthfolio_core::activities::Activity;

use crate::cash_activities::model::{CashActivity, CashFlowBucket};

/// Activity metadata key holding the payment method.
pub const PAYMENT_METHOD_KEY: &str = "payment_method";
/// Filter token matching activities without a payment method.
pub const UNSPECIFIED_PAYMENT_METHOD: &str = "unspecified";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Upi,
    Cash,
    DebitCard,
    CreditCard,
    BankTransfer,
    Other,
}

impl PaymentMethod {
    pub const ALL: [PaymentMethod; 6] = [
        Self::Upi,
        Self::Cash,
        Self::DebitCard,
        Self::CreditCard,
        Self::BankTransfer,
        Self::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Upi => "upi",
            Self::Cash => "cash",
            Self::DebitCard => "debit_card",
            Self::CreditCard => "credit_card",
            Self::BankTransfer => "bank_transfer",
            Self::Other => "other",
        }
    }

    /// Display label, also matched by free-text transaction search.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Upi => "UPI",
            Self::Cash => "Cash",
            Self::DebitCard => "Debit card",
            Self::CreditCard => "Credit card",
            Self::BankTransfer => "Bank transfer",
            Self::Other => "Other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        Self::ALL.into_iter().find(|m| m.as_str() == normalized)
    }

    /// The payment method stored on an activity, if any. Unknown values read
    /// as none rather than failing, so a newer client's value cannot break reads.
    pub fn of_metadata(metadata: Option<&Value>) -> Option<Self> {
        metadata?
            .get(PAYMENT_METHOD_KEY)?
            .as_str()
            .and_then(Self::parse)
    }

    pub fn of(activity: &Activity) -> Option<Self> {
        Self::of_metadata(activity.metadata.as_ref())
    }
}

/// Whether `activity` matches any of the filter tokens (`upi`, …, `unspecified`).
pub fn matches_payment_methods(activity: &Activity, tokens: &[String]) -> bool {
    let method = PaymentMethod::of(activity);
    tokens.iter().any(|token| match method {
        Some(m) => PaymentMethod::parse(token) == Some(m),
        None => token
            .trim()
            .eq_ignore_ascii_case(UNSPECIFIED_PAYMENT_METHOD),
    })
}

/// Request for totals by payment method and category over a period.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethodSummaryRequest {
    /// RFC3339 or YYYY-MM-DD, inclusive.
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub account_ids: Option<Vec<String>>,
    /// Restrict to these methods (tokens as in search). None = all.
    pub payment_methods: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethodCategoryTotal {
    /// Spending category id; None for uncategorized.
    pub category_id: Option<String>,
    pub total: f64,
    pub count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethodTotal {
    /// `upi`, `cash`, …, or `unspecified`.
    pub payment_method: String,
    /// Spending (outflows), positive, in the summary currency.
    pub spent: f64,
    pub spent_count: usize,
    /// Income and other inflows paid in by this method, positive.
    pub received: f64,
    pub received_count: usize,
    pub categories: Vec<PaymentMethodCategoryTotal>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethodSummary {
    /// Currency of every figure: the base currency when rows could be
    /// converted, otherwise the single currency they share.
    pub currency: Option<String>,
    /// Rows left out because they could not be converted to `currency`.
    pub unconverted_count: usize,
    pub methods: Vec<PaymentMethodTotal>,
}

/// Aggregates search rows into payment-method totals. Spending outflows are
/// counted as spent (refunds in the spending bucket reduce it); income inflows
/// are counted as received; neutral transfers are left out.
pub fn summarize_payment_methods(
    rows: &[CashActivity],
    base_currency: Option<&str>,
) -> PaymentMethodSummary {
    use std::collections::BTreeMap;

    // Rows that cannot be expressed in one currency are counted, not guessed.
    let single_currency = {
        let mut currencies = rows.iter().map(|r| r.activity.currency.as_str());
        let first = currencies.next();
        first.filter(|c| currencies.all(|other| other == *c))
    };
    let currency = base_currency.or(single_currency).map(str::to_string);

    let mut by_method: BTreeMap<String, PaymentMethodTotal> = BTreeMap::new();
    let mut categories: BTreeMap<(String, Option<String>), PaymentMethodCategoryTotal> =
        BTreeMap::new();
    let mut unconverted_count = 0;

    for row in rows {
        let amount = match (row.net_amount_base, currency.as_deref()) {
            (Some(converted), Some(c)) if Some(c) == base_currency => converted,
            (_, Some(c)) if row.activity.currency == c => row.net_amount,
            _ => {
                unconverted_count += 1;
                continue;
            }
        };
        let method = PaymentMethod::of(&row.activity)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| UNSPECIFIED_PAYMENT_METHOD.to_string());
        let total = by_method
            .entry(method.clone())
            .or_insert_with(|| PaymentMethodTotal {
                payment_method: method.clone(),
                ..Default::default()
            });
        match row.cash_flow_bucket {
            CashFlowBucket::Spending => {
                // Outflows are negative nets; a refund (positive) reduces spending.
                let spent = -amount;
                total.spent += spent;
                total.spent_count += 1;
                let spending_splits: Vec<_> = row
                    .splits
                    .iter()
                    .filter(|s| s.taxonomy_id == "spending_categories")
                    .collect();
                let split_total: f64 = spending_splits
                    .iter()
                    .filter_map(|s| s.amount.abs().to_f64())
                    .sum();
                if !spending_splits.is_empty() && split_total > 0.0 {
                    for split in spending_splits {
                        let share = split.amount.abs().to_f64().unwrap_or(0.0) / split_total;
                        let entry = categories
                            .entry((method.clone(), Some(split.category_id.clone())))
                            .or_insert_with(|| PaymentMethodCategoryTotal {
                                category_id: Some(split.category_id.clone()),
                                ..Default::default()
                            });
                        entry.total += spent * share;
                        entry.count += 1;
                    }
                } else {
                    let category_id = row
                        .assignments
                        .iter()
                        .find(|a| a.taxonomy_id == "spending_categories")
                        .map(|a| a.category_id.clone());
                    let entry = categories
                        .entry((method.clone(), category_id.clone()))
                        .or_insert_with(|| PaymentMethodCategoryTotal {
                            category_id,
                            ..Default::default()
                        });
                    entry.total += spent;
                    entry.count += 1;
                }
            }
            CashFlowBucket::Income if amount > 0.0 => {
                total.received += amount;
                total.received_count += 1;
            }
            _ => {}
        }
    }

    let round = |v: f64| (v * 100.0).round() / 100.0;
    let mut methods: Vec<PaymentMethodTotal> = by_method
        .into_values()
        .map(|mut total| {
            total.spent = round(total.spent);
            total.received = round(total.received);
            let mut cats: Vec<PaymentMethodCategoryTotal> = categories
                .iter()
                .filter(|((m, _), _)| *m == total.payment_method)
                .map(|(_, c)| PaymentMethodCategoryTotal {
                    total: round(c.total),
                    ..c.clone()
                })
                .collect();
            cats.sort_by(|a, b| b.total.total_cmp(&a.total));
            total.categories = cats;
            total
        })
        .collect();
    methods.sort_by(|a, b| b.spent.total_cmp(&a.spent));
    PaymentMethodSummary {
        currency,
        unconverted_count,
        methods,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use serde_json::json;
    use wealthfolio_core::activities::ActivityStatus;

    fn activity(metadata: Option<Value>) -> Activity {
        Activity {
            id: "activity-1".to_string(),
            account_id: "account-1".to_string(),
            asset_id: None,
            activity_type: "WITHDRAWAL".to_string(),
            activity_type_override: None,
            source_type: None,
            subtype: None,
            status: ActivityStatus::Posted,
            activity_date: Utc::now(),
            settlement_date: None,
            quantity: None,
            unit_price: None,
            amount: Some(Decimal::new(100, 0)),
            fee: None,
            tax: None,
            currency: "INR".to_string(),
            fx_rate: None,
            notes: None,
            metadata,
            source_system: None,
            source_record_id: None,
            source_group_id: None,
            idempotency_key: None,
            import_run_id: None,
            is_user_modified: false,
            needs_review: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn reads_payment_method_from_metadata() {
        assert_eq!(
            PaymentMethod::of(&activity(Some(json!({"payment_method": "upi"})))),
            Some(PaymentMethod::Upi)
        );
        assert_eq!(
            PaymentMethod::of(&activity(Some(json!({"payment_method": "Debit Card"})))),
            Some(PaymentMethod::DebitCard)
        );
        // Existing activities without the field, and unknown values, read as none.
        assert_eq!(PaymentMethod::of(&activity(None)), None);
        assert_eq!(
            PaymentMethod::of(&activity(Some(json!({"flow": {"is_external": true}})))),
            None
        );
        assert_eq!(
            PaymentMethod::of(&activity(Some(json!({"payment_method": "cheque"})))),
            None
        );
        // A cleared value (null after a metadata merge) reads as none.
        assert_eq!(
            PaymentMethod::of(&activity(Some(json!({"payment_method": null})))),
            None
        );
    }

    #[test]
    fn filter_tokens_include_unspecified() {
        let upi = activity(Some(json!({"payment_method": "upi"})));
        let none = activity(None);
        let upi_only = vec!["upi".to_string()];
        assert!(matches_payment_methods(&upi, &upi_only));
        assert!(!matches_payment_methods(&none, &upi_only));
        let unspecified = vec!["unspecified".to_string()];
        assert!(matches_payment_methods(&none, &unspecified));
        assert!(!matches_payment_methods(&upi, &unspecified));
    }

    #[test]
    fn every_method_round_trips() {
        for method in PaymentMethod::ALL {
            assert_eq!(PaymentMethod::parse(method.as_str()), Some(method));
            let json = serde_json::to_value(method).unwrap();
            assert_eq!(json, json!(method.as_str()));
        }
    }

    fn row(
        id: &str,
        method: Option<&str>,
        bucket: CashFlowBucket,
        net: f64,
        category: Option<&str>,
    ) -> CashActivity {
        let mut a = activity(method.map(|m| json!({ "payment_method": m })));
        a.id = id.to_string();
        CashActivity {
            activity: a,
            cash_flow_bucket: bucket,
            assignments: category
                .map(|c| {
                    vec![crate::activity_assignments::ActivityTaxonomyAssignment {
                        id: format!("asg-{id}"),
                        activity_id: id.to_string(),
                        taxonomy_id: "spending_categories".to_string(),
                        category_id: c.to_string(),
                        weight: 10000,
                        source: "manual".to_string(),
                        created_at: Utc::now().naive_utc(),
                        updated_at: Utc::now().naive_utc(),
                    }]
                })
                .unwrap_or_default(),
            splits: Vec::new(),
            event_id: None,
            transfer_link_status: None,
            net_amount: net,
            net_amount_base: Some(net),
            visible_spending_amount: 0.0,
            cash_balance: None,
            cash_balance_currency: None,
        }
    }

    #[test]
    fn summarizes_upi_spending_by_category() {
        let rows = vec![
            row(
                "1",
                Some("upi"),
                CashFlowBucket::Spending,
                -250.0,
                Some("food"),
            ),
            row(
                "2",
                Some("upi"),
                CashFlowBucket::Spending,
                -100.0,
                Some("food"),
            ),
            row(
                "3",
                Some("upi"),
                CashFlowBucket::Spending,
                -40.5,
                Some("transport"),
            ),
            // Refund paid back over UPI reduces spending.
            row(
                "4",
                Some("upi"),
                CashFlowBucket::Spending,
                50.0,
                Some("food"),
            ),
            row("5", Some("cash"), CashFlowBucket::Spending, -30.0, None),
            row("6", None, CashFlowBucket::Spending, -10.0, Some("food")),
            row("7", Some("upi"), CashFlowBucket::Income, 1000.0, None),
            row("8", Some("upi"), CashFlowBucket::Neutral, -500.0, None),
        ];
        let summary = summarize_payment_methods(&rows, Some("INR"));
        assert_eq!(summary.currency.as_deref(), Some("INR"));
        let upi = summary
            .methods
            .iter()
            .find(|m| m.payment_method == "upi")
            .unwrap();
        assert_eq!(upi.spent, 340.5);
        assert_eq!(upi.spent_count, 4);
        assert_eq!(upi.received, 1000.0);
        assert_eq!(upi.categories[0].category_id.as_deref(), Some("food"));
        assert_eq!(upi.categories[0].total, 300.0);
        assert_eq!(upi.categories[1].total, 40.5);
        let unspecified = summary
            .methods
            .iter()
            .find(|m| m.payment_method == "unspecified")
            .unwrap();
        assert_eq!(unspecified.spent, 10.0);
        // Highest spending first.
        assert_eq!(summary.methods[0].payment_method, "upi");
    }

    #[test]
    fn unconvertible_rows_are_counted_not_guessed() {
        let mut usd = row("1", Some("upi"), CashFlowBucket::Spending, -10.0, None);
        usd.activity.currency = "USD".into();
        usd.net_amount_base = None;
        let inr = row("2", Some("upi"), CashFlowBucket::Spending, -20.0, None);
        let summary = summarize_payment_methods(&[usd, inr], Some("INR"));
        assert_eq!(summary.unconverted_count, 1);
        assert_eq!(summary.methods[0].spent, 20.0);
    }
}
