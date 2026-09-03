/// SIGTERM a process. Darwin has no public TCB-delete for another process's socket.
pub fn terminate(pid: u32) -> Result<(), String> {
    if pid == 0 {
        return Err("no owning PID".into());
    }
    let me = std::process::id();
    if pid == me {
        return Err("refusing to terminate NetBoard itself".into());
    }
    let rc = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if rc == 0 {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        Err(format!("kill({pid}): {err}"))
    }
}
