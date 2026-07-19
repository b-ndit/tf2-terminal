use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-partner-inventory-service-test-{}-{}",
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

fn tf2_item_from_json(json: &str) -> TF2Item {
    serde_json::from_str(json).unwrap()
}

#[tokio::test]
async fn resolve_views_uses_catalog_name_and_image_when_known() {
    let (pool, dir) = test_pool().await;
    let key = ItemKey {
        defindex: 5021,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    let id = ItemsRepo::get_or_create(&pool, &key, "Mann Co. Supply Crate Key")
        .await
        .unwrap();
    ItemsRepo::set_image_url(&pool, id, "https://example.com/key.png")
        .await
        .unwrap();

    let items = vec![tf2_item_from_json(
        r#"{"id": 1, "defindex": 5021, "quality": 6}"#,
    )];
    let views = resolve_views(&pool, items).await.unwrap();

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].asset_id, "1");
    assert_eq!(views[0].name, "Mann Co. Supply Crate Key");
    assert_eq!(
        views[0].image_url,
        Some("https://example.com/key.png".to_string())
    );
    assert_eq!(views[0].quality, 6);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn resolve_views_falls_back_to_unknown_item_name_when_uncatalogued() {
    let (pool, dir) = test_pool().await;
    let items = vec![tf2_item_from_json(
        r#"{"id": 2, "defindex": 999999, "quality": 6}"#,
    )];

    let views = resolve_views(&pool, items).await.unwrap();

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "Unknown Item 999999");
    assert_eq!(views[0].image_url, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn resolve_views_skips_items_with_unparseable_keys() {
    let (pool, dir) = test_pool().await;
    let items = vec![tf2_item_from_json(
        r#"{"id": 3, "defindex": 1, "quality": 250}"#,
    )];

    let views = resolve_views(&pool, items).await.unwrap();
    assert!(views.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
