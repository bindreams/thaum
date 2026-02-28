//! Table formatting from benchmark results.

use std::collections::HashMap;

use thaum::table::{Align, Table};

use super::types::{BenchResult, Kind, Value};

/// Format a number with thousands separators.
fn format_count(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format a time value with auto-scaling.
fn format_time(mean: f64, stddev: f64) -> String {
    let (val, sd, unit) = if mean < 1e-3 {
        (mean * 1e6, stddev * 1e6, "µs")
    } else if mean < 1.0 {
        (mean * 1e3, stddev * 1e3, "ms")
    } else {
        (mean, stddev, "s")
    };
    format!("{val:.2}±{sd:.2}{unit}")
}

/// Format a single cell value.
fn format_value(val: &Value) -> String {
    match val {
        Value::Count(n) => format_count(*n),
        Value::Time { mean, stddev } => format_time(*mean, *stddev),
    }
}

/// Compute percentage change between two values.
fn pct_change(current: &Value, baseline: &Value) -> String {
    match (current, baseline) {
        (Value::Count(cur), Value::Count(base)) => {
            if *base == 0 {
                return "N/A".to_string();
            }
            let p = ((*cur as f64 - *base as f64) / *base as f64) * 100.0;
            let sign = if p >= 0.0 { "+" } else { "" };
            format!("{sign}{p:.2}%")
        }
        (Value::Time { mean: cur, .. }, Value::Time { mean: base, .. }) => {
            if *base == 0.0 {
                return "N/A".to_string();
            }
            let p = ((cur - base) / base) * 100.0;
            let sign = if p >= 0.0 { "+" } else { "" };
            format!("{sign}{p:.2}%")
        }
        _ => "N/A".to_string(),
    }
}

/// Baseline lookup: name → (kind → value).
pub type Baseline = HashMap<String, HashMap<Kind, Value>>;

/// Parse a baseline from JSON lines (our own format).
pub fn parse_baseline(json_str: &str) -> Baseline {
    let mut map = Baseline::new();
    for line in json_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<BenchResult>(line) else {
            continue;
        };
        map.insert(entry.name.clone(), entry.measurements);
    }
    map
}

/// Build a table from benchmark results.
pub fn build_table(results: &[BenchResult], kinds: &[Kind], baseline: Option<&Baseline>) -> Table {
    let mut table = Table::new().col("BENCHMARK", Align::Left);

    for kind in kinds {
        table = table.col(&kind.to_string().to_uppercase(), Align::Right);
        if baseline.is_some() {
            table = table.col("vs. BASE", Align::Right);
        }
    }

    for result in results {
        let mut cells: Vec<String> = vec![result.name.clone()];

        for kind in kinds {
            if let Some(val) = result.measurements.get(kind) {
                cells.push(format_value(val));
                if let Some(base) = baseline {
                    let pct = base
                        .get(&result.name)
                        .and_then(|m| m.get(kind))
                        .map(|bv| pct_change(val, bv))
                        .unwrap_or_default();
                    cells.push(pct);
                }
            } else {
                cells.push("-".to_string());
                if baseline.is_some() {
                    cells.push(String::new());
                }
            }
        }

        let cell_refs: Vec<&str> = cells.iter().map(|s| s.as_str()).collect();
        table = table.row(&cell_refs);
    }

    table
}
