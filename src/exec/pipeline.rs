//! Pipeline execution: flattens the left-associative `Pipe` tree, spawns each
//! stage with piped stdin/stdout, and returns the exit status of the last stage.

use std::io::Write;

use crate::ast::Expression;
use crate::exec::command_ex::{ChildEx, CommandEx, Fd};
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
    let mut children: Vec<ChildEx> = Vec::new();
    let mut prev_stdout: Option<std::fs::File> = None;

    for (i, stage) in stages.iter().enumerate() {
        let is_last = i == stages.len() - 1;

        let child = spawn_pipeline_stage(executor, stage, prev_stdout.take(), !is_last, io)?;

        if let Some(mut child) = child {
            prev_stdout = child.take_pipe(1);
            children.push(child);
        } else {
            prev_stdout = None;
        }
    }

    // Wait for all children and collect exit statuses.
    let mut statuses: Vec<i32> = Vec::new();
    for mut child in children {
        let status = child.wait().map_err(ExecError::Io)?;
        statuses.push(status);
    }
    let last_status = statuses.last().copied().unwrap_or(0);

    // Store PIPESTATUS array.
    let status_strs: Vec<String> = statuses.iter().map(|s| s.to_string()).collect();
    debug_assert!(executor.env_mut().set_array("PIPESTATUS", status_strs).is_ok());

    Ok(last_status)
}

/// Spawn a single pipeline stage.
///
/// Returns None if the stage was handled internally (builtin/assignment)
/// without spawning a child process.
fn spawn_pipeline_stage(
    executor: &mut Executor,
    expr: &Expression,
    stdin: Option<std::fs::File>,
    pipe_stdout: bool,
    io: &mut IoContext<'_>,
) -> Result<Option<ChildEx>, ExecError> {
    match expr {
        Expression::Command(cmd) => {
            // Expand arguments
            let mut expanded_args: Vec<String> = Vec::new();
            for arg in &cmd.arguments {
                let fields = crate::exec::expand::expand_argument(arg, executor.env_mut())?;
                expanded_args.extend(fields);
            }

            if expanded_args.is_empty() {
                for assignment in &cmd.assignments {
                    executor.execute_assignment(assignment)?;
                }
                return Ok(None);
            }

            let cmd_name = &expanded_args[0];
            let cmd_args = &expanded_args[1..];

            // Builtins in pipeline — run in-process, pipe output through `cat`.
            if crate::exec::builtins::is_builtin(cmd_name) {
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

                if pipe_stdout && !stdout_buf.is_empty() {
                    let (read_end, write_end) = os_pipe()?;
                    // Write builtin output and close the write end so the
                    // next stage sees EOF.
                    {
                        let mut writer = write_end;
                        let _ = writer.write_all(&stdout_buf);
                    }
                    let mut pipes = std::collections::HashMap::new();
                    pipes.insert(1, read_end);
                    return Ok(Some(ChildEx::completed(_status.unwrap_or(1), pipes)));
                } else {
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
            let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(1 + cmd_args.len());
            argv.push(cmd_name.into());
            argv.extend(cmd_args.iter().map(std::ffi::OsString::from));

            let mut child_cmd = CommandEx::new(argv);
            child_cmd.cwd = Some(executor.env().cwd().to_path_buf());

            let env: std::collections::HashMap<std::ffi::OsString, std::ffi::OsString> = executor
                .env()
                .exported_vars()
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect();
            child_cmd.env = env;

            for assignment in &cmd.assignments {
                let value = executor.expand_scalar_assignment(assignment)?;
                child_cmd.env.insert(assignment.name.clone().into(), value.into());
            }

            for (&fd, file) in executor.fd_table().iter() {
                child_cmd
                    .fds
                    .insert(fd, Fd::File(file.try_clone().map_err(ExecError::Io)?));
            }

            if let Some(prev_out) = stdin {
                child_cmd.fds.insert(0, Fd::File(prev_out));
            }

            if pipe_stdout {
                child_cmd.fds.insert(1, Fd::Pipe);
            } else {
                // Last pipeline stage: pipe stdout to relay through IoContext.
                child_cmd.fds.entry(1).or_insert(Fd::Pipe);
            }
            // Always pipe stderr (unless redirected) to relay through IoContext.
            child_cmd.fds.entry(2).or_insert(Fd::Pipe);

            match child_cmd.spawn() {
                Ok(mut child) => {
                    if !pipe_stdout {
                        // Last stage: drain pipes and relay output through io.
                        let (stdout_buf, stderr_buf) = crate::exec::child_io::drain_child_pipes(&mut child)?;
                        if !stdout_buf.is_empty() {
                            io.stdout.write_all(&stdout_buf).map_err(ExecError::Io)?;
                        }
                        if !stderr_buf.is_empty() {
                            io.stderr.write_all(&stderr_buf).map_err(ExecError::Io)?;
                        }
                    }
                    Ok(Some(child))
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let _ = writeln!(io.stderr, "{cmd_name}: command not found");
                    Ok(None)
                }
                Err(e) => Err(ExecError::Io(e)),
            }
        }
        _ => {
            let status = executor.execute_expression(expr, io)?;
            executor.env_mut().set_last_exit_status(status);
            Ok(None)
        }
    }
}

/// Create an OS pipe, returning `(read_end, write_end)` as `std::fs::File`.
pub(super) fn os_pipe() -> Result<(std::fs::File, std::fs::File), ExecError> {
    #[cfg(unix)]
    {
        let (read, write) = nix::unistd::pipe().map_err(|e| ExecError::Io(std::io::Error::other(e)))?;
        Ok((std::fs::File::from(read), std::fs::File::from(write)))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::FromRawHandle;
        use windows::Win32::System::Pipes::CreatePipe;
        let mut read_handle = windows::Win32::Foundation::HANDLE::default();
        let mut write_handle = windows::Win32::Foundation::HANDLE::default();
        unsafe { CreatePipe(&mut read_handle, &mut write_handle, None, 0) }
            .map_err(|e| ExecError::Io(std::io::Error::other(e)))?;
        Ok((unsafe { std::fs::File::from_raw_handle(read_handle.0 as _) }, unsafe {
            std::fs::File::from_raw_handle(write_handle.0 as _)
        }))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(ExecError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "pipes not supported on this platform",
        )))
    }
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
