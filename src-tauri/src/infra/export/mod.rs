//! Export writers (Module 15): pure byte-generating adapters for external
//! file formats, the same role `infra::backpack_tf::models` plays for JSON
//! mapping — not `domain::` (TF2 business logic, not generic file-format
//! serialization) even though none of them do any I/O themselves. The
//! actual `std::fs::write` happens one level up, in
//! `services::export_service`.

pub mod csv;
pub mod pdf;
pub mod xlsx;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Xlsx,
    Json,
    Pdf,
}

/// Generic tabular shape the CSV/XLSX/PDF writers all consume — decouples
/// format logic from any one dataset's Rust struct. JSON export bypasses
/// this and serializes the original structured data directly instead (see
/// `services::export_service`), since flattening to strings would throw
/// away type information a JSON consumer would want back.
#[derive(Debug, Clone, Default)]
pub struct ExportTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
