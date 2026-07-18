use super::*;
use crate::infra::db::repos::item_meta_repo::ItemMeta;
use crate::infra::db::repos::tags_repo::Tag;

fn sample_backpack_item() -> BackpackItem {
    BackpackItem {
        asset_id: "1".to_string(),
        item_id: 42,
        name: "Team Captain".to_string(),
        quality: 5, // Unusual
        effect_id: Some(701),
        killstreak_tier: 0,
        australium: false,
        festivized: false,
        craftable: true,
        craft_number: None,
        paint_id: None,
        strange_count: None,
        tradable: true,
        marketable: Some(true),
        acquired_ts: None,
        last_seen_ts: 1_700_000_000.0,
        meta: ItemMeta {
            folder: Some("Trade-up".to_string()),
            pinned: true,
            favorite: false,
            note: None,
            custom_label: None,
        },
        tags: vec![Tag {
            id: 1,
            name: "keep".to_string(),
            color: "#4d7455".to_string(),
        }],
    }
}

#[test]
fn backpack_table_maps_quality_id_to_display_name_and_flattens_tags() {
    let table = backpack_table(&[sample_backpack_item()]);
    assert_eq!(table.headers[0], "Name");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0][0], "Team Captain");
    assert_eq!(table.rows[0][1], "Unusual");
    let tags_col = table.headers.iter().position(|h| h == "Tags").unwrap();
    assert_eq!(table.rows[0][tags_col], "keep");
}

#[test]
fn backpack_table_falls_back_to_numeric_quality_for_unknown_id() {
    let mut item = sample_backpack_item();
    item.quality = 99;
    let table = backpack_table(&[item]);
    assert_eq!(table.rows[0][1], "99");
}

#[test]
fn trade_history_table_joins_given_and_received_with_values() {
    let trade = TradeLedgerView {
        trade_offer_id: "123".to_string(),
        partner_steam_id: "76561198000000001".to_string(),
        completed_ts: 1_700_000_000.0,
        given: vec![LedgerItemView {
            name: "Refined Metal".to_string(),
            value_ref: Some(1.0),
        }],
        received: vec![LedgerItemView {
            name: "Unresolved (not tracked while active)".to_string(),
            value_ref: None,
        }],
        net_value_ref: 5.5,
        rating: Some(4),
        notes: Some("good trade".to_string()),
    };

    let table = trade_history_table(&[trade]);
    assert_eq!(table.rows[0][2], "Refined Metal (1.00 ref)");
    assert_eq!(table.rows[0][3], "Unresolved (not tracked while active)");
    assert_eq!(table.rows[0][4], "5.50");
}

#[test]
fn portfolio_table_formats_totals() {
    let snapshot = PortfolioSnapshotView {
        ts: 1_700_000_000.0,
        total_ref: 486.2,
        total_keys: 7.2,
        pure_keys: 3,
        pure_metal_ref: 12.33,
        item_count: 214,
        unusual_count: 2,
        australium_count: 1,
    };

    let table = portfolio_table(&[snapshot]);
    assert_eq!(table.rows[0][1], "486.20");
    assert_eq!(table.rows[0][5], "214");
}

#[test]
fn format_ts_renders_a_known_unix_timestamp() {
    assert_eq!(format_ts(0.0), "1970-01-01 00:00:00 UTC");
}

#[test]
fn bool_str_and_opt_render_as_expected() {
    assert_eq!(bool_str(true), "Yes");
    assert_eq!(bool_str(false), "No");
    assert_eq!(opt(Some(5)), "5");
    assert_eq!(opt::<i32>(None), "");
}
