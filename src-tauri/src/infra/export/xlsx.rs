use rust_xlsxwriter::{Format, Workbook};

use super::ExportTable;
use crate::error::{AppError, AppResult};

pub fn write(table: &ExportTable) -> AppResult<Vec<u8>> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    let header_format = Format::new().set_bold();

    for (col, header) in table.headers.iter().enumerate() {
        sheet
            .write_string_with_format(0, col as u16, header, &header_format)
            .map_err(|e| AppError::Export(e.to_string()))?;
    }
    for (row_idx, row) in table.rows.iter().enumerate() {
        for (col, value) in row.iter().enumerate() {
            sheet
                .write_string((row_idx + 1) as u32, col as u16, value)
                .map_err(|e| AppError::Export(e.to_string()))?;
        }
    }

    workbook
        .save_to_buffer()
        .map_err(|e| AppError::Export(e.to_string()))
}

#[cfg(test)]
#[path = "xlsx_tests.rs"]
mod tests;
