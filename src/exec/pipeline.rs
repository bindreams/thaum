use std::io::Write;
use std::process::{Child, Command, Stdio};

use crate::ast::Expression;
use crate::exec::command_ex::CommandEx;
use crate::exec::error::ExecError;
use crate::exec::io_context::IoContext;
use crate::exec::Executor;

/// Flatten a pipe tree into a list of expressions (left to right).
///
/// The AST represents `a | b | c` as `Pipe(Pipe(a, b), c)`.
/// This flattens it into `[a, b, c]`.
pub fn flatten_pipeline(expr: &Expression) -> Vec<&Expression> {
    let mut stages = Vec::new();
    collect_pipeline_stages(expr, &mut stages);
    stages
}

fn collect_pipeline_stages<'a>(expr: &'a Expression, stages: &mut Vec<&'a Expression>) {
    match expr {
        Expression::Pipe { left, right, .. } => {
            collect_pipeline_stages(left, stages);
            stages.push(right);
        }
        _ => {
            stages.push(expr);
        }
    }
}

/// Execute a pipeline of commands connected by pipes.
///
/// Returns the exit status of the last command in the pipeline.
pub fn execute_pipeline(
    executor: &mut Executor,
    stages: &[&Expression],
    io: &mut IoContext<'_>,
) -> Result<i32, ExecError> {
    debug_assert!(!stages.is_empty());

    if stages.len() == 1 {
        return executor.execute_expression(stages[0], io);
    }

    // Build the pipeline: spawn each stage, connecting stdout→stdin.
    let mut children: Vec<Child> = Vec::new();
    let mut prev_stdout: Option<std::process::ChildStdout> = None;

    for (i, stage) in stages.iter().enumerate() {
        let is_last = i == stages.len() - 1;

        // For each pipeline stage, we need to spawn the command with
        // appropriate stdin/stdout.
        let child = spawn_pipeline_stage(
            executor,
            stage,
            prev_stdout.take(),
            !is_last, // pipe_stdout: all but last
            io,
        )?;

        if let Some(mut child) = child {
            prev_stdout = child.stdout.take();
            children.push(child);
        } else {
            // Stage was a builtin or assignment — prev_stdout stays None
            prev_stdout = None;
        }
    }

    // Wait for all children and return the last one's exit status.
    let mut last_status = 0;
    for mut child in children {
        let status = child.wait().map_err(ExecError::Io)?;
        last_status = status.code().unwrap_or(128);
    }

    Ok(last_status)
}

/// Spawn a single pipeline stage.
///
/// Returns None if the stage was handled internally (builtin/assignment)
/// without spawning a child process.
fn spawn_pipeline_stage(
    executor: &mut Executor,
    expr: &Expression,
    stdin: Option<std::process::ChildStdout>,
    pipe_stdout: bool,
    io: &mut IoContext<'_>,
) -> Result<Option<Child>, ExecError> {
    match expr {
        Expression::Command(cmd) => {
            // Expand arguments
            let mut expanded_args: Vec<String> = Vec::new();
            for arg in &cmd.arguments {
                let fields = crate::exec::expand::expand_argument(arg, executor.env_mut())?;
                expanded_args.extend(fields);
            }

            if expanded_args.is_empty() {
                // Assignment-only command — handle in-process
                for assignment in &cmd.assignments {
                    let value = crate::exec::expand::expand_word(
                        assignment.value.as_scalar(),
                        executor.env_mut(),
                    )?;
                    executor.env_mut().set_var(&assignment.name, &value)?;
                }
                return Ok(None);
            }

            let cmd_name = &expanded_args[0];
            let cmd_args = &expanded_args[1..];

            // Check for builtins in pipeline — run in a subprocess-like manner
            if crate::exec::builtins::is_builtin(cmd_name) {
                // For builtins in a pipeline, we need to handle I/O differently.
                // Run the builtin capturing its output, then we can pipe it.
                let mut stdout_buf: Vec<u8> = Vec::new();
                let mut stderr_buf: Vec<u8> = Vec::new();

                let _status = crate::exec::builtins::run_builtin(
                    cmd_name,
                    cmd_args,
                    executor.env_mut(),
                    io.stdin,
                    &mut stdout_buf,
                    &mut stderr_buf,
                );

                // For builtins in a pipeline, we use `echo`-like approach:
                // pipe the output through `cat` to create a proper child process.
                if pipe_stdout && !stdout_buf.is_empty() {
                    let mut child = Command::new("cat")
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .spawn()
                        .map_err(ExecError::Io)?;

                    if let Some(ref mut child_stdin) = child.stdin {
                        let _ = child_stdin.write_all(&stdout_buf);
                    }
                    // Drop stdin to signal EOF
                    child.stdin.take();
                    return Ok(Some(child));
                } else {
                    // Last stage or no output — write directly
                    if !stdout_buf.is_empty() {
                        io.stdout.write_all(&stdout_buf).map_err(ExecError::Io)?;
                    }
                    if !stderr_buf.is_empty() {
                        io.stderr.write_all(&stderr_buf).map_err(ExecError::Io)?;
                    }
                    return Ok(None);
                }
            }

            // External command
            let mut child_cmd = CommandEx::new(cmd_name);
            child_cmd.args(cmd_args);
            child_cmd.current_dir(executor.env().cwd());

            // Set environment
            child_cmd.env_clear();
            for (key, val) in executor.env().exported_vars() {
                child_cmd.env(&key, &val);
            }

            // Apply prefix assignments as env vars
            for assignment in &cmd.assignments {
                let value = crate::exec::expand::expand_word(
                    assignment.value.as_scalar(),
                    executor.env_mut(),
                )?;
                child_cmd.env(&assignment.name, &value);
            }

            // Inherit persistent FDs 3+ from the executor's fd_table
            for (&fd, file) in executor.fd_table().iter() {
                child_cmd.fd_mapping(fd, file.try_clone().map_err(ExecError::Io)?);
            }

            // Set up stdin from previous stage
            if let Some(prev_out) = stdin {
                child_cmd.stdin(prev_out);
            }

            // Set up stdout for piping
            if pipe_stdout {
                child_cmd.stdout(Stdio::piped());
            }

            match child_cmd.spawn() {
                Ok(child) => Ok(Some(child)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let _ = writeln!(io.stderr, "{}: command not found", cmd_name);
                    Ok(None)
                }
                Err(e) => Err(ExecError::Io(e)),
            }
        }
        // For compound commands in pipeline stages, we'd need to fork.
        // For now, fall back to sequential execution.
        _ => {
            let status = executor.execute_expression(expr, io)?;
            executor.env_mut().set_last_exit_status(status);
            Ok(None)
        }
    }
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
