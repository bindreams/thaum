//! Label constants shared by all integration-test binaries. Each binary that
//! includes `common` registers all labels defined here regardless of whether
//! it uses them — skuld only panics on duplicate definitions, not on unused
//! ones, so a single source of truth is cheaper than per-binary subsets.

skuld::new_label!(pub LEX, "lex");
skuld::new_label!(pub PARSE, "parse");
skuld::new_label!(pub EXEC, "exec");
skuld::new_label!(pub CLI, "cli");
skuld::new_label!(pub INTERACTIVE, "interactive");
skuld::new_label!(pub BENCH, "bench");
skuld::new_label!(pub INFRA, "infra");
skuld::new_label!(pub DOCKER, "docker");
skuld::new_label!(pub GAUNTLET, "gauntlet");
