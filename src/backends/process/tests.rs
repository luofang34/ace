use super::*;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn blocked_process() -> std::process::Command {
    let mut command = std::process::Command::new("/usr/bin/tail");
    command.args(["-f", "/dev/null"]);
    command
}

#[test]
fn synchronous_supervision_can_be_called_from_an_async_entry_point() {
    runtime().block_on(async {
        let command = std::process::Command::new("/usr/bin/true");
        assert!(
            output_blocking(command, &RunControl::default())
                .unwrap()
                .status
                .success()
        );
    });
}

async fn assert_reaped(pid: u32) {
    let status = Command::new("/bin/kill")
        .args(["-0", "--", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .await
        .unwrap();
    assert!(!status.success(), "owned child is still running");
}

#[test]
#[cfg(unix)]
fn cancellation_reaps_a_real_child_process_after_its_start_event() {
    runtime().block_on(async {
        let control = RunControl::bounded(Duration::from_secs(10));
        let token = control.clone();
        let (started, ready) = oneshot::channel();
        let task =
            tokio::spawn(async move { output(blocked_process(), &token, Some(started)).await });
        let pid = ready.await.unwrap();
        control.cancel();
        let result = task.await.unwrap();
        assert!(matches!(
            result,
            Err(ProcessError::Interrupted(Interruption::Canceled))
        ));
        assert_reaped(pid).await;
    });
}

#[test]
#[cfg(unix)]
fn timeout_reaps_a_real_child_without_polling_or_fabricating_completion() {
    runtime().block_on(async {
        let control = RunControl::bounded(Duration::from_secs(1));
        let (started, ready) = oneshot::channel();
        let task =
            tokio::spawn(async move { output(blocked_process(), &control, Some(started)).await });
        let pid = ready.await.unwrap();
        let result = task.await.unwrap();
        assert!(matches!(
            result,
            Err(ProcessError::Interrupted(Interruption::TimedOut))
        ));
        assert_reaped(pid).await;
    });
}
