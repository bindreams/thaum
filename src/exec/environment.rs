use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use contracts::debug_ensures;

use crate::ast::{CompoundCommand, FunctionDef};
use crate::exec::error::ExecError;

/// The value of a shell variable — scalar or indexed array.
#[derive(Debug, Clone)]
pub(crate) enum VarValue {
    /// A single string value (POSIX).
    Scalar(String),
    /// A sparse indexed array (Bash).  Uses `BTreeMap` so iteration is in
    /// index order, matching bash's `${a[@]}` behaviour.
    IndexedArray(BTreeMap<usize, String>),
}

/// A shell variable with metadata.
#[derive(Debug, Clone)]
struct ShellVar {
    value: VarValue,
    exported: bool,
    readonly: bool,
}

impl ShellVar {
    /// Return the scalar string for this variable.
    ///
    /// For scalars, returns the value directly. For indexed arrays, returns
    /// element 0 (bash compatibility: `$a` == `${a[0]}`).
    fn scalar_str(&self) -> &str {
        match &self.value {
            VarValue::Scalar(s) => s,
            VarValue::IndexedArray(map) => map.get(&0).map(|s| s.as_str()).unwrap_or(""),
        }
    }
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
}

/// A stored function definition (just the parts we need for execution).
#[derive(Debug, Clone)]
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
        };

        let _ = env.set_var("IFS", " \t\n");
        env
    }

    // --- Scalar variable access ---

    /// Get the scalar value of a variable, or None if unset.
    ///
    /// For indexed arrays, returns element 0 (bash: `$a` == `${a[0]}`).
    pub fn get_var(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(|v| v.scalar_str())
    }

    /// Set a variable's scalar value. Returns an error if the variable is readonly.
    ///
    /// If the variable is already an indexed array, sets element 0 (bash compat).
    /// Otherwise creates or overwrites a scalar variable.
    pub fn set_var(&mut self, name: &str, value: &str) -> Result<(), ExecError> {
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }

        let prev = self.variables.get(name);
        let exported = prev.map(|v| v.exported).unwrap_or(false);
        let is_array = prev
            .map(|v| matches!(v.value, VarValue::IndexedArray(_)))
            .unwrap_or(false);

        if is_array {
            if let Some(var) = self.variables.get_mut(name) {
                if let VarValue::IndexedArray(ref mut map) = var.value {
                    map.insert(0, value.to_string());
                }
            }
        } else {
            self.variables.insert(
                name.to_string(),
                ShellVar {
                    value: VarValue::Scalar(value.to_string()),
                    exported,
                    readonly: false,
                },
            );
        }
        Ok(())
    }

    // --- Indexed array access ---

    /// Get a single element from an indexed array variable.
    pub fn get_array_element(&self, name: &str, index: usize) -> Option<&str> {
        match self.variables.get(name)?.value {
            VarValue::Scalar(ref s) => {
                if index == 0 {
                    Some(s.as_str())
                } else {
                    None
                }
            }
            VarValue::IndexedArray(ref map) => map.get(&index).map(|s| s.as_str()),
        }
    }

    /// Set a single element in an indexed array variable.
    ///
    /// If the variable does not exist, creates a new indexed array.
    /// If it exists as a scalar, promotes it to an array (element 0 = old value).
    pub fn set_array_element(
        &mut self,
        name: &str,
        index: usize,
        value: &str,
    ) -> Result<(), ExecError> {
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }

        match self.variables.get_mut(name) {
            Some(var) => match var.value {
                VarValue::IndexedArray(ref mut map) => {
                    map.insert(index, value.to_string());
                }
                VarValue::Scalar(ref s) => {
                    let old = s.clone();
                    let mut map = BTreeMap::new();
                    map.insert(0, old);
                    map.insert(index, value.to_string());
                    var.value = VarValue::IndexedArray(map);
                }
            },
            None => {
                let mut map = BTreeMap::new();
                map.insert(index, value.to_string());
                self.variables.insert(
                    name.to_string(),
                    ShellVar {
                        value: VarValue::IndexedArray(map),
                        exported: false,
                        readonly: false,
                    },
                );
            }
        }
        Ok(())
    }

    /// Set an entire indexed array from a list of values (indices 0..n).
    pub fn set_array(&mut self, name: &str, elements: Vec<String>) -> Result<(), ExecError> {
        if let Some(existing) = self.variables.get(name) {
            if existing.readonly {
                return Err(ExecError::ReadonlyVariable(name.to_string()));
            }
        }

        let exported = self
            .variables
            .get(name)
            .map(|v| v.exported)
            .unwrap_or(false);

        let map: BTreeMap<usize, String> = elements.into_iter().enumerate().collect();
        self.variables.insert(
            name.to_string(),
            ShellVar {
                value: VarValue::IndexedArray(map),
                exported,
                readonly: false,
            },
        );
        Ok(())
    }

    /// Get all elements of an indexed array, in index order.
    pub fn get_array_all(&self, name: &str) -> Option<Vec<&str>> {
        match self.variables.get(name)?.value {
            VarValue::Scalar(ref s) => Some(vec![s.as_str()]),
            VarValue::IndexedArray(ref map) => Some(map.values().map(|s| s.as_str()).collect()),
        }
    }

    /// Get the number of set elements in an indexed array.
    pub fn get_array_length(&self, name: &str) -> usize {
        match self.variables.get(name) {
            None => 0,
            Some(var) => match var.value {
                VarValue::Scalar(_) => 1,
                VarValue::IndexedArray(ref map) => map.len(),
            },
        }
    }

    /// Unset a single element of an indexed array.
    pub fn unset_array_element(&mut self, name: &str, index: usize) -> Result<(), ExecError> {
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
                VarValue::Scalar(_) => {
                    if index == 0 {
                        self.variables.remove(name);
                    }
                }
            }
        }
        Ok(())
    }

    // --- Variable metadata ---

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
                    },
                );
            }
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
                    },
                );
            }
        }
    }

    /// Unset a variable (whole array or scalar). Returns an error if readonly.
    pub fn unset_var(&mut self, name: &str) -> Result<(), ExecError> {
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
        self.variables
            .get(name)
            .map(|v| v.readonly)
            .unwrap_or(false)
    }

    /// Returns whether a variable is exported.
    pub fn is_exported(&self, name: &str) -> bool {
        self.variables
            .get(name)
            .map(|v| v.exported)
            .unwrap_or(false)
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

    /// Returns all variables as (name, value) pairs (scalar view).
    pub fn all_vars(&self) -> Vec<(String, String)> {
        self.variables
            .iter()
            .map(|(k, v)| (k.clone(), v.scalar_str().to_string()))
            .collect()
    }

    // --- Special parameters ---

    /// Get a special parameter value: `$?`, `$#`, `$0`, `$$`, `$!`, `$@`, `$*`.
    /// Returns None if the name is not a special parameter.
    pub fn get_special(&self, name: &str) -> Option<String> {
        match name {
            "?" => Some(self.last_exit_status.to_string()),
            "#" => Some(self.positional_params.len().to_string()),
            "0" => Some(self.program_name.clone()),
            "$" => Some(self.pid.to_string()),
            "!" => self.last_bg_pid.map(|p| p.to_string()),
            "@" | "*" => Some(self.positional_params.join(" ")),
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

    pub fn positional_params(&self) -> &[String] {
        &self.positional_params
    }

    pub fn set_positional_params(&mut self, params: Vec<String>) {
        self.positional_params = params;
    }

    pub fn program_name(&self) -> &str {
        &self.program_name
    }

    pub fn set_program_name(&mut self, name: String) {
        self.program_name = name;
    }

    // --- Exit status ---

    pub fn last_exit_status(&self) -> i32 {
        self.last_exit_status
    }

    pub fn set_last_exit_status(&mut self, status: i32) {
        self.last_exit_status = status;
    }

    // --- Working directory ---

    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

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

    // --- Functions ---

    pub fn get_function(&self, name: &str) -> Option<&StoredFunction> {
        self.functions.get(name)
    }

    pub fn set_function(&mut self, name: String, func: StoredFunction) {
        self.functions.insert(name, func);
    }

    // --- Scope management (for function calls) ---

    #[debug_ensures(self.scope_stack.len() == old(self.scope_stack.len()) + 1)]
    pub fn push_scope(&mut self, new_positional: Vec<String>) {
        let saved_positional = std::mem::replace(&mut self.positional_params, new_positional);
        self.scope_stack.push(Scope {
            saved: HashMap::new(),
            saved_positional,
        });
    }

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
        }
    }

    pub fn in_function_scope(&self) -> bool {
        !self.scope_stack.is_empty()
    }

    pub fn declare_local(&mut self, name: &str) -> Result<(), ExecError> {
        let scope = self.scope_stack.last_mut().ok_or_else(|| {
            ExecError::BadSubstitution("local: can only be used in a function".to_string())
        })?;

        if !scope.saved.contains_key(name) {
            let current = self.variables.get(name).cloned();
            scope.saved.insert(name.to_string(), current.clone());

            let value = current
                .as_ref()
                .map(|v| v.value.clone())
                .unwrap_or(VarValue::Scalar(String::new()));
            let exported = current.as_ref().map(|v| v.exported).unwrap_or(false);
            self.variables.insert(
                name.to_string(),
                ShellVar {
                    value,
                    exported,
                    readonly: false,
                },
            );
        }

        Ok(())
    }

    pub fn ifs(&self) -> &str {
        self.get_var("IFS").unwrap_or(" \t\n")
    }

    pub fn inherit_from_process(&mut self) {
        for (key, value) in std::env::vars() {
            self.variables.insert(
                key,
                ShellVar {
                    value: VarValue::Scalar(value),
                    exported: true,
                    readonly: false,
                },
            );
        }
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;
