//! Shell state: variables (scalar, indexed-array, associative-array), exported
//! vars, functions, aliases, positional parameters, CWD, and `$?`.  Supports
//! `declare`/`typeset` attribute flags (`-i`, `-r`, `-l`, `-u`, `-x`), scoped
//! positional parameters for function calls, and serialization for subshell
//! spawning.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use contracts::debug_ensures;
use serde::{Deserialize, Serialize};

use crate::ast::{CompoundCommand, FunctionDef};
use crate::exec::error::ExecError;

/// The value of a shell variable — scalar, indexed array, or associative array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VarValue {
    /// A single string value (POSIX).
    Scalar(String),
    /// A sparse indexed array (Bash).  Uses `BTreeMap` so iteration is in
    /// index order, matching bash's `${a[@]}` behaviour.
    IndexedArray(BTreeMap<usize, String>),
    /// An associative array (Bash `declare -A`).  String-keyed.
    AssocArray(HashMap<String, String>),
}

/// A shell variable with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellVar {
    value: VarValue,
    exported: bool,
    readonly: bool,
    /// `-i` flag: evaluate RHS as arithmetic on assignment.
    integer: bool,
    /// `-l` flag: convert value to lowercase on assignment.
    lowercase: bool,
    /// `-u` flag: convert value to uppercase on assignment.
    uppercase: bool,
    /// `-n` flag: nameref — this variable is an alias for the named target.
    nameref: Option<String>,
}

impl ShellVar {
    /// Return the scalar string for this variable.
    ///
    /// For scalars, returns the value directly. For indexed arrays, returns
    /// element 0 (bash compatibility: `$a` == `${a[0]}`). For associative
    /// arrays, returns element with key "0" or empty string.
    fn scalar_str(&self) -> &str {
        match &self.value {
            VarValue::Scalar(s) => s,
            VarValue::IndexedArray(map) => map.get(&0).map(|s| s.as_str()).unwrap_or(""),
            VarValue::AssocArray(map) => map.get("0").map(|s| s.as_str()).unwrap_or(""),
        }
    }
}

/// Attributes parsed from `declare`/`typeset` flags.
#[derive(Clone, Default, Debug)]
pub struct DeclareAttrs {
    /// `-a`: create indexed array.
    pub indexed_array: bool,
    /// `-A`: create associative array.
    pub assoc_array: bool,
    /// `-r`: mark readonly.
    pub readonly_set: bool,
    /// `-x`: mark exported.
    pub exported_set: bool,
    /// `-i`: integer attribute (arithmetic evaluation on assignment).
    pub integer_set: bool,
    /// `-l`: lowercase on assignment.
    pub lowercase_set: bool,
    /// `-u`: uppercase on assignment.
    pub uppercase_set: bool,
    /// `-n`: nameref (variable is an alias for the named target).
    pub nameref_set: bool,
    /// `-g`: force global scope (even inside a function).
    pub global: bool,
    /// `-p`: print variable declarations.
    pub print: bool,
    /// `-f`: list functions.
    pub list_functions: bool,
    /// `-F`: list function names only.
    pub list_function_names: bool,

    // Attribute removal flags (via `+` prefix) ----
    /// `+x`: remove export attribute.
    pub unexport: bool,
    /// `+r`: remove readonly (only effective when `typeset_can_unset_readonly` is enabled).
    pub unreadonly: bool,
    /// `+i`: remove integer attribute.
    pub uninteger: bool,
    /// `+l`: remove lowercase attribute.
    pub unlowercase: bool,
    /// `+u`: remove uppercase attribute.
    pub unuppercase: bool,
}

/// A saved scope frame for function calls.
///
/// In POSIX, function calls only save/restore positional parameters.
/// Regular variables are NOT scoped — they are global. The `saved` map
/// is used by the `local` builtin (Bash extension).
#[derive(Debug, Clone)]
struct Scope {
    /// Variables saved for restoration (used by `local` — Bash extension).
    saved: HashMap<String, Option<ShellVar>>,
    /// Positional parameters saved from the caller.
    saved_positional: Vec<String>,
    /// Function name for this scope (used by FUNCNAME).
    function_name: String,
    /// Source file for this scope (used by BASH_SOURCE).
    source_file: String,
    /// Line number where this function was called (used by BASH_LINENO).
    call_lineno: usize,
}

/// Metadata about a function/source call, used to populate call-stack variables.
#[derive(Debug, Clone, Default)]
pub struct CallInfo {
    pub function_name: String,
    pub source_file: String,
    pub call_lineno: usize,
}

/// The shell execution environment.
///
/// Holds variables, functions, positional parameters, CWD, exit status,
/// and a scope stack for function calls.
#[derive(Debug)]
pub struct Environment {
    variables: HashMap<String, ShellVar>,
    functions: HashMap<String, StoredFunction>,
    positional_params: Vec<String>,
    program_name: String,
    last_exit_status: i32,
    last_bg_pid: Option<u32>,
    cwd: PathBuf,
    pid: u32,
    scope_stack: Vec<Scope>,
    aliases: HashMap<String, String>,
    expand_aliases: bool,
    errexit: bool,
    nounset: bool,
    xtrace: bool,
    /// Bash 4.x bug: `"${a[@]:+word}"` on array with empty element returns word.
    array_empty_element_alternative_bug: bool,
    /// When true, `typeset +r` / `declare +r` removes the readonly attribute.
    typeset_can_unset_readonly: bool,

    // Dynamic variable state -----
    /// Tracks which Category-A dynamic variables still have special behavior.
    /// When a variable is unset, it is removed from this set permanently.
    special_active: HashSet<String>,
    /// RANDOM LCG state.
    random_state: u32,
    /// Shell start time (Unix epoch seconds) for SECONDS.
    start_epoch_secs: u64,
    /// User-assigned offset for SECONDS (from `SECONDS=N`).
    seconds_offset: i64,
    /// Current source line number (updated by the executor).
    lineno: usize,
    /// User-assigned offset for LINENO (from `LINENO=N`).
    lineno_offset: isize,
    /// Internal sub-index for getopts grouped option processing (e.g. `-abc`).
    /// Reset when OPTIND is set to 1.
    getopts_subindex: usize,
}

/// A stored function definition (just the parts we need for execution).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFunction {
    pub body: CompoundCommand,
    pub redirects: Vec<crate::ast::Redirect>,
}

impl From<&FunctionDef> for StoredFunction {
    fn from(def: &FunctionDef) -> Self {
        StoredFunction {
            body: (*def.body).clone(),
            redirects: def.redirects.clone(),
        }
    }
}

impl Environment {
    /// Create a new environment with sensible defaults.
    ///
    /// Inherits CWD and PID from the current process. Sets default IFS.
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let pid = std::process::id();

        let mut env = Environment {
            variables: HashMap::new(),
            functions: HashMap::new(),
            positional_params: Vec::new(),
            program_name: String::from("sh"),
            last_exit_status: 0,
            last_bg_pid: None,
            cwd,
            pid,
            scope_stack: Vec::new(),
            aliases: HashMap::new(),
            expand_aliases: false,
            errexit: false,
            nounset: false,
            xtrace: false,
            array_empty_element_alternative_bug: false,
            typeset_can_unset_readonly: false,
            special_active: default_special_active(),
            random_state: pid ^ epoch_secs_now() as u32,
            start_epoch_secs: epoch_secs_now(),
            seconds_offset: 0,
            lineno: 0,
            lineno_offset: 0,
            getopts_subindex: 0,
        };

        let _ = env.set_var("IFS", " \t\n");
        // $_ is a regular variable, exported by default, auto-updated by the executor.
        let _ = env.set_var("_", "");
        env.export_var("_");
        // OPTIND: getopts index, initialized to 1 per POSIX.
        let _ = env.set_var("OPTIND", "1");
        // PPID: parent process ID (readonly). POSIX required.
        #[cfg(unix)]
        {
            let ppid = nix::unistd::getppid().as_raw().to_string();
            let _ = env.set_var("PPID", &ppid);
            env.set_readonly("PPID");
        }
        env
    }

    /// Initialize Bash-specific static variables. Called when the executor is
    /// configured for Bash dialect.
    #[cfg(unix)]
    pub fn initialize_bash_vars(&mut self) {
        let hosttype = std::env::consts::ARCH;
        let ostype = match std::env::consts::OS {
            "linux" => "linux-gnu",
            "macos" => "darwin",
            "freebsd" => "freebsd",
            other => other,
        };
        let machtype = format!("{hosttype}-pc-{ostype}");

        // BASH_VERSION: compatibility version string.
        let _ = self.set_var("BASH_VERSION", "5.2.0(1)-release");

        // BASH_VERSINFO: readonly array with version components.
        let _ = self.set_array(
            "BASH_VERSINFO",
            vec![
                "5".to_string(),
                "2".to_string(),
                "0".to_string(),
                "1".to_string(),
                "release".to_string(),
                machtype.clone(),
            ],
        );
        self.set_readonly("BASH_VERSINFO");

        // UID / EUID: readonly integers.
        let _ = self.set_var("UID", &nix::unistd::getuid().as_raw().to_string());
        self.set_readonly("UID");
        let _ = self.set_var("EUID", &nix::unistd::geteuid().as_raw().to_string());
        self.set_readonly("EUID");

        // HOSTNAME
        let hostname = nix::unistd::gethostname()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _ = self.set_var("HOSTNAME", &hostname);

        // HOSTTYPE, OSTYPE, MACHTYPE
        let _ = self.set_var("HOSTTYPE", hosttype);
        let _ = self.set_var("OSTYPE", ostype);
        let _ = self.set_var("MACHTYPE", &machtype);

        // GROUPS: array of group IDs. Assign-silently-ignored (Category D).
        if let Ok(groups) = nix::unistd::getgroups() {
            let group_strs: Vec<String> = groups.iter().map(|g| g.as_raw().to_string()).collect();
            let _ = self.set_array("GROUPS", group_strs);
            self.special_active.insert("GROUPS".to_string());
        }
    }

    /// Non-unix stub.
    #[cfg(not(unix))]
    pub fn initialize_bash_vars(&mut self) {
        let _ = self.set_var("BASH_VERSION", "5.2.0(1)-release");
        let hosttype = std::env::consts::ARCH;
        let ostype = std::env::consts::OS;
        let machtype = format!("{hosttype}-pc-{ostype}");
        let _ = self.set_array(
            "BASH_VERSINFO",
            vec![
                "5".to_string(),
                "2".to_string(),
                "0".to_string(),
                "1".to_string(),
                "release".to_string(),
                machtype.clone(),
            ],
        );
        self.set_readonly("BASH_VERSINFO");
        let _ = self.set_var("HOSTTYPE", hosttype);
        let _ = self.set_var("OSTYPE", ostype);
        let _ = self.set_var("MACHTYPE", &machtype);
    }

    // Variable chain resolution -------------------------------------------------------------------------------------------

    /// Follow a chain of variable lookups with cycle detection.
    ///
    /// Starting from `start`, calls `follow(var)` at each step. If it returns
    /// `Some(next_name)`, follows to the next variable. If `None`, returns
    /// the current name. Stops on cycles (returns the last non-cyclic name).
    fn resolve_var_chain<'a, F>(&'a self, start: &'a str, follow: F) -> &'a str
    where
        F: Fn(&ShellVar) -> Option<&str>,
    {
        let mut seen = HashSet::new();
        seen.insert(start);
        let mut current = start;
        loop {
            match self.variables.get(current).and_then(&follow) {
                Some(target) => {
                    if !seen.insert(target) {
                        return current; // cycle detected
                    }
                    current = target;
                }
                None => return current,
            }
        }
    }

    /// Follow nameref chain to the ultimate target variable name.
    fn resolve_nameref<'a>(&'a self, name: &'a str) -> &'a str {
        self.resolve_var_chain(name, |v| v.nameref.as_deref())
    }

    /// Follow a chain where each variable's scalar value is treated as a
    /// variable name, stopping when the value is absent, empty, or not a
    /// valid identifier. Used by arithmetic evaluation.
    pub fn resolve_value_chain<'a>(&'a self, name: &'a str) -> &'a str {
        self.resolve_var_chain(name, |v| {
            let s = v.scalar_str();
            if s.is_empty()
                || s.as_bytes()
                    .first()
                    .is_none_or(|b| b.is_ascii_digit() || *b == b'-' || *b == b'+')
            {
                None
            } else {
                Some(s)
            }
        })
    }

    /// Create or update a nameref variable pointing to `target`.
    pub fn set_nameref(&mut self, name: &str, target: &str) -> Result<(), ExecError> {
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }
        self.variables.insert(
            name.to_string(),
            ShellVar {
                value: VarValue::Scalar(String::new()),
                exported: false,
                readonly: false,
                integer: false,
                lowercase: false,
                uppercase: false,
                nameref: Some(target.to_string()),
            },
        );
        Ok(())
    }

    // Scalar variable access ------------------------------------------------------------------------------------------

    /// Get the scalar value of a variable, or None if unset.
    ///
    /// For indexed arrays, returns element 0 (bash: `$a` == `${a[0]}`).
    pub fn get_var(&self, name: &str) -> Option<&str> {
        let name = self.resolve_nameref(name);
        self.variables.get(name).map(|v| v.scalar_str())
    }

    /// Set a variable's scalar value. Returns an error if the variable is readonly.
    ///
    /// If the variable is already an indexed array, sets element 0 (bash compat).
    /// Otherwise creates or overwrites a scalar variable.
    ///
    /// Applies variable attributes: integer (`-i`) evaluates the value as an
    /// arithmetic expression, lowercase (`-l`) / uppercase (`-u`) transform
    /// the case.
    pub fn set_var(&mut self, name: &str, value: &str) -> Result<(), ExecError> {
        let name = self.resolve_nameref(name).to_string();
        let name = name.as_str();
        // Dynamic variable intercept (RANDOM seeds, SECONDS reset, etc.).
        if let Some(result) = self.set_dynamic(name, value) {
            return result;
        }
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }

        let prev = self.variables.get(name);
        let exported = prev.map(|v| v.exported).unwrap_or(false);
        let integer = prev.map(|v| v.integer).unwrap_or(false);
        let lowercase = prev.map(|v| v.lowercase).unwrap_or(false);
        let uppercase = prev.map(|v| v.uppercase).unwrap_or(false);
        let is_indexed_array = prev
            .map(|v| matches!(v.value, VarValue::IndexedArray(_)))
            .unwrap_or(false);
        let is_assoc_array = prev
            .map(|v| matches!(v.value, VarValue::AssocArray(_)))
            .unwrap_or(false);

        let final_value = self.apply_var_transforms(value, integer, lowercase, uppercase);

        if is_indexed_array {
            if let Some(var) = self.variables.get_mut(name) {
                if let VarValue::IndexedArray(ref mut map) = var.value {
                    map.insert(0, final_value);
                }
            }
        } else if is_assoc_array {
            if let Some(var) = self.variables.get_mut(name) {
                if let VarValue::AssocArray(ref mut map) = var.value {
                    map.insert("0".to_string(), final_value);
                }
            }
        } else {
            self.variables.insert(
                name.to_string(),
                ShellVar {
                    value: VarValue::Scalar(final_value),
                    exported,
                    readonly: false,
                    integer,
                    lowercase,
                    uppercase,
                    nameref: None,
                },
            );
        }
        Ok(())
    }

    // Indexed array access --------------------------------------------------------------------------------------------

    /// Get a single element from an indexed array variable.
    pub fn get_array_element(&self, name: &str, index: usize) -> Option<&str> {
        let name = self.resolve_nameref(name);
        match self.variables.get(name)?.value {
            VarValue::Scalar(ref s) => {
                if index == 0 {
                    Some(s.as_str())
                } else {
                    None
                }
            }
            VarValue::IndexedArray(ref map) => map.get(&index).map(|s| s.as_str()),
            VarValue::AssocArray(ref map) => map.get(&index.to_string()).map(|s| s.as_str()),
        }
    }

    /// Set a single element in an indexed array variable.
    ///
    /// If the variable does not exist, creates a new indexed array.
    /// If it exists as a scalar, promotes it to an array (element 0 = old value).
    pub fn set_array_element(&mut self, name: &str, index: usize, value: &str) -> Result<(), ExecError> {
        let name = self.resolve_nameref(name).to_string();
        let name = name.as_str();
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }

        // Apply case transforms from existing variable attributes.
        let final_value = if let Some(var) = self.variables.get(name) {
            self.apply_var_transforms(value, false, var.lowercase, var.uppercase)
        } else {
            value.to_string()
        };

        match self.variables.get_mut(name) {
            Some(var) => match var.value {
                VarValue::IndexedArray(ref mut map) => {
                    map.insert(index, final_value);
                }
                VarValue::AssocArray(ref mut map) => {
                    map.insert(index.to_string(), final_value);
                }
                VarValue::Scalar(ref s) => {
                    let old = s.clone();
                    let mut map = BTreeMap::new();
                    map.insert(0, old);
                    map.insert(index, final_value);
                    var.value = VarValue::IndexedArray(map);
                }
            },
            None => {
                let mut map = BTreeMap::new();
                map.insert(index, final_value);
                self.variables.insert(
                    name.to_string(),
                    ShellVar {
                        value: VarValue::IndexedArray(map),
                        exported: false,
                        readonly: false,
                        integer: false,
                        lowercase: false,
                        uppercase: false,
                        nameref: None,
                    },
                );
            }
        }
        Ok(())
    }

    /// Set an entire indexed array from a list of values (indices 0..n).
    pub fn set_array(&mut self, name: &str, elements: Vec<String>) -> Result<(), ExecError> {
        let name = self.resolve_nameref(name).to_string();
        let name = name.as_str();
        // Dynamic variable intercept for array writes.
        if let Some(result) = self.set_dynamic(name, "") {
            return result;
        }
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }

        let prev = self.variables.get(name);
        let exported = prev.map(|v| v.exported).unwrap_or(false);
        let integer = prev.map(|v| v.integer).unwrap_or(false);
        let lowercase = prev.map(|v| v.lowercase).unwrap_or(false);
        let uppercase = prev.map(|v| v.uppercase).unwrap_or(false);

        let map: BTreeMap<usize, String> = elements.into_iter().enumerate().collect();
        self.variables.insert(
            name.to_string(),
            ShellVar {
                value: VarValue::IndexedArray(map),
                exported,
                readonly: false,
                integer,
                lowercase,
                uppercase,
                nameref: None,
            },
        );
        Ok(())
    }

    /// Get all elements of an array, in index order (indexed) or arbitrary order (assoc).
    pub fn get_array_all(&self, name: &str) -> Option<Vec<&str>> {
        let name = self.resolve_nameref(name);
        match self.variables.get(name)?.value {
            VarValue::Scalar(ref s) => Some(vec![s.as_str()]),
            VarValue::IndexedArray(ref map) => Some(map.values().map(|s| s.as_str()).collect()),
            VarValue::AssocArray(ref map) => Some(map.values().map(|s| s.as_str()).collect()),
        }
    }

    /// Get the number of set elements in an array.
    pub fn get_array_length(&self, name: &str) -> usize {
        let name = self.resolve_nameref(name);
        match self.variables.get(name) {
            None => 0,
            Some(var) => match var.value {
                VarValue::Scalar(_) => 1,
                VarValue::IndexedArray(ref map) => map.len(),
                VarValue::AssocArray(ref map) => map.len(),
            },
        }
    }

    /// Unset a single element of an indexed array.
    pub fn unset_array_element(&mut self, name: &str, index: usize) -> Result<(), ExecError> {
        let name = self.resolve_nameref(name).to_string();
        let name = name.as_str();
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }

        if let Some(var) = self.variables.get_mut(name) {
            match var.value {
                VarValue::IndexedArray(ref mut map) => {
                    map.remove(&index);
                }
                VarValue::AssocArray(ref mut map) => {
                    map.remove(&index.to_string());
                }
                VarValue::Scalar(_) => {
                    if index == 0 {
                        self.variables.remove(name);
                    }
                }
            }
        }
        Ok(())
    }

    // Associative array access ----------------------------------------------------------------------------------------

    /// Check if a variable is an associative array.
    pub fn is_assoc_array(&self, name: &str) -> bool {
        let name = self.resolve_nameref(name);
        self.variables
            .get(name)
            .map(|v| matches!(v.value, VarValue::AssocArray(_)))
            .unwrap_or(false)
    }

    /// Create an empty associative array variable, or reset an existing one.
    pub fn create_assoc(&mut self, name: &str) -> Result<(), ExecError> {
        let name = self.resolve_nameref(name).to_string();
        let name = name.as_str();
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }
        let prev = self.variables.get(name);
        let exported = prev.map(|v| v.exported).unwrap_or(false);
        let lowercase = prev.map(|v| v.lowercase).unwrap_or(false);
        let uppercase = prev.map(|v| v.uppercase).unwrap_or(false);
        self.variables.insert(
            name.to_string(),
            ShellVar {
                value: VarValue::AssocArray(HashMap::new()),
                exported,
                readonly: false,
                integer: false,
                lowercase,
                uppercase,
                nameref: None,
            },
        );
        Ok(())
    }

    /// Set a single element in an associative array variable.
    ///
    /// If the variable exists as an associative array, inserts the key-value pair.
    /// If it exists as a non-assoc variable, falls back to `set_var`.
    /// If it does not exist, creates a new associative array.
    pub fn set_assoc_element(&mut self, name: &str, key: &str, value: &str) -> Result<(), ExecError> {
        let name = self.resolve_nameref(name).to_string();
        let name = name.as_str();
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }

        // Apply case transforms from existing variable attributes.
        let final_value = if let Some(var) = self.variables.get(name) {
            self.apply_var_transforms(value, false, var.lowercase, var.uppercase)
        } else {
            value.to_string()
        };

        match self.variables.get_mut(name) {
            Some(var) => match var.value {
                VarValue::AssocArray(ref mut map) => {
                    map.insert(key.to_string(), final_value);
                }
                _ => {
                    // If not assoc, treat key as index 0 for scalar compat
                    return self.set_var(name, value);
                }
            },
            None => {
                let mut map = HashMap::new();
                map.insert(key.to_string(), final_value);
                self.variables.insert(
                    name.to_string(),
                    ShellVar {
                        value: VarValue::AssocArray(map),
                        exported: false,
                        readonly: false,
                        integer: false,
                        lowercase: false,
                        uppercase: false,
                        nameref: None,
                    },
                );
            }
        }
        Ok(())
    }

    /// Get a single element from an associative array by string key.
    ///
    /// For scalars, returns the value if the key is "0".
    pub fn get_assoc_element(&self, name: &str, key: &str) -> Option<&str> {
        let name = self.resolve_nameref(name);
        match self.variables.get(name)?.value {
            VarValue::AssocArray(ref map) => map.get(key).map(|s| s.as_str()),
            VarValue::Scalar(ref s) if key == "0" => Some(s.as_str()),
            _ => None,
        }
    }

    /// Unset a single element of an associative array by string key.
    pub fn unset_assoc_element(&mut self, name: &str, key: &str) -> Result<(), ExecError> {
        let name = self.resolve_nameref(name).to_string();
        let name = name.as_str();
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }
        if let Some(var) = self.variables.get_mut(name) {
            if let VarValue::AssocArray(ref mut map) = var.value {
                map.remove(key);
            }
        }
        Ok(())
    }

    // Combined element access (scalar + array subscript dispatch) -----------------------------------------------------

    /// Read a variable element by name, handling array subscripts.
    ///
    /// Handles `"a[0]"` (indexed), `"a[key]"` (assoc), `"a[@]"` / `"a[*]"` (all),
    /// and plain `"a"` (scalar).
    pub fn resolve_element(&self, name: &str) -> Option<String> {
        // NOTE: resolve_nameref is called by the individual accessors (get_var,
        // get_array_element, etc.), so we do NOT resolve at this level to avoid
        // double-resolution. Array subscript bases are resolved inside the
        // callees.
        if let Some((base, subscript)) = crate::exec::expand::parse_array_subscript(name) {
            match subscript {
                "@" | "*" => self.get_array_all(base).map(|v| v.join(" ")),
                _ if self.is_assoc_array(base) => self.get_assoc_element(base, subscript).map(|s| s.to_string()),
                _ => {
                    let index: usize = subscript.parse().unwrap_or(0);
                    self.get_array_element(base, index).map(|s| s.to_string())
                }
            }
        } else {
            self.get_var(name).map(|s| s.to_string())
        }
    }

    /// Write a variable element by name, handling array subscripts.
    ///
    /// Dispatches to `set_assoc_element`, `set_array_element`, or `set_var`
    /// based on the subscript and variable type.
    pub fn set_element(&mut self, name: &str, value: &str) -> Result<(), ExecError> {
        // NOTE: resolve_nameref is called by the individual mutators (set_var,
        // set_array_element, etc.), so we do NOT resolve at this level.
        if let Some((base, subscript)) = crate::exec::expand::parse_array_subscript(name) {
            if self.is_assoc_array(base) {
                self.set_assoc_element(base, subscript, value)
            } else {
                let index: usize = subscript.parse().unwrap_or(0);
                self.set_array_element(base, index, value)
            }
        } else {
            self.set_var(name, value)
        }
    }

    // Variable metadata -----------------------------------------------------------------------------------------------

    /// Mark a variable as exported. If it doesn't exist, create it with empty value.
    pub fn export_var(&mut self, name: &str) {
        match self.variables.get_mut(name) {
            Some(var) => var.exported = true,
            None => {
                self.variables.insert(
                    name.to_string(),
                    ShellVar {
                        value: VarValue::Scalar(String::new()),
                        exported: true,
                        readonly: false,
                        integer: false,
                        lowercase: false,
                        uppercase: false,
                        nameref: None,
                    },
                );
            }
        }
    }

    /// Remove the export attribute from a variable (no-op if variable doesn't exist).
    pub fn unexport_var(&mut self, name: &str) {
        if let Some(var) = self.variables.get_mut(name) {
            var.exported = false;
        }
    }

    /// Mark a variable as readonly. If it doesn't exist, create it with empty value.
    pub fn set_readonly(&mut self, name: &str) {
        match self.variables.get_mut(name) {
            Some(var) => var.readonly = true,
            None => {
                self.variables.insert(
                    name.to_string(),
                    ShellVar {
                        value: VarValue::Scalar(String::new()),
                        exported: false,
                        readonly: true,
                        integer: false,
                        lowercase: false,
                        uppercase: false,
                        nameref: None,
                    },
                );
            }
        }
    }

    /// Unset a variable (whole array or scalar). Returns an error if readonly.
    ///
    /// If `name` is a nameref, unsets the *target* variable (not the ref itself).
    pub fn unset_var(&mut self, name: &str) -> Result<(), ExecError> {
        let name = self.resolve_nameref(name).to_string();
        let name = name.as_str();
        // Dynamic variable intercept (Category A: kills special behavior).
        if let Some(result) = self.unset_dynamic(name) {
            return result;
        }
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }
        self.variables.remove(name);
        Ok(())
    }

    /// Returns whether a variable is readonly.
    pub fn is_readonly(&self, name: &str) -> bool {
        self.variables.get(name).map(|v| v.readonly).unwrap_or(false)
    }

    /// Returns whether a variable is exported.
    pub fn is_exported(&self, name: &str) -> bool {
        self.variables.get(name).map(|v| v.exported).unwrap_or(false)
    }

    /// Returns all exported variables as (name, value) pairs.
    ///
    /// For arrays, exports element 0 (matching bash behaviour).
    pub fn exported_vars(&self) -> Vec<(String, String)> {
        self.variables
            .iter()
            .filter(|(_, v)| v.exported)
            .map(|(k, v)| (k.clone(), v.scalar_str().to_string()))
            .collect()
    }

    /// Returns all readonly variables as (name, value) pairs.
    pub fn readonly_vars(&self) -> Vec<(String, String)> {
        self.variables
            .iter()
            .filter(|(_, v)| v.readonly)
            .map(|(k, v)| (k.clone(), v.scalar_str().to_string()))
            .collect()
    }

    /// Returns all variables as (name, value) pairs (scalar view).
    pub fn all_vars(&self) -> Vec<(String, String)> {
        self.variables
            .iter()
            .map(|(k, v)| (k.clone(), v.scalar_str().to_string()))
            .collect()
    }

    // Special parameters ----------------------------------------------------------------------------------------------

    /// Return the current IFS value, defaulting to space+tab+newline.
    pub fn get_ifs(&self) -> &str {
        self.get_var("IFS").unwrap_or(" \t\n")
    }

    /// Get a special parameter value (`$?`, `$#`, `$0`, `$$`, `$!`, `$@`, `$*`, `$-`).
    /// Returns `None` if the name is not a special parameter.
    pub fn get_special(&self, name: &str) -> Option<String> {
        match name {
            "?" => Some(self.last_exit_status.to_string()),
            "#" => Some(self.positional_params.len().to_string()),
            "0" => Some(self.program_name.clone()),
            "$" => Some(self.pid.to_string()),
            "!" => self.last_bg_pid.map(|p| p.to_string()),
            "@" | "*" => Some(self.positional_params.join(" ")),
            "-" => Some(self.option_flags_string()),
            _ => {
                if let Ok(n) = name.parse::<usize>() {
                    if n >= 1 {
                        return self.positional_params.get(n - 1).cloned();
                    }
                }
                None
            }
        }
    }

    /// Build the `$-` flags string from the currently enabled shell options.
    ///
    /// Each enabled option contributes a single letter, following bash conventions:
    /// `e` = errexit, `u` = nounset, `x` = xtrace, `B` = braceexpand (always on),
    /// `h` = hashall (always on for now).
    fn option_flags_string(&self) -> String {
        let mut flags = String::new();
        // Bash outputs flags in a specific order: himBHs (roughly alphabetical
        // with capitals after lowercase). We follow a similar convention.
        if self.errexit {
            flags.push('e');
        }
        // h (hashall) — we hash by default
        flags.push('h');
        if self.nounset {
            flags.push('u');
        }
        if self.xtrace {
            flags.push('x');
        }
        // B (braceexpand) — always on for now
        flags.push('B');
        flags
    }

    // Dynamic variables --------------------------------------------------------------------------------------------------

    /// Get a dynamic variable's computed value.
    ///
    /// Returns `Some(value)` for variables with active special behavior,
    /// `None` to fall through to regular variable lookup.
    pub fn get_dynamic(&mut self, name: &str) -> Option<String> {
        match name {
            "RANDOM" if self.special_active.contains("RANDOM") => {
                self.random_state = self.random_state.wrapping_mul(1103515245).wrapping_add(12345);
                Some(((self.random_state >> 16) & 0x7fff).to_string())
            }
            "SECONDS" if self.special_active.contains("SECONDS") => {
                let elapsed = epoch_secs_now().saturating_sub(self.start_epoch_secs) as i64;
                Some((elapsed + self.seconds_offset).to_string())
            }
            "EPOCHSECONDS" if self.special_active.contains("EPOCHSECONDS") => Some(epoch_secs_now().to_string()),
            "EPOCHREALTIME" if self.special_active.contains("EPOCHREALTIME") => {
                let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
                Some(format!("{}.{:06}", dur.as_secs(), dur.subsec_micros()))
            }
            "SRANDOM" if self.special_active.contains("SRANDOM") => {
                let mut buf = [0u8; 4];
                getrandom::fill(&mut buf).unwrap_or_default();
                Some(u32::from_ne_bytes(buf).to_string())
            }
            "BASHPID" if self.special_active.contains("BASHPID") => Some(std::process::id().to_string()),
            "LINENO" if self.special_active.contains("LINENO") => {
                Some(((self.lineno as isize + self.lineno_offset) as usize).to_string())
            }
            "SHELLOPTS" => {
                let mut opts = Vec::new();
                if self.errexit {
                    opts.push("errexit");
                }
                // hashall is always on
                opts.push("hashall");
                opts.push("interactive-comments");
                if self.nounset {
                    opts.push("nounset");
                }
                if self.xtrace {
                    opts.push("xtrace");
                }
                Some(opts.join(":"))
            }
            "BASHOPTS" => {
                let mut opts = Vec::new();
                if self.expand_aliases {
                    opts.push("expand_aliases");
                }
                Some(opts.join(":"))
            }
            _ => None,
        }
    }

    /// Intercept writes to dynamic variables.
    ///
    /// Returns `Some(Ok(()))` if the write was handled (caller should not store),
    /// `Some(Err(_))` if the write is forbidden, or `None` to fall through.
    pub fn set_dynamic(&mut self, name: &str, value: &str) -> Option<Result<(), ExecError>> {
        match name {
            "RANDOM" if self.special_active.contains("RANDOM") => {
                // Assignment seeds the RNG.
                self.random_state = value.parse::<u32>().unwrap_or(0);
                Some(Ok(()))
            }
            "SECONDS" if self.special_active.contains("SECONDS") => {
                // Assignment resets the timer.
                let assigned: i64 = value.parse().unwrap_or(0);
                let elapsed = epoch_secs_now().saturating_sub(self.start_epoch_secs) as i64;
                self.seconds_offset = assigned - elapsed;
                Some(Ok(()))
            }
            "EPOCHSECONDS" if self.special_active.contains("EPOCHSECONDS") => {
                // Assignment accepted but overridden on next read.
                Some(Ok(()))
            }
            "EPOCHREALTIME" if self.special_active.contains("EPOCHREALTIME") => Some(Ok(())),
            "SRANDOM" if self.special_active.contains("SRANDOM") => {
                // Assignment silently ignored.
                Some(Ok(()))
            }
            "BASHPID" if self.special_active.contains("BASHPID") => {
                // Assignment silently ignored.
                Some(Ok(()))
            }
            "GROUPS" if self.special_active.contains("GROUPS") => {
                // Assignment silently ignored (Category D).
                Some(Ok(()))
            }
            // Call stack variables: assign silently ignored (Category C2/D).
            "FUNCNAME" | "BASH_SOURCE" | "BASH_LINENO" if self.in_function_scope() => Some(Ok(())),
            "LINENO" if self.special_active.contains("LINENO") => {
                // Assignment offsets subsequent line numbers.
                let assigned: isize = value.parse().unwrap_or(0);
                self.lineno_offset = assigned - self.lineno as isize;
                Some(Ok(()))
            }
            // Category C: readonly, cannot assign.
            "SHELLOPTS" | "BASHOPTS" => Some(Err(ExecError::ReadonlyVariable(name.to_string()))),
            // When OPTIND is set to 1, reset the getopts sub-index.
            "OPTIND" => {
                if value == "1" {
                    self.getopts_subindex = 0;
                }
                None // fall through to normal set_var
            }
            _ => None,
        }
    }

    /// Intercept unset of dynamic variables.
    ///
    /// Returns `Some(Ok(()))` if handled (Category A: kills special behavior),
    /// `Some(Err(_))` if unset is forbidden, or `None` to fall through.
    pub fn unset_dynamic(&mut self, name: &str) -> Option<Result<(), ExecError>> {
        match name {
            // Category C: cannot unset.
            "SHELLOPTS" | "BASHOPTS" => Some(Err(ExecError::ReadonlyVariable(name.to_string()))),
            // Category C2: cannot unset.
            "BASH_SOURCE" | "BASH_LINENO" => Some(Err(ExecError::ReadonlyVariable(name.to_string()))),
            // Category A: unset kills special behavior forever.
            "RANDOM" | "SECONDS" | "EPOCHSECONDS" | "EPOCHREALTIME" | "SRANDOM" | "LINENO" => {
                self.special_active.remove(name);
                // Also remove from the regular variable store if present.
                self.variables.remove(name);
                Some(Ok(()))
            }
            // Category D: unset works, removes variable.
            "BASHPID" => {
                self.special_active.remove("BASHPID");
                Some(Ok(()))
            }
            "GROUPS" if self.special_active.contains("GROUPS") => {
                self.special_active.remove("GROUPS");
                self.variables.remove("GROUPS");
                Some(Ok(()))
            }
            _ => None,
        }
    }

    /// Update the current line number (called by the executor).
    pub fn set_lineno(&mut self, lineno: usize) {
        self.lineno = lineno;
    }

    /// Get the getopts sub-index (position within a grouped option string like `-abc`).
    pub fn getopts_subindex(&self) -> usize {
        self.getopts_subindex
    }

    /// Set the getopts sub-index.
    pub fn set_getopts_subindex(&mut self, idx: usize) {
        self.getopts_subindex = idx;
    }

    /// Returns `$1`, `$2`, ... as a slice (0-indexed: `[0]` is `$1`).
    pub fn positional_params(&self) -> &[String] {
        &self.positional_params
    }

    /// Replace all positional parameters. Used by the `set` and `shift` builtins.
    pub fn set_positional_params(&mut self, params: Vec<String>) {
        self.positional_params = params;
    }

    /// Update `$_` (last argument of previous simple command).
    ///
    /// The executor calls this after each simple command with the last
    /// expanded argument (or the command name if there were no arguments).
    pub fn set_last_arg(&mut self, arg: &str) {
        // $_  is stored as a regular variable so it appears in `declare -p`.
        // We bypass readonly checks — the shell always updates $_.
        if let Some(var) = self.variables.get_mut("_") {
            var.value = VarValue::Scalar(arg.to_string());
        } else {
            let _ = self.set_var("_", arg);
        }
    }

    /// Returns `$0` (the shell or script name).
    pub fn program_name(&self) -> &str {
        &self.program_name
    }

    /// Override `$0`. Called when the shell is invoked with an explicit script name.
    pub fn set_program_name(&mut self, name: String) {
        self.program_name = name;
    }

    // Exit status -----------------------------------------------------------------------------------------------------

    /// Returns `$?` (the exit status of the last completed command).
    pub fn last_exit_status(&self) -> i32 {
        self.last_exit_status
    }

    /// Update `$?`. Called by the executor after each command completes.
    pub fn set_last_exit_status(&mut self, status: i32) {
        self.last_exit_status = status;
    }

    // Working directory -----------------------------------------------------------------------------------------------

    /// Returns the current working directory.
    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Change the working directory. Canonicalizes the path and verifies it exists.
    pub fn set_cwd(&mut self, path: PathBuf) -> Result<(), ExecError> {
        if !path.is_dir() {
            return Err(ExecError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("cd: {}: No such file or directory", path.display()),
            )));
        }
        let canonical = path.canonicalize().map_err(ExecError::Io)?;
        self.cwd = canonical;
        Ok(())
    }

    // Functions -------------------------------------------------------------------------------------------------------

    /// Look up a shell function by name.
    pub fn get_function(&self, name: &str) -> Option<&StoredFunction> {
        self.functions.get(name)
    }

    /// Register a shell function. Overwrites any previous definition with the same name.
    pub fn set_function(&mut self, name: String, func: StoredFunction) {
        self.functions.insert(name, func);
    }

    /// Return the names of all defined functions.
    pub fn function_names(&self) -> Vec<&str> {
        self.functions.keys().map(|k| k.as_str()).collect()
    }

    // Scope management (for function calls) ---------------------------------------------------------------------------

    /// Enter a function scope: saves positional parameters and pushes a new scope frame.
    ///
    /// `local` declarations within the scope are tracked and restored on `pop_scope`.
    #[debug_ensures(self.scope_stack.len() == old(self.scope_stack.len()) + 1)]
    pub fn push_scope(&mut self, new_positional: Vec<String>) {
        self.push_scope_with_info(new_positional, CallInfo::default());
    }

    /// Enter a function scope with call metadata for FUNCNAME/BASH_SOURCE/BASH_LINENO.
    #[debug_ensures(self.scope_stack.len() == old(self.scope_stack.len()) + 1)]
    pub fn push_scope_with_info(&mut self, new_positional: Vec<String>, info: CallInfo) {
        let saved_positional = std::mem::replace(&mut self.positional_params, new_positional);
        self.scope_stack.push(Scope {
            saved: HashMap::new(),
            saved_positional,
            function_name: info.function_name,
            source_file: info.source_file,
            call_lineno: info.call_lineno,
        });
        self.rebuild_call_stack_vars();
    }

    /// Leave a function scope: restores positional parameters and all `local` variables.
    #[debug_ensures(self.scope_stack.len() == old(self.scope_stack.len()) - 1)]
    pub fn pop_scope(&mut self) {
        if let Some(scope) = self.scope_stack.pop() {
            self.positional_params = scope.saved_positional;
            for (name, saved) in scope.saved {
                match saved {
                    Some(var) => {
                        self.variables.insert(name, var);
                    }
                    None => {
                        self.variables.remove(&name);
                    }
                }
            }
            self.rebuild_call_stack_vars();
        }
    }

    /// Rebuild FUNCNAME, BASH_SOURCE, and BASH_LINENO arrays from the scope stack.
    fn rebuild_call_stack_vars(&mut self) {
        let depth = self.scope_stack.len();
        if depth == 0 {
            // Outside functions — clear the arrays.
            self.variables.remove("FUNCNAME");
            self.variables.remove("BASH_SOURCE");
            self.variables.remove("BASH_LINENO");
            return;
        }

        // Build arrays from innermost (top of stack) to outermost.
        let mut funcnames = Vec::with_capacity(depth + 1);
        let mut sources = Vec::with_capacity(depth + 1);
        let mut linenos = Vec::with_capacity(depth);

        for scope in self.scope_stack.iter().rev() {
            funcnames.push(scope.function_name.clone());
            sources.push(scope.source_file.clone());
            linenos.push(scope.call_lineno.to_string());
        }
        // Bottom of the stack is "main".
        funcnames.push("main".to_string());
        sources.push(String::new());

        // Store as arrays, bypassing readonly/dynamic checks by writing directly.
        let make_array = |elems: Vec<String>| {
            let map: std::collections::BTreeMap<usize, String> = elems.into_iter().enumerate().collect();
            ShellVar {
                value: VarValue::IndexedArray(map),
                exported: false,
                readonly: false,
                integer: false,
                lowercase: false,
                uppercase: false,
                nameref: None,
            }
        };
        self.variables.insert("FUNCNAME".to_string(), make_array(funcnames));
        self.variables.insert("BASH_SOURCE".to_string(), make_array(sources));
        self.variables.insert("BASH_LINENO".to_string(), make_array(linenos));
    }

    /// Returns whether execution is inside a function call (scope stack is non-empty).
    pub fn in_function_scope(&self) -> bool {
        !self.scope_stack.is_empty()
    }

    /// Mark a variable as function-local (Bash `local` builtin).
    ///
    /// Saves the variable's current state so `pop_scope` can restore it. Only the
    /// first `local` call per name per scope saves state -- subsequent calls are no-ops.
    /// Returns an error if called outside a function scope.
    pub fn declare_local(&mut self, name: &str) -> Result<(), ExecError> {
        let scope = self
            .scope_stack
            .last_mut()
            .ok_or_else(|| ExecError::BadSubstitution("local: can only be used in a function".to_string()))?;

        if !scope.saved.contains_key(name) {
            let current = self.variables.get(name).cloned();
            scope.saved.insert(name.to_string(), current.clone());

            let value = current
                .as_ref()
                .map(|v| v.value.clone())
                .unwrap_or(VarValue::Scalar(String::new()));
            let exported = current.as_ref().map(|v| v.exported).unwrap_or(false);
            let integer = current.as_ref().map(|v| v.integer).unwrap_or(false);
            let lowercase = current.as_ref().map(|v| v.lowercase).unwrap_or(false);
            let uppercase = current.as_ref().map(|v| v.uppercase).unwrap_or(false);
            let nameref = current.as_ref().and_then(|v| v.nameref.clone());
            self.variables.insert(
                name.to_string(),
                ShellVar {
                    value,
                    exported,
                    readonly: false,
                    integer,
                    lowercase,
                    uppercase,
                    nameref,
                },
            );
        }

        Ok(())
    }

    // Variable attribute queries --------------------------------------------------------------------------------------

    /// Returns whether a variable has the integer (`-i`) attribute.
    pub fn has_integer_attr(&self, name: &str) -> bool {
        self.variables.get(name).map(|v| v.integer).unwrap_or(false)
    }

    /// Return the `declare`-style attribute flag string for a variable.
    ///
    /// Flag letters follow bash ordering: `A` (assoc), `a` (indexed),
    /// `i` (integer), `l` (lowercase), `n` (nameref), `r` (readonly),
    /// `u` (uppercase), `x` (exported).
    pub fn get_var_attributes(&self, name: &str) -> String {
        let name = self.resolve_nameref(name);
        match self.variables.get(name) {
            Some(var) => {
                let mut flags = String::new();
                if matches!(var.value, VarValue::AssocArray(_)) {
                    flags.push('A');
                }
                if matches!(var.value, VarValue::IndexedArray(_)) {
                    flags.push('a');
                }
                if var.integer {
                    flags.push('i');
                }
                if var.lowercase {
                    flags.push('l');
                }
                if var.nameref.is_some() {
                    flags.push('n');
                }
                if var.readonly {
                    flags.push('r');
                }
                if var.uppercase {
                    flags.push('u');
                }
                if var.exported {
                    flags.push('x');
                }
                flags
            }
            None => String::new(),
        }
    }

    /// Return the keys of an array variable, or `None` for scalars/unset.
    ///
    /// For indexed arrays, keys are stringified indices in sorted order.
    /// For associative arrays, keys are in arbitrary order.
    pub fn get_array_keys(&self, name: &str) -> Option<Vec<String>> {
        let name = self.resolve_nameref(name);
        match &self.variables.get(name)?.value {
            VarValue::IndexedArray(map) => Some(map.keys().map(|k| k.to_string()).collect()),
            VarValue::AssocArray(map) => Some(map.keys().cloned().collect()),
            VarValue::Scalar(_) => None,
        }
    }

    /// Return all key-value pairs from an array (or `("0", value)` for scalars).
    ///
    /// Indexed arrays iterate in key order (BTreeMap). Associative arrays
    /// iterate in arbitrary order (HashMap).
    pub fn get_array_key_value_pairs(&self, name: &str) -> Option<Vec<(String, String)>> {
        let name = self.resolve_nameref(name);
        match &self.variables.get(name)?.value {
            VarValue::IndexedArray(map) => Some(map.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()),
            VarValue::AssocArray(map) => Some(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            VarValue::Scalar(s) => Some(vec![("0".to_string(), s.clone())]),
        }
    }

    /// Declare a variable with attributes from `declare`/`typeset`.
    ///
    /// If in a function scope and not `global`, the variable is made local
    /// first.  Then attributes and an optional initial value are applied.
    pub fn declare_with_attrs(
        &mut self,
        name: &str,
        value: Option<&str>,
        attrs: &DeclareAttrs,
    ) -> Result<(), ExecError> {
        // If in function scope and not global, declare local first.
        if self.in_function_scope() && !attrs.global {
            self.declare_local(name)?;
        }

        if let Some(var) = self.variables.get_mut(name) {
            // Apply attribute flags to existing variable.
            if attrs.readonly_set {
                var.readonly = true;
            }
            if attrs.exported_set {
                var.exported = true;
            }
            if attrs.integer_set {
                var.integer = true;
            }
            if attrs.lowercase_set {
                var.lowercase = true;
                var.uppercase = false;
            }
            if attrs.uppercase_set {
                var.uppercase = true;
                var.lowercase = false;
            }
            // Apply attribute removal flags (+X).
            if attrs.unexport {
                var.exported = false;
            }
            if attrs.uninteger {
                var.integer = false;
            }
            if attrs.unlowercase {
                var.lowercase = false;
            }
            if attrs.unuppercase {
                var.uppercase = false;
            }
            if attrs.unreadonly && self.typeset_can_unset_readonly {
                var.readonly = false;
            }
        } else {
            // Variable does not exist yet — create it.
            let initial = value.unwrap_or("").to_string();
            let final_value =
                self.apply_var_transforms(&initial, attrs.integer_set, attrs.lowercase_set, attrs.uppercase_set);
            self.variables.insert(
                name.to_string(),
                ShellVar {
                    value: VarValue::Scalar(final_value),
                    exported: attrs.exported_set,
                    readonly: attrs.readonly_set,
                    integer: attrs.integer_set,
                    lowercase: attrs.lowercase_set && !attrs.uppercase_set,
                    uppercase: attrs.uppercase_set,
                    nameref: None,
                },
            );
            return Ok(());
        }

        // Set value if provided (attributes are already applied above).
        if let Some(val) = value {
            self.set_var(name, val)?;
        }

        Ok(())
    }

    /// Apply integer / lowercase / uppercase transforms to a value string.
    ///
    /// For the integer attribute, evaluates the string as an arithmetic
    /// expression using a simple recursive parser that handles `+`, `-`,
    /// `*`, `/`, and variable references.  Full arithmetic evaluation
    /// (matching bash's `$((...))` semantics) requires the Executor, so
    /// complex expressions like nested ternaries are not supported here.
    ///
    /// Case transforms use ICU4X locale-aware conversion, respecting
    /// `LC_CTYPE` / `LC_ALL` / `LANG` environment variables.
    fn apply_var_transforms(&self, value: &str, _integer: bool, lowercase: bool, uppercase: bool) -> String {
        // Note: the integer attribute is handled by the Executor, which has
        // access to the full arithmetic evaluator.  Environment only applies
        // case transforms.
        if lowercase {
            let locale = super::locale::ctype_locale(self);
            super::locale::to_lowercase(value, &locale)
        } else if uppercase {
            let locale = super::locale::ctype_locale(self);
            super::locale::to_uppercase(value, &locale)
        } else {
            value.to_string()
        }
    }

    /// Returns the current `$IFS` value, defaulting to `" \t\n"` if unset.
    pub fn ifs(&self) -> &str {
        self.get_var("IFS").unwrap_or(" \t\n")
    }

    // Aliases ---------------------------------------------------------------------------------------------------------

    /// Define an alias. Takes effect immediately in the alias table.
    pub fn define_alias(&mut self, name: &str, value: &str) {
        self.aliases.insert(name.to_string(), value.to_string());
    }

    /// Remove an alias. Returns true if it existed.
    pub fn unalias(&mut self, name: &str) -> bool {
        self.aliases.remove(name).is_some()
    }

    /// Remove all aliases.
    pub fn unalias_all(&mut self) {
        self.aliases.clear();
    }

    /// Look up an alias by name.
    pub fn get_alias(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(|s| s.as_str())
    }

    /// Return a snapshot of the current alias table.
    pub fn alias_snapshot(&self) -> HashMap<String, String> {
        self.aliases.clone()
    }

    /// Return a reference to the alias table (for listing).
    pub fn aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }

    /// Whether alias expansion is enabled.
    pub fn expand_aliases_enabled(&self) -> bool {
        self.expand_aliases
    }

    /// Enable or disable alias expansion.
    pub fn set_expand_aliases(&mut self, enabled: bool) {
        self.expand_aliases = enabled;
    }

    /// Whether `set -e` (errexit) is enabled.
    pub fn errexit_enabled(&self) -> bool {
        self.errexit
    }

    /// Enable or disable `set -e` (errexit).
    pub fn set_errexit(&mut self, enabled: bool) {
        self.errexit = enabled;
    }

    /// Whether `set -u` (nounset) is enabled.
    pub fn nounset_enabled(&self) -> bool {
        self.nounset
    }

    /// Enable or disable `set -u` (nounset).
    pub fn set_nounset(&mut self, enabled: bool) {
        self.nounset = enabled;
    }

    /// Whether `set -x` (xtrace) is enabled.
    pub fn xtrace_enabled(&self) -> bool {
        self.xtrace
    }

    /// Enable or disable `set -x` (xtrace).
    pub fn set_xtrace(&mut self, enabled: bool) {
        self.xtrace = enabled;
    }

    /// Whether the bash 4.x array empty-element alternative bug is active.
    pub fn array_empty_element_alternative_bug(&self) -> bool {
        self.array_empty_element_alternative_bug
    }

    /// Set the bash 4.x array empty-element alternative bug flag.
    pub fn set_array_empty_element_alternative_bug(&mut self, enabled: bool) {
        self.array_empty_element_alternative_bug = enabled;
    }

    /// Whether `typeset +r` / `declare +r` removes the readonly attribute.
    pub fn typeset_can_unset_readonly(&self) -> bool {
        self.typeset_can_unset_readonly
    }

    /// Set the `typeset_can_unset_readonly` flag.
    pub fn set_typeset_can_unset_readonly(&mut self, enabled: bool) {
        self.typeset_can_unset_readonly = enabled;
    }

    // Attribute removal (declare +X) ----------------------------------------------------------------------------------

    /// Remove the exported attribute from a variable.
    pub fn unset_exported(&mut self, name: &str) {
        let name = self.resolve_nameref(name).to_string();
        if let Some(var) = self.variables.get_mut(name.as_str()) {
            var.exported = false;
        }
    }

    /// Remove the integer attribute from a variable.
    pub fn unset_integer(&mut self, name: &str) {
        let name = self.resolve_nameref(name).to_string();
        if let Some(var) = self.variables.get_mut(name.as_str()) {
            var.integer = false;
        }
    }

    /// Remove the lowercase attribute from a variable.
    pub fn unset_lowercase(&mut self, name: &str) {
        let name = self.resolve_nameref(name).to_string();
        if let Some(var) = self.variables.get_mut(name.as_str()) {
            var.lowercase = false;
        }
    }

    /// Remove the uppercase attribute from a variable.
    pub fn unset_uppercase(&mut self, name: &str) {
        let name = self.resolve_nameref(name).to_string();
        if let Some(var) = self.variables.get_mut(name.as_str()) {
            var.uppercase = false;
        }
    }

    /// Remove the readonly attribute. Only effective when `typeset_can_unset_readonly`
    /// is enabled (Oils behavior); callers must gate on that flag.
    pub fn unset_readonly(&mut self, name: &str) {
        let name = self.resolve_nameref(name).to_string();
        if let Some(var) = self.variables.get_mut(name.as_str()) {
            var.readonly = false;
        }
    }

    /// Import all environment variables from the current OS process, marking them exported.
    pub fn inherit_from_process(&mut self) {
        for (key, value) in std::env::vars() {
            self.variables.insert(
                key,
                ShellVar {
                    value: VarValue::Scalar(value),
                    exported: true,
                    readonly: false,
                    integer: false,
                    lowercase: false,
                    uppercase: false,
                    nameref: None,
                },
            );
        }
    }

    /// Serialize the environment for cross-process transfer (subshells).
    ///
    /// Captures all variables, functions, aliases, positional params, and shell
    /// options.  FDs and scope stack are NOT serialized.
    pub(crate) fn serialize(&self) -> SerializedEnvironment {
        SerializedEnvironment {
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            positional_params: self.positional_params.clone(),
            program_name: self.program_name.clone(),
            last_exit_status: self.last_exit_status,
            aliases: self.aliases.clone(),
            expand_aliases: self.expand_aliases,
            errexit: self.errexit,
            nounset: self.nounset,
            xtrace: self.xtrace,
            cwd: self.cwd.clone(),
            array_empty_element_alternative_bug: self.array_empty_element_alternative_bug,
            typeset_can_unset_readonly: self.typeset_can_unset_readonly,
            special_active: self.special_active.clone(),
        }
    }

    /// Reconstruct an environment from serialized state.
    ///
    /// Used by the `exec-ast` child process. The scope stack and FD table are
    /// not transferred -- the child starts with a fresh PID and no open FDs.
    pub fn from_serialized(s: SerializedEnvironment) -> Self {
        Environment {
            variables: s.variables,
            functions: s.functions,
            positional_params: s.positional_params,
            program_name: s.program_name,
            last_exit_status: s.last_exit_status,
            last_bg_pid: None,
            cwd: s.cwd,
            pid: std::process::id(),
            scope_stack: Vec::new(),
            aliases: s.aliases,
            expand_aliases: s.expand_aliases,
            errexit: s.errexit,
            nounset: s.nounset,
            xtrace: s.xtrace,
            array_empty_element_alternative_bug: s.array_empty_element_alternative_bug,
            typeset_can_unset_readonly: s.typeset_can_unset_readonly,
            special_active: s.special_active,
            random_state: std::process::id() ^ epoch_secs_now() as u32,
            start_epoch_secs: epoch_secs_now(),
            seconds_offset: 0,
            lineno: 0,
            lineno_offset: 0,
            getopts_subindex: 0,
        }
    }
}

/// Serializable subset of `Environment` for cross-process transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEnvironment {
    pub variables: HashMap<String, ShellVar>,
    pub functions: HashMap<String, StoredFunction>,
    pub positional_params: Vec<String>,
    pub program_name: String,
    pub last_exit_status: i32,
    pub aliases: HashMap<String, String>,
    pub expand_aliases: bool,
    pub errexit: bool,
    pub nounset: bool,
    pub xtrace: bool,
    pub cwd: PathBuf,
    #[serde(default)]
    pub array_empty_element_alternative_bug: bool,
    #[serde(default)]
    pub typeset_can_unset_readonly: bool,
    #[serde(default = "default_special_active")]
    pub special_active: HashSet<String>,
}

fn default_special_active() -> HashSet<String> {
    [
        "RANDOM",
        "SECONDS",
        "EPOCHSECONDS",
        "EPOCHREALTIME",
        "SRANDOM",
        "LINENO",
        "BASHPID",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Current Unix epoch time in seconds.
fn epoch_secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

// Integer attribute arithmetic evaluation is handled by the Executor,
// which has access to the full arithmetic module (src/exec/arithmetic.rs).

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;

// NOTE: The mini arithmetic evaluator that was here has been removed.
// Integer attribute (-i) evaluation is handled by the Executor, which
// has access to the full arithmetic module (src/exec/arithmetic.rs).
//
// For now, set_var() with integer=true does a simple i64::parse().
// Full arithmetic expression evaluation (e.g. "2+3") requires the
// Executor to intercept the assignment and evaluate before calling
// set_var(). This is a TODO tracked in the declare -i tests.

// REMOVED: ArithToken enum, tokenize_arith(), parse_additive(),
// parse_multiplicative(), parse_unary(), parse_primary() — ~200 lines
// of duplicated arithmetic parsing that belonged in exec/arithmetic.rs.
//
// DO NOT add arithmetic evaluation to Environment. Use the Executor.
// (mini parser removed — was ~200 lines of duplicated arithmetic code)
