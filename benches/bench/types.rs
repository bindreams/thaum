//! Shared types for the benchmark system.

use std::collections::HashMap;
use std::fmt;

/// A pipeline stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Lex,
    Parse,
    Exec,
    Total,
}

impl Stage {
    pub const ALL: &[Stage] = &[Stage::Lex, Stage::Parse, Stage::Exec, Stage::Total];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Lex => "lex",
            Stage::Parse => "parse",
            Stage::Exec => "exec",
            Stage::Total => "total",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stage::Lex => "lex",
            Stage::Parse => "parse",
            Stage::Exec => "exec",
            Stage::Total => "total",
        })
    }
}

impl std::str::FromStr for Stage {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "lex" => Ok(Stage::Lex),
            "parse" => Ok(Stage::Parse),
            "exec" => Ok(Stage::Exec),
            "total" => Ok(Stage::Total),
            other => Err(format!("unknown stage: {other}")),
        }
    }
}

/// A measurement metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Instructions,
    DataReads,
    DataWrites,
    L1Hits,
    LlHits,
    RamHits,
    EstCycles,
    Walltime,
}

impl Metric {
    pub const ALL: &[Metric] = &[
        Metric::Instructions,
        Metric::DataReads,
        Metric::DataWrites,
        Metric::L1Hits,
        Metric::LlHits,
        Metric::RamHits,
        Metric::EstCycles,
        Metric::Walltime,
    ];

    /// Whether this metric comes from the callgrind backend.
    pub fn is_callgrind(self) -> bool {
        !matches!(self, Metric::Walltime)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Metric::Instructions => "instructions",
            Metric::DataReads => "data-reads",
            Metric::DataWrites => "data-writes",
            Metric::L1Hits => "l1-hits",
            Metric::LlHits => "ll-hits",
            Metric::RamHits => "ram-hits",
            Metric::EstCycles => "est-cycles",
            Metric::Walltime => "walltime",
        }
    }
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Metric::Instructions => "instructions",
            Metric::DataReads => "data-reads",
            Metric::DataWrites => "data-writes",
            Metric::L1Hits => "l1-hits",
            Metric::LlHits => "ll-hits",
            Metric::RamHits => "ram-hits",
            Metric::EstCycles => "est-cycles",
            Metric::Walltime => "walltime",
        })
    }
}

impl std::str::FromStr for Metric {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "instructions" => Ok(Metric::Instructions),
            "data-reads" => Ok(Metric::DataReads),
            "data-writes" => Ok(Metric::DataWrites),
            "l1-hits" => Ok(Metric::L1Hits),
            "ll-hits" => Ok(Metric::LlHits),
            "ram-hits" => Ok(Metric::RamHits),
            "est-cycles" => Ok(Metric::EstCycles),
            "walltime" => Ok(Metric::Walltime),
            other => Err(format!("unknown metric: {other}")),
        }
    }
}

/// A fully qualified kind: stage + metric (e.g. "lex.instructions").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Kind {
    pub stage: Stage,
    pub metric: Metric,
}

impl Kind {
    /// Enumerate all valid stage.metric combinations.
    pub fn all() -> Vec<Kind> {
        let mut kinds = Vec::new();
        for &stage in Stage::ALL {
            for &metric in Metric::ALL {
                kinds.push(Kind { stage, metric });
            }
        }
        kinds
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.stage, self.metric)
    }
}

/// Resolve a comma-separated glob pattern into concrete Kind values.
///
/// Each element is matched against all valid `stage.metric` pairs using simple
/// glob semantics (`*` matches any sequence of characters).  A bare stage name
/// (e.g. `lex`) is treated as `lex.*`, and a bare metric name (e.g.
/// `instructions`) is treated as `*.instructions`.
pub fn resolve_kinds(pattern: &str) -> Vec<Kind> {
    let all = Kind::all();
    let stage_names: Vec<&str> = Stage::ALL.iter().map(|s| s.as_str()).collect();
    let metric_names: Vec<&str> = Metric::ALL.iter().map(|m| m.as_str()).collect();

    let mut result = Vec::new();
    for glob in pattern.split(',').map(str::trim) {
        // Expand bare stage/metric names to wildcard patterns.
        let expanded: std::borrow::Cow<'_, str> = if !glob.contains('.') && !glob.contains('*') {
            if stage_names.contains(&glob) {
                format!("{glob}.*").into()
            } else if metric_names.contains(&glob) {
                format!("*.{glob}").into()
            } else {
                glob.into()
            }
        } else {
            glob.into()
        };

        for kind in &all {
            if glob_matches(&expanded, &kind.to_string()) && !result.contains(kind) {
                result.push(*kind);
            }
        }
    }
    result
}

/// Simple glob matching: `*` matches any sequence of characters.
fn glob_matches(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    glob_match_bytes(p, t)
}

fn glob_match_bytes(p: &[u8], t: &[u8]) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = usize::MAX;
    let mut star_ti = 0;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == t[ti] || p[pi] == b'?') {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// A single measurement value.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// Integer count (instruction counts, cache hits, etc.).
    Count(u64),
    /// Time measurement with mean and standard deviation in seconds.
    Time { mean: f64, stddev: f64 },
}

/// One benchmark entry with all its measurements.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchResult {
    /// Script name (no stage tag).
    pub name: String,
    /// Measurements keyed by kind (stage.metric).
    pub measurements: HashMap<Kind, Value>,
}

/// A benchmark script loaded from disk.
pub struct Script {
    /// Stem (filename without extension): "trivial", "arithmetic", etc.
    pub name: String,
    /// "bash" or "posix".
    pub dialect: String,
    /// Path to a plain .sh file (for hyperfine/valgrind to invoke).
    pub path: std::path::PathBuf,
}

/// Load benchmark scripts from a path (file or directory).
pub fn load_scripts(path: &std::path::Path) -> Vec<Script> {
    use thaum::testkit::sh_yaml::ShYaml;

    if path.is_dir() {
        let specs = ShYaml::load_dir(path).unwrap_or_else(|e| panic!("{e}"));
        specs.into_iter().map(Script::from_sh_yaml).collect()
    } else {
        let spec = ShYaml::load(path).unwrap_or_else(|e| panic!("{e}"));
        vec![Script::from_sh_yaml(spec)]
    }
}

impl Script {
    fn from_sh_yaml(spec: thaum::testkit::sh_yaml::ShYaml) -> Self {
        let sh_path = std::env::temp_dir()
            .join("thaum-bench-scripts")
            .join(format!("{}.sh", spec.name));
        std::fs::create_dir_all(sh_path.parent().unwrap()).ok();
        std::fs::write(&sh_path, &spec.body).expect("cannot write temp script");

        Script {
            name: spec.name,
            dialect: spec.dialect,
            path: sh_path,
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;

    #[test]
    fn glob_exact() {
        assert!(glob_matches("lex.instructions", "lex.instructions"));
        assert!(!glob_matches("lex.instructions", "parse.instructions"));
    }

    #[test]
    fn glob_star() {
        assert!(glob_matches("*", "lex.instructions"));
        assert!(glob_matches("lex.*", "lex.instructions"));
        assert!(glob_matches("lex.*", "lex.walltime"));
        assert!(glob_matches("*.instructions", "lex.instructions"));
        assert!(glob_matches("*.instructions", "exec.instructions"));
        assert!(!glob_matches("*.instructions", "exec.walltime"));
    }

    #[test]
    fn resolve_star() {
        let kinds = resolve_kinds("*");
        assert_eq!(kinds.len(), Stage::ALL.len() * Metric::ALL.len());
    }

    #[test]
    fn resolve_stage_star() {
        let kinds = resolve_kinds("lex.*");
        assert!(kinds.iter().all(|k| k.stage == Stage::Lex));
        assert_eq!(kinds.len(), Metric::ALL.len());
    }

    #[test]
    fn resolve_metric_star() {
        let kinds = resolve_kinds("*.walltime");
        assert!(kinds.iter().all(|k| k.metric == Metric::Walltime));
        assert_eq!(kinds.len(), Stage::ALL.len());
    }

    #[test]
    fn resolve_comma_separated() {
        let kinds = resolve_kinds("lex.instructions,exec.walltime");
        assert_eq!(kinds.len(), 2);
        assert_eq!(
            kinds[0],
            Kind {
                stage: Stage::Lex,
                metric: Metric::Instructions
            }
        );
        assert_eq!(
            kinds[1],
            Kind {
                stage: Stage::Exec,
                metric: Metric::Walltime
            }
        );
    }

    #[test]
    fn resolve_deduplicates() {
        let kinds = resolve_kinds("lex.instructions,lex.*");
        // lex.instructions appears in both, but only once in result.
        let count = kinds
            .iter()
            .filter(|k| k.stage == Stage::Lex && k.metric == Metric::Instructions)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn resolve_bare_metric_name() {
        let kinds = resolve_kinds("instructions");
        assert!(kinds.iter().all(|k| k.metric == Metric::Instructions));
        assert_eq!(kinds.len(), Stage::ALL.len());
    }

    #[test]
    fn resolve_bare_stage_name() {
        let kinds = resolve_kinds("lex");
        assert!(kinds.iter().all(|k| k.stage == Stage::Lex));
        assert_eq!(kinds.len(), Metric::ALL.len());
    }
}
