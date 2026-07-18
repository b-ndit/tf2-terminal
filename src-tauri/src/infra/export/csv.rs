use super::ExportTable;
use crate::error::{AppError, AppResult};

pub fn write(table: &ExportTable) -> AppResult<Vec<u8>> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record(&table.headers)
        .map_err(|e| AppError::Export(e.to_string()))?;
    for row in &table.rows {
        writer
            .write_record(row)
            .map_err(|e| AppError::Export(e.to_string()))?;
    }
    writer
        .into_inner()
        .map_err(|e| AppError::Export(e.to_string()))
}

#[cfg(test)]
#[path = "csv_tests.rs"]
mod tests;
