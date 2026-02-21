use std::collections::HashMap;
use std::path::PathBuf;

use contracts::debug_ensures;

use crate::ast::{CompoundCommand, FunctionDef};
use crate::exec::error::ExecError;

/// A shell variable with metadata.
#[derive(Debug, Clone)]
struct ShellVar {
    value: String,
    exported: bool,
    readonly: bool,
}

/// A saved scope frame for function calls.
///
/// In POSIX, function calls only save/restore positional parameters.
/// Regular variables are NOT scoped — they are global. The `saved` map
/// is reserved for a future `local` builtin (Bash extension).
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

        // Set default IFS
        // IFS is always settable in a fresh environment (not readonly).
        let _ = env.set_var("IFS", " \t\n");

        env
    }

    // --- Variable access ---

    /// Get the value of a variable, or None if unset.
    pub fn get_var(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(|v| v.value.as_str())
    }

    /// Set a variable's value. Returns an error if the variable is readonly.
    ///
    /// In POSIX mode, variables are always global. Scope-based save/restore
    /// only happens for variables explicitly declared `local` (Bash extension).
    pub fn set_var(&mut self, name: &str, value: &str) -> Result<(), ExecError> {
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

        self.variables.insert(
            name.to_string(),
            ShellVar {
                value: value.to_string(),
                exported,
                readonly: false,
            },
        );
        Ok(())
    }

    /// Mark a variable as exported. If it doesn't exist, create it with empty value.
    pub fn export_var(&mut self, name: &str) {
        match self.variables.get_mut(name) {
            Some(var) => var.exported = true,
            None => {
                self.variables.insert(
                    name.to_string(),
                    ShellVar {
                        value: String::new(),
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
                        value: String::new(),
                        exported: false,
                        readonly: true,
                    },
                );
            }
        }
    }

    /// Unset a variable. Returns an error if readonly.
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
    pub fn exported_vars(&self) -> Vec<(String, String)> {
        self.variables
            .iter()
            .filter(|(_, v)| v.exported)
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect()
    }

    /// Returns all variables as (name, value) pairs.
    pub fn all_vars(&self) -> Vec<(String, String)> {
        self.variables
            .iter()
            .map(|(k, v)| (k.clone(), v.value.clone()))
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
                // Positional: $1, $2, ...
                if let Ok(n) = name.parse::<usize>() {
                    if n >= 1 {
                        return self.positional_params.get(n - 1).cloned();
                    }
                }
                None
            }
        }
    }

    /// Expand `$@` into separate fields (for proper `"$@"` handling).
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

    /// Change the current working directory. Returns Err on invalid path.
    pub fn set_cwd(&mut self, path: PathBuf) -> Result<(), ExecError> {
        if !path.is_dir() {
            return Err(ExecError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("cd: {}: No such file or directory", path.display()),
            )));
        }
        let canonical = path
            .canonicalize()
            .map_err(|e| ExecError::Io(e))?;
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

    /// Push a new scope for a function call.
    /// Saves the current positional params so they can be restored on pop.
    #[debug_ensures(self.scope_stack.len() == old(self.scope_stack.len()) + 1)]
    pub fn push_scope(&mut self, new_positional: Vec<String>) {
        let saved_positional = std::mem::replace(&mut self.positional_params, new_positional);
        self.scope_stack.push(Scope {
            saved: HashMap::new(),
            saved_positional,
        });
    }

    /// Pop the current scope, restoring positional params.
    ///
    /// In POSIX mode, only positional parameters are restored.
    /// Regular variables remain as-is (POSIX functions don't scope variables).
    /// The `saved` map is only used by `local` (Bash extension, future).
    #[debug_ensures(self.scope_stack.len() == old(self.scope_stack.len()) - 1)]
    pub fn pop_scope(&mut self) {
        if let Some(scope) = self.scope_stack.pop() {
            self.positional_params = scope.saved_positional;

            // Restore any explicitly-saved local variables (from `local` builtin).
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

    /// Returns true if we are inside a function scope.
    pub fn in_function_scope(&self) -> bool {
        !self.scope_stack.is_empty()
    }

    /// Declare a variable as local in the current function scope.
    ///
    /// Saves the current value (or absence) so `pop_scope` will restore it.
    /// Replaces the variable with a non-readonly copy so assignments to the
    /// local work even if the outer was readonly.
    pub fn declare_local(&mut self, name: &str) -> Result<(), ExecError> {
        let scope = self.scope_stack.last_mut().ok_or_else(|| {
            ExecError::BadSubstitution("local: can only be used in a function".to_string())
        })?;

        // Only save the first time — repeated `local X` is a no-op.
        if !scope.saved.contains_key(name) {
            let current = self.variables.get(name).cloned();
            scope.saved.insert(name.to_string(), current.clone());

            // Replace with a non-readonly copy so the local can be assigned.
            let value = current.as_ref().map(|v| v.value.as_str()).unwrap_or("");
            let exported = current.as_ref().map(|v| v.exported).unwrap_or(false);
            self.variables.insert(
                name.to_string(),
                ShellVar {
                    value: value.to_string(),
                    exported,
                    readonly: false,
                },
            );
        }

        Ok(())
    }

    /// Returns the IFS string (default: " \t\n").
    pub fn ifs(&self) -> &str {
        self.get_var("IFS").unwrap_or(" \t\n")
    }

    /// Import environment variables from the process environment.
    pub fn inherit_from_process(&mut self) {
        for (key, value) in std::env::vars() {
            self.variables.insert(
                key,
                ShellVar {
                    value,
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
