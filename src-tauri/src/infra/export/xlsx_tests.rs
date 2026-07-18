use super::*;

const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

#[test]
fn produces_a_valid_zip_container() {
    let table = ExportTable {
        headers: vec!["Name".to_string(), "Value (ref)".to_string()],
        rows: vec![vec!["Team Captain".to_string(), "42.5".to_string()]],
    };

    let bytes = write(&table).unwrap();
    assert!(!bytes.is_empty());
    assert_eq!(&bytes[0..4], &ZIP_MAGIC);
}

#[test]
fn empty_rows_still_produce_a_valid_workbook() {
    let table = ExportTable {
        headers: vec!["Name".to_string()],
        rows: vec![],
    };

    let bytes = write(&table).unwrap();
    assert_eq!(&bytes[0..4], &ZIP_MAGIC);
}
