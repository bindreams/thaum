use super::*;

const FULL_OUTPUT: &str = "\
# callgrind format
version: 1
creator: callgrind-3.22.0
pid: 12345
cmd:  /usr/local/bin/bench --run-bench trivial lex
part: 1

desc: I1 cache: 32768 B, 64 B, 8-way associative
desc: D1 cache: 32768 B, 64 B, 8-way associative
desc: LL cache: 268435456 B, 64 B, 2-way associative

positions: line
events: Ir Dr Dw I1mr D1mr D1mw ILmr DLmr DLmw
summary: 6028 1715 1195 120 16 12 119 11 6

ob=(2) /usr/lib/x86_64-linux-gnu/libc.so.6
fl=(1) some_file.rs
fn=(1) main
1 100 50 30 1 1 1 0 0 0
";

#[test]
fn parse_full_output() {
    let m = parse(FULL_OUTPUT).unwrap();
    assert_eq!(m.ir, 6028);
    assert_eq!(m.dr, 1715);
    assert_eq!(m.dw, 1195);
    assert_eq!(m.i1mr, 120);
    assert_eq!(m.d1mr, 16);
    assert_eq!(m.d1mw, 12);
    assert_eq!(m.ilmr, 119);
    assert_eq!(m.dlmr, 11);
    assert_eq!(m.dlmw, 6);
}

#[test]
fn derived_metrics() {
    let m = parse(FULL_OUTPUT).unwrap();
    // L1 hits = (6028+1715+1195) - (120+16+12) = 8938 - 148 = 8790
    assert_eq!(m.l1_hits(), 8790);
    // LL hits = (120+16+12) - (119+11+6) = 148 - 136 = 12
    assert_eq!(m.ll_hits(), 12);
    // RAM hits = 119+11+6 = 136
    assert_eq!(m.ram_hits(), 136);
    // Est cycles = 8790 + 10*12 + 100*136 = 8790 + 120 + 13600 = 22510
    assert_eq!(m.est_cycles(), 22510);
}

#[test]
fn instructions_only() {
    // When --cache-sim is not used, callgrind only reports Ir.
    let text = "\
events: Ir
summary: 42000
";
    let m = parse(text).unwrap();
    assert_eq!(m.ir, 42000);
    assert_eq!(m.dr, 0);
    assert_eq!(m.l1_hits(), 42000); // all hits when no misses
    assert_eq!(m.ram_hits(), 0);
}

#[test]
fn missing_events_line() {
    let text = "summary: 100\n";
    assert!(parse(text).is_err());
}

#[test]
fn missing_summary_line() {
    let text = "events: Ir\n";
    assert!(parse(text).is_err());
}

#[test]
fn extra_events_ignored() {
    // If callgrind adds new events, we ignore them gracefully.
    let text = "\
events: Ir Dr Dw FutureEvent I1mr D1mr D1mw ILmr DLmr DLmw
summary: 100 50 30 999 1 2 3 0 0 0
";
    let m = parse(text).unwrap();
    assert_eq!(m.ir, 100);
    assert_eq!(m.dr, 50);
    assert_eq!(m.i1mr, 1);
}
