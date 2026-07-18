use super::*;

#[test]
fn produces_a_valid_pdf_document() {
    let table = ExportTable {
        headers: vec!["Name".to_string(), "Value (ref)".to_string()],
        rows: vec![vec!["Team Captain".to_string(), "42.5".to_string()]],
    };

    let bytes = write("Backpack Export", &table).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn empty_rows_still_produce_a_valid_pdf_with_headers() {
    let table = ExportTable {
        headers: vec!["Name".to_string()],
        rows: vec![],
    };

    let bytes = write("Empty Export", &table).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn truncate_leaves_short_strings_untouched() {
    assert_eq!(truncate("Team Captain", 20), "Team Captain");
}

#[test]
fn truncate_shortens_long_strings_with_an_ellipsis() {
    let out = truncate("A very long item name that overflows a column", 10);
    assert_eq!(out.chars().count(), 10);
    assert!(out.ends_with('…'));
}
