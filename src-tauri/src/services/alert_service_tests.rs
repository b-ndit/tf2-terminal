use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::alerts_repo::CreateAlertRule;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::UpsertMarketListing;
use crate::infra::db::repos::price_history_repo::{
    InsertPricePoint, PriceDailyRepo, PricePointsRepo,
};

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-alert-service-test-{}-{}",
        std::process::id(),
        uniq_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    (pool, dir)
}

fn uniq_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn plain_key(defindex: u32) -> ItemKey {
    ItemKey {
        defindex,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    }
}

fn rule_row(
    id: i32,
    item_id: i64,
    kind: &str,
    threshold: Option<f64>,
    channels_json: &str,
) -> AlertRuleRow {
    AlertRuleRow {
        id,
        item_id,
        kind: kind.to_string(),
        threshold,
        channels: channels_json.to_string(),
        enabled: true,
    }
}

fn listing_event(kind: ListingEventKind, intent: &str, price_ref: Option<f64>) -> ListingEvent {
    ListingEvent {
        listing_id: "l1".to_string(),
        kind,
        defindex: 5021,
        quality: Quality::Unique as u8,
        effect_id: None,
        killstreak_tier: 0,
        australium: false,
        festivized: false,
        craftable: true,
        intent: intent.to_string(),
        steam_id: "sid0".to_string(),
        steam_name: Some("Trader".to_string()),
        value_ref: price_ref,
    }
}

// ---- check_rule (pure) ----

#[test]
fn check_rule_fires_and_builds_a_fired_alert() {
    let rule = rule_row(1, 10, "price_drop", Some(50.0), r#"["desktop","sound"]"#);
    let signal = AlertSignal::Listing {
        is_new: false,
        intent: Intent::Sell,
        price_ref: 40.0,
        current_spread_pct: None,
    };
    let fired = check_rule(&rule, "Mann Co. Supply Crate Key", &signal).unwrap();
    assert_eq!(fired.rule_id, 1);
    assert_eq!(fired.kind, AlertKind::PriceDrop);
    assert_eq!(
        fired.channels,
        vec!["desktop".to_string(), "sound".to_string()]
    );
    assert!(fired.message.contains("40.00"));
}

#[test]
fn check_rule_returns_none_when_rule_does_not_fire() {
    let rule = rule_row(1, 10, "price_drop", Some(30.0), "[]");
    let signal = AlertSignal::Listing {
        is_new: false,
        intent: Intent::Sell,
        price_ref: 40.0,
        current_spread_pct: None,
    };
    assert!(check_rule(&rule, "Key", &signal).is_none());
}

#[test]
fn check_rule_returns_none_for_unparseable_kind() {
    let rule = rule_row(1, 10, "not_a_real_kind", Some(1.0), "[]");
    let signal = AlertSignal::Listing {
        is_new: true,
        intent: Intent::Buy,
        price_ref: 1.0,
        current_spread_pct: None,
    };
    assert!(check_rule(&rule, "Key", &signal).is_none());
}

#[test]
fn check_rule_defaults_to_empty_channels_on_malformed_json() {
    let rule = rule_row(1, 10, "new_buyer", None, "not json");
    let signal = AlertSignal::Listing {
        is_new: true,
        intent: Intent::Buy,
        price_ref: 1.0,
        current_spread_pct: None,
    };
    let fired = check_rule(&rule, "Key", &signal).unwrap();
    assert!(fired.channels.is_empty());
}

// ---- find_fired_alerts_for_event (DB-backed) ----

#[tokio::test]
async fn find_fired_alerts_for_event_fires_a_matching_price_drop_rule() {
    let (pool, dir) = test_pool().await;
    let item_id = ItemsRepo::get_or_create(&pool, &plain_key(5021), "Mann Co. Supply Crate Key")
        .await
        .unwrap();
    AlertRulesRepo::create(
        &pool,
        &CreateAlertRule {
            item_id,
            kind: "price_drop",
            threshold: Some(50.0),
            channels_json: r#"["desktop"]"#,
        },
    )
    .await
    .unwrap();

    let event = listing_event(ListingEventKind::New, "sell", Some(45.0));
    let fired = find_fired_alerts_for_event(&pool, &event).await.unwrap();

    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].kind, AlertKind::PriceDrop);
    assert_eq!(fired[0].item_name, "Mann Co. Supply Crate Key");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_fired_alerts_for_event_ignores_removed_events() {
    let (pool, dir) = test_pool().await;
    let item_id = ItemsRepo::get_or_create(&pool, &plain_key(5021), "Key")
        .await
        .unwrap();
    AlertRulesRepo::create(
        &pool,
        &CreateAlertRule {
            item_id,
            kind: "new_seller",
            threshold: None,
            channels_json: "[]",
        },
    )
    .await
    .unwrap();

    let event = listing_event(ListingEventKind::Removed, "sell", Some(45.0));
    let fired = find_fired_alerts_for_event(&pool, &event).await.unwrap();
    assert!(fired.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_fired_alerts_for_event_is_empty_for_unknown_item() {
    let (pool, dir) = test_pool().await;
    let event = listing_event(ListingEventKind::New, "sell", Some(45.0));
    let fired = find_fired_alerts_for_event(&pool, &event).await.unwrap();
    assert!(fired.is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_fired_alerts_for_event_computes_spread_only_when_a_rule_needs_it() {
    let (pool, dir) = test_pool().await;
    let key = plain_key(5021);
    let item_id = ItemsRepo::get_or_create(&pool, &key, "Key").await.unwrap();
    AlertRulesRepo::create(
        &pool,
        &CreateAlertRule {
            item_id,
            kind: "spread_widen",
            threshold: Some(10.0),
            channels_json: "[]",
        },
    )
    .await
    .unwrap();

    // A buy at 50 and this incoming sell at 60 -> spread 16.7%, over the
    // 10% threshold.
    MarketListingsRepo::upsert(
        &pool,
        &UpsertMarketListing {
            listing_id: "buy1",
            defindex: key.defindex as i64,
            quality: key.quality as u8 as i64,
            effect_id: None,
            killstreak_tier: 0,
            australium: false,
            festivized: false,
            craftable: true,
            intent: "buy",
            price_ref: 50.0,
            steam_id: "buyer",
            steam_name: None,
            updated_at: 1000,
        },
    )
    .await
    .unwrap();
    MarketListingsRepo::upsert(
        &pool,
        &UpsertMarketListing {
            listing_id: "l1",
            defindex: key.defindex as i64,
            quality: key.quality as u8 as i64,
            effect_id: None,
            killstreak_tier: 0,
            australium: false,
            festivized: false,
            craftable: true,
            intent: "sell",
            price_ref: 60.0,
            steam_id: "sid0",
            steam_name: None,
            updated_at: 1000,
        },
    )
    .await
    .unwrap();

    let event = listing_event(ListingEventKind::New, "sell", Some(60.0));
    let fired = find_fired_alerts_for_event(&pool, &event).await.unwrap();

    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].kind, AlertKind::SpreadWiden);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

// ---- find_fired_hist_alerts (DB-backed) ----

async fn seed_daily_bar(pool: &SqlitePool, item_id: i64, day: i64, close_ref: f64) {
    let ts = day * DAY_SECONDS;
    PricePointsRepo::insert(
        pool,
        &InsertPricePoint {
            item_id,
            ts,
            source: "snapshot",
            best_buy_keys: None,
            best_buy_ref: Some(close_ref),
            best_sell_keys: None,
            best_sell_ref: Some(close_ref),
            buy_count: Some(1),
            sell_count: Some(1),
            key_rate_ref: 60.0,
        },
    )
    .await
    .unwrap();
    PriceDailyRepo::recompute_day(pool, item_id, day)
        .await
        .unwrap();
}

#[tokio::test]
async fn find_fired_hist_alerts_fires_when_today_undercuts_every_prior_day() {
    let (pool, dir) = test_pool().await;
    let item_id = ItemsRepo::get_or_create(&pool, &plain_key(5021), "Key")
        .await
        .unwrap();
    AlertRulesRepo::create(
        &pool,
        &CreateAlertRule {
            item_id,
            kind: "hist_low",
            threshold: None,
            channels_json: "[]",
        },
    )
    .await
    .unwrap();

    let today = 19_100_i64;
    seed_daily_bar(&pool, item_id, today - 2, 50.0).await;
    seed_daily_bar(&pool, item_id, today - 1, 45.0).await;
    seed_daily_bar(&pool, item_id, today, 40.0).await;

    let fired = find_fired_hist_alerts(&pool, today * DAY_SECONDS)
        .await
        .unwrap();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].kind, AlertKind::HistLow);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_fired_hist_alerts_skips_rule_without_a_bar_for_today() {
    let (pool, dir) = test_pool().await;
    let item_id = ItemsRepo::get_or_create(&pool, &plain_key(5021), "Key")
        .await
        .unwrap();
    AlertRulesRepo::create(
        &pool,
        &CreateAlertRule {
            item_id,
            kind: "hist_low",
            threshold: None,
            channels_json: "[]",
        },
    )
    .await
    .unwrap();

    let today = 19_100_i64;
    seed_daily_bar(&pool, item_id, today - 1, 45.0).await;
    // No bar for `today` yet.

    let fired = find_fired_hist_alerts(&pool, today * DAY_SECONDS)
        .await
        .unwrap();
    assert!(fired.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_fired_hist_alerts_does_not_fire_when_not_a_new_extreme() {
    let (pool, dir) = test_pool().await;
    let item_id = ItemsRepo::get_or_create(&pool, &plain_key(5021), "Key")
        .await
        .unwrap();
    AlertRulesRepo::create(
        &pool,
        &CreateAlertRule {
            item_id,
            kind: "hist_low",
            threshold: None,
            channels_json: "[]",
        },
    )
    .await
    .unwrap();

    let today = 19_100_i64;
    seed_daily_bar(&pool, item_id, today - 1, 45.0).await;
    seed_daily_bar(&pool, item_id, today, 46.0).await;

    let fired = find_fired_hist_alerts(&pool, today * DAY_SECONDS)
        .await
        .unwrap();
    assert!(fired.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_fired_hist_alerts_is_empty_when_no_hist_rules_exist() {
    let (pool, dir) = test_pool().await;
    let fired = find_fired_hist_alerts(&pool, 19_100 * DAY_SECONDS)
        .await
        .unwrap();
    assert!(fired.is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
