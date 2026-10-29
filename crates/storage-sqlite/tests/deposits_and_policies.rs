#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! Fixed deposits, insurance policies and concurrent-edit protection, through
//! the real alternative-asset service and a migrated SQLite database.
mod common;

use std::sync::Arc;

use chrono::NaiveDate;
use diesel::RunQueryDsl;
use rust_decimal::Decimal;
use serde_json::json;
use wealthfolio_core::assets::insurance_policy::PolicyPremiumTag;
use wealthfolio_core::assets::{
    AlternativeAssetService, AlternativeAssetServiceTrait, AssetKind, AssetRepositoryTrait,
    CreateAlternativeAssetRequest, UpdateAssetDetailsRequest,
};
use wealthfolio_core::quotes::{QuoteService, QuoteServiceTrait};
use wealthfolio_core::secrets::SecretStore;
use wealthfolio_core::{Error, Result};
use wealthfolio_storage_sqlite::{
    activities::ActivityRepository,
    assets::{AlternativeAssetRepository, AssetRepository},
    db,
    market_data::{MarketDataRepository, QuoteSyncStateRepository},
};

struct NoSecrets;

impl SecretStore for NoSecrets {
    fn get_secret(&self, _: &str) -> Result<Option<String>> {
        panic!("must not access credentials")
    }
    fn set_secret(&self, _: &str, _: &str) -> Result<()> {
        panic!("must not access credentials")
    }
    fn delete_secret(&self, _: &str) -> Result<()> {
        panic!("must not access credentials")
    }
}

struct Fixture {
    pool: Arc<db::DbPool>,
    service: AlternativeAssetService,
    assets: Arc<AssetRepository>,
    quotes: Arc<dyn QuoteServiceTrait>,
    _dir: common::TestDir,
}

impl Fixture {
    fn execute(&self, statement: &str) -> std::result::Result<usize, diesel::result::Error> {
        diesel::sql_query(statement).execute(&mut db::get_connection(&self.pool).unwrap())
    }

    fn add_account(&self, id: &str) {
        self.execute(&format!(
            "INSERT INTO accounts (id, name, account_type, currency, is_default, is_active, \
             created_at, updated_at, is_archived, tracking_mode) VALUES ('{id}', '{id}', \
             'CASH', 'INR', 0, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 0, 'NOT_SET')"
        ))
        .unwrap();
    }

    /// A premium withdrawal as the frontend records it.
    fn add_premium(
        &self,
        id: &str,
        policy: &str,
        installment: u32,
        day: &str,
    ) -> std::result::Result<usize, diesel::result::Error> {
        let metadata = json!({
            "policy_premium": { "policy_id": policy, "installment": installment }
        });
        let key = PolicyPremiumTag::idempotency_key(policy, installment);
        self.execute(&format!(
            "INSERT INTO activities (id, account_id, activity_type, status, activity_date, \
             amount, currency, metadata, idempotency_key, is_user_modified, needs_review, \
             created_at, updated_at) VALUES ('{id}', 'savings', 'WITHDRAWAL', 'POSTED', \
             '{day}T06:00:00Z', '10000', 'INR', '{metadata}', '{key}', 0, 0, \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
    }

    async fn create(&self, name: &str, metadata: serde_json::Value, date: &str) -> Result<String> {
        let date: NaiveDate = date.parse().unwrap();
        self.service
            .create_alternative_asset(CreateAlternativeAssetRequest {
                kind: AssetKind::Other,
                name: name.into(),
                currency: "INR".into(),
                current_value: Decimal::new(100000, 0),
                value_date: date,
                purchase_price: None,
                purchase_date: None,
                metadata: Some(metadata),
                linked_asset_id: None,
                loan: None,
            })
            .await
            .map(|r| r.asset_id)
    }
}

async fn fixture() -> Fixture {
    let dir = common::TestDir::new();
    let access = db::DbAccess::plaintext(dir.path().join("app.db").to_str().unwrap());
    access.prepare().unwrap();
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    diesel::sql_query("UPDATE market_data_providers SET enabled=0")
        .execute(&mut db::get_connection(&pool).unwrap())
        .unwrap();
    let (writer, _task) = db::write_actor::spawn_writer_with_sync_state(
        (*pool).clone(),
        Arc::new(|| {}),
        Arc::default(),
    )
    .unwrap();
    let assets = Arc::new(AssetRepository::new(pool.clone(), writer.clone()));
    let market_data = Arc::new(MarketDataRepository::new(pool.clone(), writer.clone()));
    let quotes: Arc<dyn QuoteServiceTrait> = Arc::new(
        QuoteService::new(
            market_data.clone(),
            Arc::new(QuoteSyncStateRepository::new(pool.clone(), writer.clone())),
            market_data,
            assets.clone(),
            Arc::new(ActivityRepository::new(pool.clone(), writer.clone())),
            Arc::new(NoSecrets),
        )
        .await
        .unwrap(),
    );
    let repository = Arc::new(AlternativeAssetRepository::new(pool.clone(), writer));
    let service = AlternativeAssetService::new(repository, assets.clone(), quotes.clone());
    Fixture {
        pool,
        service,
        assets,
        quotes,
        _dir: dir,
    }
}

fn fixed_deposit() -> serde_json::Value {
    json!({
        "sub_type": "fixed_deposit",
        "fd_issuer": "HDFC Bank",
        "fd_principal": "100000",
        "fd_interest_rate": "7",
        "fd_interest_method": "compound",
        "fd_compounding": "quarterly",
        "fd_start_date": "2023-01-01",
        "fd_maturity_date": "2024-01-01",
        "purchase_price": "100000",
        "purchase_date": "2023-01-01",
    })
}

fn policy() -> serde_json::Value {
    json!({
        "sub_type": "insurance_policy",
        "policy_insurer": "LIC",
        "policy_number": "987654321",
        "policy_type": "endowment",
        "policy_premium_amount": "10000",
        "policy_premium_frequency": "yearly",
        "policy_commencement_date": "2020-01-01",
        "policy_term_years": "20",
        "policy_premium_term_years": "20",
        "policy_premiums_paid_before": "3",
        "policy_expected_maturity": "350000",
    })
}

#[tokio::test]
async fn fixed_deposit_is_created_valued_and_summarized() {
    let fx = fixture().await;
    let id = fx
        .create("HDFC FD", fixed_deposit(), "2023-01-01")
        .await
        .unwrap();
    let holding = fx
        .service
        .get_alternative_holdings()
        .unwrap()
        .into_iter()
        .find(|h| h.id == id)
        .unwrap();
    // Matured long ago: worth the calculated maturity amount, not principal + maturity.
    assert_eq!(holding.market_value, Decimal::new(10718590, 2));
    let summary = holding.fixed_deposit.unwrap();
    assert_eq!(summary.expected_maturity_amount, 107185.90);
    assert_eq!(summary.expected_interest, 7185.90);
    assert_eq!(summary.issuer.as_deref(), Some("HDFC Bank"));
    assert!(holding.policy.is_none());
}

#[tokio::test]
async fn invalid_fixed_deposit_is_refused_and_nothing_is_saved() {
    let fx = fixture().await;
    let mut bad = fixed_deposit();
    bad["fd_maturity_date"] = json!("2022-12-01");
    let err = fx.create("Bad FD", bad, "2023-01-01").await.unwrap_err();
    assert!(err.to_string().contains("after the start date"), "{err}");
    assert!(fx
        .assets
        .list()
        .unwrap()
        .iter()
        .all(|a| a.name.as_deref() != Some("Bad FD")));
}

#[tokio::test]
async fn editing_terms_is_validated_and_stale_edits_are_refused() {
    let fx = fixture().await;
    let id = fx
        .create("FD", fixed_deposit(), "2023-01-01")
        .await
        .unwrap();
    let loaded = fx.assets.get_by_id(&id).unwrap().updated_at;
    let edit = |key: &str, value: &str, expected| UpdateAssetDetailsRequest {
        asset_id: id.clone(),
        name: None,
        notes: None,
        metadata: Some([(key.to_string(), Some(value.to_string()))].into()),
        loan: None,
        expected_updated_at: expected,
    };
    // Invalid term change is refused.
    let err = fx
        .service
        .update_asset_details(edit("fd_interest_rate", "-3", Some(loaded)))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("between 0%"), "{err}");
    // Device A saves with the token it loaded.
    fx.service
        .update_asset_details(edit("fd_interest_rate", "7.5", Some(loaded)))
        .await
        .unwrap();
    // Device B, which loaded the same version, cannot overwrite it.
    let err = fx
        .service
        .update_asset_details(edit("fd_interest_rate", "6", Some(loaded)))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::ConstraintViolation(_)), "{err}");
    let stored = fx.assets.get_by_id(&id).unwrap();
    assert_eq!(stored.metadata.unwrap()["fd_interest_rate"], "7.5");
    assert_ne!(stored.updated_at, loaded);
    // After reloading, device B's edit goes through.
    fx.service
        .update_asset_details(edit("fd_interest_rate", "6", Some(stored.updated_at)))
        .await
        .unwrap();
}

#[tokio::test]
async fn policy_premiums_are_counted_once_per_instalment() {
    let fx = fixture().await;
    fx.add_account("savings");
    let id = fx
        .create("Jeevan Anand", policy(), "2020-01-01")
        .await
        .unwrap();
    fx.add_premium("p4", &id, 4, "2023-01-02").unwrap();
    fx.add_premium("p5", &id, 5, "2024-01-03").unwrap();
    // A retry from another device reuses the instalment's idempotency key.
    let duplicate = fx.add_premium("p5-retry", &id, 5, "2024-01-03");
    assert!(duplicate.is_err(), "duplicate premium must be refused");

    let holding = fx
        .service
        .get_alternative_holdings()
        .unwrap()
        .into_iter()
        .find(|h| h.id == id)
        .unwrap();
    let summary = holding.policy.unwrap();
    assert_eq!(summary.premiums_paid, 5);
    assert_eq!(summary.total_premiums_paid, 50000.0);
    assert_eq!(summary.payments.len(), 2);
    assert_eq!(summary.total_scheduled_premiums, 20);
    // Valued at premiums paid, not at the documented maturity benefit.
    assert_eq!(holding.market_value, Decimal::new(50000, 0));
    assert_eq!(
        summary.user_maturity_estimate.map(|e| e.amount),
        Some(350000.0)
    );
    // The creation quote is not used while the policy is valued from premiums.
    assert!(!fx.quotes.get_historical_quotes(&id).unwrap().is_empty());
}
