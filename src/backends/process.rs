use ace_engine::{Interruption, RunControl};
use std::{
    path::PathBuf,
    process::{Output, Stdio},
    time::Duration,
};
use tokio::{process::Command, sync::oneshot};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ProcessError {
    #[error("external process {executable}: {source}")]
    Io {
        executable: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Interrupted(#[from] Interruption),
    #[error("process group {pid} did not exit during cleanup: {source}")]
    Cleanup {
        pid: u32,
        #[source]
        source: tokio::time::error::Elapsed,
    },
    #[error("supervised process groups are unavailable on this platform")]
    Unsupported,
    #[error("external process supervisor terminated unexpectedly")]
    Supervisor,
}

pub(crate) fn output_blocking(
    command: std::process::Command,
    control: &RunControl,
) -> Result<Output, ProcessError> {
    let executable = PathBuf::from(command.get_program());
    // Synchronous entry points can be invoked from either native or async callers.
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("external-process-supervisor".into())
            .spawn_scoped(scope, || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|source| ProcessError::Io {
                        executable: executable.clone(),
                        source,
                    })?;
                runtime.block_on(output(command, control, None))
            })
            .map_err(|source| ProcessError::Io {
                executable: executable.clone(),
                source,
            })?;
        worker.join().map_err(|_| ProcessError::Supervisor)?
    })
}

async fn output(
    command: std::process::Command,
    control: &RunControl,
    started: Option<oneshot::Sender<u32>>,
) -> Result<Output, ProcessError> {
    control.checkpoint()?;
    if !cfg!(unix) {
        return Err(ProcessError::Unsupported);
    }
    let executable = PathBuf::from(command.get_program());
    let mut command = Command::from(command);
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = command.spawn().map_err(|source| ProcessError::Io {
        executable: executable.clone(),
        source,
    })?;
    let pid = child.id().ok_or(ProcessError::Unsupported)?;
    if let Some(started) = started {
        started.send(pid).ok();
    }
    let mut completion = Box::pin(child.wait_with_output());
    let deadline = control
        .deadline()
        .unwrap_or_else(|| std::time::Instant::now() + Duration::from_secs(300));
    let interruption = tokio::select! {
        result = &mut completion => return result.map_err(|source| ProcessError::Io { executable,source }),
        () = control.cancelled() => Interruption::Canceled,
        () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Interruption::TimedOut,
    };
    let kill = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .status()
        .await
        .map_err(|source| ProcessError::Io {
            executable: PathBuf::from("/bin/kill"),
            source,
        })?;
    if !kill.success() {
        tracing::debug!(pid, "process group already exited during cancellation");
    }
    tokio::time::timeout(Duration::from_secs(5), completion)
        .await
        .map_err(|source| ProcessError::Cleanup { pid, source })?
        .map_err(|source| ProcessError::Io { executable, source })?;
    Err(ProcessError::Interrupted(interruption))
}

#[cfg(test)]
mod tests;
