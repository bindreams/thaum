//! Directory stack operations for `pushd`, `popd`, `dirs`, and `DIRSTACK`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{Environment, ShellVar, VarValue};

impl Environment {
    /// Get the directory stack (index 0 = current directory).
    pub fn dir_stack(&self) -> &[PathBuf] {
        &self.dir_stack
    }

    /// Push a directory onto the stack.
    pub fn dir_stack_push(&mut self, dir: PathBuf) {
        self.dir_stack.insert(0, dir);
        self.sync_dirstack_var();
    }

    /// Pop the top directory from the stack. Returns None if only 1 entry (current dir).
    pub fn dir_stack_pop(&mut self) -> Option<PathBuf> {
        if self.dir_stack.len() <= 1 {
            return None;
        }
        let popped = self.dir_stack.remove(0);
        self.sync_dirstack_var();
        Some(popped)
    }

    /// Remove a directory at a specific index. Returns None if out of bounds.
    pub fn dir_stack_remove(&mut self, index: usize) -> Option<PathBuf> {
        if index >= self.dir_stack.len() || self.dir_stack.len() <= 1 {
            return None;
        }
        let removed = self.dir_stack.remove(index);
        self.sync_dirstack_var();
        Some(removed)
    }

    /// Swap the top two entries in the directory stack.
    pub fn dir_stack_swap(&mut self) -> bool {
        if self.dir_stack.len() < 2 {
            return false;
        }
        self.dir_stack.swap(0, 1);
        self.sync_dirstack_var();
        true
    }

    /// Insert a directory at a specific index in the stack.
    pub fn dir_stack_insert(&mut self, index: usize, dir: PathBuf) {
        self.dir_stack.insert(index, dir);
        self.sync_dirstack_var();
    }

    /// Clear the directory stack (keep only index 0 = current dir).
    pub fn dir_stack_clear(&mut self) {
        self.dir_stack.truncate(1);
        self.sync_dirstack_var();
    }

    /// Update dir_stack[0] to match the current working directory.
    pub fn sync_dir_stack_cwd(&mut self) {
        if !self.dir_stack.is_empty() {
            self.dir_stack[0] = self.cwd.clone();
        }
        self.sync_dirstack_var();
    }

    /// Sync the DIRSTACK shell array variable from the internal dir_stack.
    fn sync_dirstack_var(&mut self) {
        let strs: Vec<String> = self.dir_stack.iter().map(|p| p.to_string_lossy().to_string()).collect();
        // Write directly to variables to bypass dynamic intercepts.
        let map: BTreeMap<usize, String> = strs.into_iter().enumerate().collect();
        self.variables.insert(
            "DIRSTACK".to_string(),
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
