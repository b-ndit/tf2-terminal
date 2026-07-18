use super::*;

#[test]
fn writes_header_and_rows_roundtrippable() {
    let table = ExportTable {
        headers: vec!["Name".to_string(), "Quality".to_string()],
        rows: vec![
            vec!["Team Captain".to_string(), "Unique".to_string()],
            vec!["Rocket Launcher, Mk. II".to_string(), "Strange".to_string()],
        ],
    };

    let bytes = write(&table).unwrap();
    let mut reader = ::csv::Reader::from_reader(bytes.as_slice());

    assert_eq!(
        reader.headers().unwrap().iter().collect::<Vec<_>>(),
        vec!["Name", "Quality"]
    );
    let records: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].get(0), Some("Rocket Launcher, Mk. II"));
}

#[test]
fn empty_table_still_writes_a_header_row() {
    let table = ExportTable {
        headers: vec!["Name".to_string()],
        rows: vec![],
    };

    let bytes = write(&table).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(text.trim(), "Name");
}
