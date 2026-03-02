//! Column-aligned table formatter for terminal output.
//!
//! ```
//! use thaum::table::{Table, Align};
//!
//! let table = Table::new()
//!     .col("NAME", Align::Left)
//!     .col("VALUE", Align::Right)
//!     .col("CHANGE", Align::Right)
//!     .row(&["alpha", "1,234", "+0.5%"])
//!     .row(&["beta",  "5,678", "-1.2%"]);
//!
//! assert_eq!(table.to_string(),
//!     "NAME   VALUE  CHANGE\n\
//!      -----  -----  ------\n\
//!      alpha  1,234   +0.5%\n\
//!      beta   5,678   -1.2%\n");
//! ```

use std::fmt;

/// Column alignment.
#[derive(Clone, Copy)]
pub enum Align {
    Left,
    Right,
}

struct Column {
    header: String,
    align: Align,
}

/// A table that auto-sizes columns and prints with consistent alignment.
///
/// Columns are defined up front via [`col`](Table::col). Rows are added via
/// [`row`](Table::row) and may have fewer cells than columns (missing cells are
/// treated as empty). The first column is special: when a cell equals the
/// previous row's first cell, it is printed as blank (row grouping).
pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new() -> Self {
        Table {
            columns: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// Add a column definition. Columns appear left-to-right in definition order.
    pub fn col(mut self, header: &str, align: Align) -> Self {
        self.columns.push(Column {
            header: header.to_string(),
            align,
        });
        self
    }

    /// Append a row. Cells correspond to columns by position.
    pub fn row(mut self, cells: &[&str]) -> Self {
        self.rows.push(cells.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Computed width for each column: max(header, all cells).
    fn widths(&self) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let max_cell = self
                    .rows
                    .iter()
                    .map(|r| r.get(i).map_or(0, |c| c.len()))
                    .max()
                    .unwrap_or(0);
                col.header.len().max(max_cell)
            })
            .collect()
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let widths = self.widths();

        // Helper: format cells into a line and trim trailing whitespace.
        let mut line = |cells: &[(&str, Align, usize)]| -> fmt::Result {
            let mut buf = String::new();
            for (i, &(text, align, w)) in cells.iter().enumerate() {
                if i > 0 {
                    buf.push_str("  ");
                }
                match align {
                    Align::Left => buf.push_str(&format!("{text:<w$}")),
                    Align::Right => buf.push_str(&format!("{text:>w$}")),
                }
            }
            writeln!(f, "{}", buf.trim_end())
        };

        // Header.
        let header: Vec<_> = self
            .columns
            .iter()
            .zip(&widths)
            .map(|(col, &w)| (col.header.as_str(), col.align, w))
            .collect();
        line(&header)?;

        // Separator.
        let dashes: Vec<String> = widths.iter().map(|&w| "-".repeat(w)).collect();
        let sep: Vec<_> = dashes
            .iter()
            .zip(&widths)
            .map(|(d, &w)| (d.as_str(), Align::Left, w))
            .collect();
        line(&sep)?;

        // Rows with first-column grouping.
        let mut prev_first = String::new();
        for row in &self.rows {
            let cells: Vec<_> = self
                .columns
                .iter()
                .zip(&widths)
                .enumerate()
                .map(|(i, (col, &w))| {
                    let cell = row.get(i).map_or("", |s| s.as_str());
                    let display = if i == 0 {
                        if cell == prev_first {
                            ""
                        } else {
                            prev_first = cell.to_string();
                            cell
                        }
                    } else {
                        cell
                    };
                    (display, col.align, w)
                })
                .collect();
            line(&cells)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod table_tests;
