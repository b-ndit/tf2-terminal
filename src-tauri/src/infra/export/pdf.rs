//! A hand-rolled tabular report, not `genpdf` — `genpdf`'s table layout is
//! more ergonomic but needs an embedded TTF font file bundled into the
//! binary; PDF's built-in Helvetica (`BuiltinFont::Helvetica`) needs no
//! font asset at all, which matters more for a packaged desktop app than
//! saving a few dozen lines of manual column layout.

use printpdf::{
    BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt, TextItem,
};

use super::ExportTable;
use crate::error::AppResult;

const PAGE_WIDTH_MM: f32 = 297.0; // landscape A4 — export tables tend to be wide, not tall
const PAGE_HEIGHT_MM: f32 = 210.0;
const MARGIN_MM: f32 = 15.0;
const ROW_HEIGHT_MM: f32 = 6.0;
const HEADER_FONT_SIZE: f32 = 10.0;
const ROW_FONT_SIZE: f32 = 9.0;
/// Rough average glyph width for Helvetica at 9-10pt — used only to avoid
/// egregious cell overlap for long values, not exact typesetting.
const MM_PER_CHAR: f32 = 1.8;

pub fn write(title: &str, table: &ExportTable) -> AppResult<Vec<u8>> {
    let mut doc = PdfDocument::new(title);
    let usable_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM;
    let col_width = if table.headers.is_empty() {
        usable_width
    } else {
        usable_width / table.headers.len() as f32
    };
    let usable_height = PAGE_HEIGHT_MM - 2.0 * MARGIN_MM - ROW_HEIGHT_MM; // reserve one row for the header
    let rows_per_page = ((usable_height / ROW_HEIGHT_MM) as usize).max(1);
    let max_chars = (col_width / MM_PER_CHAR) as usize;

    let row_chunks: Vec<&[Vec<String>]> = if table.rows.is_empty() {
        vec![&[]]
    } else {
        table.rows.chunks(rows_per_page).collect()
    };

    let pages = row_chunks
        .into_iter()
        .map(|chunk| build_page(title, table, chunk, col_width, max_chars))
        .collect();

    let mut warnings = Vec::new();
    Ok(doc
        .with_pages(pages)
        .save(&PdfSaveOptions::default(), &mut warnings))
}

fn build_page(
    title: &str,
    table: &ExportTable,
    rows: &[Vec<String>],
    col_width: f32,
    max_chars: usize,
) -> PdfPage {
    let mut ops = vec![Op::StartTextSection];
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    ops.push(set_font(BuiltinFont::HelveticaBold, HEADER_FONT_SIZE + 4.0));
    ops.push(cursor(MARGIN_MM, y));
    ops.push(show_text(title));
    y -= ROW_HEIGHT_MM + 2.0;

    ops.push(set_font(BuiltinFont::HelveticaBold, HEADER_FONT_SIZE));
    for (i, header) in table.headers.iter().enumerate() {
        ops.push(cursor(MARGIN_MM + i as f32 * col_width, y));
        ops.push(show_text(&truncate(header, max_chars)));
    }
    y -= ROW_HEIGHT_MM;

    ops.push(set_font(BuiltinFont::Helvetica, ROW_FONT_SIZE));
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            ops.push(cursor(MARGIN_MM + i as f32 * col_width, y));
            ops.push(show_text(&truncate(cell, max_chars)));
        }
        y -= ROW_HEIGHT_MM;
    }

    ops.push(Op::EndTextSection);
    PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops)
}

fn set_font(font: BuiltinFont, size: f32) -> Op {
    Op::SetFont {
        font: PdfFontHandle::Builtin(font),
        size: Pt(size),
    }
}

fn cursor(x_mm: f32, y_mm: f32) -> Op {
    Op::SetTextCursor {
        pos: Point::new(Mm(x_mm), Mm(y_mm)),
    }
}

fn show_text(text: &str) -> Op {
    Op::ShowText {
        items: vec![TextItem::Text(text.to_string())],
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if max_chars == 0 || s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[path = "pdf_tests.rs"]
mod tests;
