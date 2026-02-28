//! Parse callgrind output files into structured metrics.
//!
//! Callgrind (part of Valgrind) produces text output files with instruction
//! counts and optional cache simulation data. This module extracts the summary
//! counters from these files.

/// Raw counters from a callgrind run with `--cache-sim=yes`.
///
/// When cache simulation is disabled, only `ir` is populated; the rest are 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallgrindMetrics {
    /// Instructions executed.
    pub ir: u64,
    /// Data reads.
    pub dr: u64,
    /// Data writes.
    pub dw: u64,
    /// L1 instruction cache read misses.
    pub i1mr: u64,
    /// L1 data cache read misses.
    pub d1mr: u64,
    /// L1 data cache write misses.
    pub d1mw: u64,
    /// Last-level instruction cache read misses (→ RAM).
    pub ilmr: u64,
    /// Last-level data cache read misses (→ RAM).
    pub dlmr: u64,
    /// Last-level data cache write misses (→ RAM).
    pub dlmw: u64,
}

impl CallgrindMetrics {
    /// Total L1 cache hits: accesses that did not miss L1.
    pub fn l1_hits(&self) -> u64 {
        (self.ir + self.dr + self.dw).saturating_sub(self.i1mr + self.d1mr + self.d1mw)
    }

    /// Last-level cache hits: L1 misses that were served by LL cache.
    pub fn ll_hits(&self) -> u64 {
        (self.i1mr + self.d1mr + self.d1mw).saturating_sub(self.ilmr + self.dlmr + self.dlmw)
    }

    /// RAM hits: LL misses that went to main memory.
    pub fn ram_hits(&self) -> u64 {
        self.ilmr + self.dlmr + self.dlmw
    }

    /// Estimated CPU cycles (Cachegrind model: 1 cycle per L1 hit, 10 per LL
    /// hit, 100 per RAM access).
    pub fn est_cycles(&self) -> u64 {
        self.l1_hits() + 10 * self.ll_hits() + 100 * self.ram_hits()
    }
}

/// Parse a callgrind output file into metrics.
///
/// Reads the `events:` line for column names and the `summary:` line for totals.
/// Columns not present in the events list default to 0.
pub fn parse(text: &str) -> Result<CallgrindMetrics, String> {
    let mut event_names: Vec<&str> = Vec::new();
    let mut summary_values: Vec<u64> = Vec::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("events: ") {
            event_names = rest.split_whitespace().collect();
        } else if let Some(rest) = line.strip_prefix("summary: ") {
            summary_values = rest.split_whitespace().map(|s| s.parse::<u64>().unwrap_or(0)).collect();
        }
    }

    if event_names.is_empty() {
        return Err("no 'events:' line found in callgrind output".to_string());
    }
    if summary_values.is_empty() {
        return Err("no 'summary:' line found in callgrind output".to_string());
    }

    let get = |name: &str| -> u64 {
        event_names
            .iter()
            .position(|&n| n == name)
            .and_then(|i| summary_values.get(i).copied())
            .unwrap_or(0)
    };

    Ok(CallgrindMetrics {
        ir: get("Ir"),
        dr: get("Dr"),
        dw: get("Dw"),
        i1mr: get("I1mr"),
        d1mr: get("D1mr"),
        d1mw: get("D1mw"),
        ilmr: get("ILmr"),
        dlmr: get("DLmr"),
        dlmw: get("DLmw"),
    })
}

#[cfg(test)]
mod callgrind_parser_tests;
