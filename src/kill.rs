/// SIGTERM a process. Darwin has no public TCB-delete for another process's socket.
pub fn terminate(pid: u32) -> Result<(), String> {
    let target = checked_target(pid)?;
    // Only a positive, representable process ID reaches kill; negative IDs
    // and zero have process-group semantics on Darwin.
    let rc = unsafe { libc::kill(target, libc::SIGTERM) };
    if rc == 0 {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        Err(format!("kill({pid}): {err}"))
    }
}

/// SIGTERM `pid` only if it still names `expected`, the process the user
/// confirmed. Narrows (does not close) the PID-reuse window between the
/// snapshot that showed the row and the confirmation click.
pub fn terminate_expected(pid: u32, expected: &str) -> Result<(), String> {
    still_expected(pid, expected, crate::snapshot::process_name)?;
    terminate(pid)
}

fn still_expected(
    pid: u32,
    expected: &str,
    current: impl FnOnce(u32) -> String,
) -> Result<(), String> {
    checked_target(pid)?;
    let now = current(pid);
    if now == expected {
        Ok(())
    } else {
        Err(format!(
            "PID {pid} is now {now:?}, not {expected:?}; refresh and retry"
        ))
    }
}

fn checked_target(pid: u32) -> Result<libc::pid_t, String> {
    if pid == 0 {
        return Err("no owning PID".into());
    }
    if pid == 1 {
        return Err("refusing to terminate launchd".into());
    }
    let me = std::process::id();
    if pid == me {
        return Err("refusing to terminate CyClaw-Net-Viewer itself".into());
    }
    libc::pid_t::try_from(pid).map_err(|_| "PID is outside the valid process ID range".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test validation directly so even a regression cannot send a real signal.
    #[test]
    fn rejects_special_self_and_wrapping_pids() {
        for pid in [0, 1, std::process::id(), i32::MAX as u32 + 1, u32::MAX] {
            assert!(checked_target(pid).is_err(), "accepted PID {pid}");
        }
    }

    #[test]
    fn identity_check_refuses_reused_pids_without_signaling() {
        assert!(still_expected(4242, "curl", |_| "curl".into()).is_ok());
        let err = still_expected(4242, "curl", |_| "Safari".into()).unwrap_err();
        assert!(err.contains("refresh"), "{err}");
        // Exited process: no name, never a match for a real one.
        assert!(still_expected(4242, "curl", |_| "?".into()).is_err());
        // PID validation still runs first.
        assert!(still_expected(0, "?", |_| "?".into()).is_err());
        assert!(still_expected(1, "launchd", |_| "launchd".into()).is_err());
    }

    #[test]
    fn preserves_positive_pid() {
        assert_eq!(checked_target(i32::MAX as u32), Ok(i32::MAX));
    }
}
