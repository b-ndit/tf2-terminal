use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::items_repo::ItemsRepo;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-alerts-repo-test-{}-{}",
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

async fn seed_item(pool: &SqlitePool, defindex: u32, name: &str) -> i64 {
    let key = ItemKey {
        defindex,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    ItemsRepo::get_or_create(pool, &key, name).await.unwrap()
}

fn rule<'a>(
    item_id: i64,
    kind: &'a str,
    threshold: Option<f64>,
    channels_json: &'a str,
) -> CreateAlertRule<'a> {
    CreateAlertRule {
        item_id,
        kind,
        threshold,
        channels_json,
    }
}

#[tokio::test]
async fn create_and_list_returns_rule_joined_with_item_name() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Mann Co. Supply Crate Key").await;

    let rule_id = AlertRulesRepo::create(
        &pool,
        &rule(item_id, "price_drop", Some(50.0), r#"["desktop"]"#),
    )
    .await
    .unwrap();

    let rules = AlertRulesRepo::list(&pool).await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, rule_id);
    assert_eq!(rules[0].item_name, "Mann Co. Supply Crate Key");
    assert_eq!(rules[0].kind, "price_drop");
    assert_eq!(rules[0].threshold, Some(50.0));
    assert_eq!(rules[0].channels, r#"["desktop"]"#);
    assert!(rules[0].enabled);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn create_allows_null_threshold_for_thresholdless_kinds() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Key").await;

    AlertRulesRepo::create(&pool, &rule(item_id, "new_buyer", None, "[]"))
        .await
        .unwrap();

    let rules = AlertRulesRepo::list(&pool).await.unwrap();
    assert_eq!(rules[0].threshold, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_enabled_for_item_excludes_other_items_and_disabled_rules() {
    let (pool, dir) = test_pool().await;
    let key_item = seed_item(&pool, 5021, "Key").await;
    let other_item = seed_item(&pool, 5022, "Other").await;

    let target_rule =
        AlertRulesRepo::create(&pool, &rule(key_item, "price_drop", Some(50.0), "[]"))
            .await
            .unwrap();
    AlertRulesRepo::create(&pool, &rule(other_item, "price_drop", Some(10.0), "[]"))
        .await
        .unwrap();
    let disabled_rule =
        AlertRulesRepo::create(&pool, &rule(key_item, "spread_widen", Some(20.0), "[]"))
            .await
            .unwrap();
    AlertRulesRepo::set_enabled(&pool, disabled_rule, false)
        .await
        .unwrap();

    let rules = AlertRulesRepo::list_enabled_for_item(&pool, key_item)
        .await
        .unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, target_rule);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_enabled_by_kind_filters_across_all_items() {
    let (pool, dir) = test_pool().await;
    let item_a = seed_item(&pool, 5021, "Key").await;
    let item_b = seed_item(&pool, 5022, "Other").await;

    AlertRulesRepo::create(&pool, &rule(item_a, "hist_low", None, "[]"))
        .await
        .unwrap();
    AlertRulesRepo::create(&pool, &rule(item_b, "hist_low", None, "[]"))
        .await
        .unwrap();
    AlertRulesRepo::create(&pool, &rule(item_a, "hist_high", None, "[]"))
        .await
        .unwrap();

    let rules = AlertRulesRepo::list_enabled_by_kind(&pool, "hist_low")
        .await
        .unwrap();
    assert_eq!(rules.len(), 2);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn set_enabled_toggles_visibility_in_list_enabled_queries() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Key").await;
    let rule_id = AlertRulesRepo::create(&pool, &rule(item_id, "price_drop", Some(1.0), "[]"))
        .await
        .unwrap();

    AlertRulesRepo::set_enabled(&pool, rule_id, false)
        .await
        .unwrap();
    assert!(AlertRulesRepo::list_enabled_for_item(&pool, item_id)
        .await
        .unwrap()
        .is_empty());

    AlertRulesRepo::set_enabled(&pool, rule_id, true)
        .await
        .unwrap();
    assert_eq!(
        AlertRulesRepo::list_enabled_for_item(&pool, item_id)
            .await
            .unwrap()
            .len(),
        1
    );

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn delete_removes_the_rule() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Key").await;
    let rule_id = AlertRulesRepo::create(&pool, &rule(item_id, "price_drop", Some(1.0), "[]"))
        .await
        .unwrap();

    AlertRulesRepo::delete(&pool, rule_id).await.unwrap();

    assert!(AlertRulesRepo::list(&pool).await.unwrap().is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn insert_and_list_recent_alert_events() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Key").await;
    let rule_id = AlertRulesRepo::create(
        &pool,
        &rule(item_id, "price_drop", Some(50.0), r#"["desktop"]"#),
    )
    .await
    .unwrap();

    let event_id = AlertEventsRepo::insert(
        &pool,
        &InsertAlertEvent {
            rule_id,
            fired_ts: 1000,
            payload_json: r#"{"message":"Key dropped to 45 ref"}"#,
        },
    )
    .await
    .unwrap();

    let events = AlertEventsRepo::list_recent(&pool, 10).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, event_id);
    assert_eq!(events[0].rule_id, rule_id);
    assert!(!events[0].acked);
    assert!(events[0].payload.contains("45 ref"));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_recent_orders_newest_first_and_respects_limit() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Key").await;
    let rule_id = AlertRulesRepo::create(&pool, &rule(item_id, "price_drop", Some(50.0), "[]"))
        .await
        .unwrap();

    for ts in [1000, 2000, 3000] {
        AlertEventsRepo::insert(
            &pool,
            &InsertAlertEvent {
                rule_id,
                fired_ts: ts,
                payload_json: "{}",
            },
        )
        .await
        .unwrap();
    }

    let events = AlertEventsRepo::list_recent(&pool, 2).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].fired_ts, 3000);
    assert_eq!(events[1].fired_ts, 2000);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn ack_marks_the_event_acknowledged() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Key").await;
    let rule_id = AlertRulesRepo::create(&pool, &rule(item_id, "price_drop", Some(50.0), "[]"))
        .await
        .unwrap();
    let event_id = AlertEventsRepo::insert(
        &pool,
        &InsertAlertEvent {
            rule_id,
            fired_ts: 1000,
            payload_json: "{}",
        },
    )
    .await
    .unwrap();

    AlertEventsRepo::ack(&pool, event_id).await.unwrap();

    let events = AlertEventsRepo::list_recent(&pool, 10).await.unwrap();
    assert!(events[0].acked);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
