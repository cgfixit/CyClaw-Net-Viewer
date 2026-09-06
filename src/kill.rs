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
    fn preserves_positive_pid() {
        assert_eq!(checked_target(i32::MAX as u32), Ok(i32::MAX));
    }
}
