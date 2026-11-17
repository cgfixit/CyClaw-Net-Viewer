//! Process identity from public libproc: parent PID and start time.
//!
//! The kernel reuses PIDs. A PID plus the process start time names one
//! process for as long as it lives, which is what the tree tracker and the
//! watch stream key on.

use std::mem::{size_of, MaybeUninit};

/// When a process was created, as the kernel reports it. This is wall-clock
/// time, so it identifies a process but does not order two of them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProcStart {
    pub sec: u64,
    pub usec: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProcIdentity {
    pub pid: u32,
    pub ppid: u32,
    pub start: ProcStart,
}

/// Identity of a running process, or `None` when it has exited or this user
/// may not read it. `PROC_PIDTBSDINFO` is same-user only unless root, so
/// processes owned by other users have no identity here. An exited process
/// its parent has not reaped yet (a zombie) still answers this call; it
/// holds no sockets and counts as exited.
pub fn identity(pid: u32) -> Option<ProcIdentity> {
    let info = bsd_info(pid)?;
    (info.pbi_pid == pid && info.pbi_status != libc::SZOMB).then_some(ProcIdentity {
        pid,
        ppid: info.pbi_ppid,
        start: ProcStart {
            sec: info.pbi_start_tvsec,
            usec: info.pbi_start_tvusec,
        },
    })
}

/// True only when this user can read `pid` and it is an exited process its
/// parent has not reaped. A zombie holds no descriptors.
pub fn is_zombie(pid: u32) -> bool {
    bsd_info(pid).is_some_and(|info| info.pbi_pid == pid && info.pbi_status == libc::SZOMB)
}

fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    let target = libc::pid_t::try_from(pid).ok().filter(|p| *p > 0)?;
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = libc::c_int::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: the buffer is writable for exactly `size` bytes, the flavor
    // matches the struct, and libproc does not keep the pointer.
    let written = unsafe {
        libc::proc_pidinfo(
            target,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    // A short or failed read leaves nothing to trust.
    if written != size {
        return None;
    }
    // SAFETY: the storage was zeroed, then fully written by libproc.
    Some(unsafe { info.assume_init() })
}

/// Every PID the kernel lists right now. Processes can start between the
/// size query and the read, so the buffer has headroom and grows on a
/// full read.
pub fn pids() -> Result<Vec<u32>, String> {
    let mut headroom = 64usize;
    for _ in 0..4 {
        // SAFETY: a null buffer of size zero only asks for the PID count.
        let estimate = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        if estimate <= 0 {
            return Err("libproc could not list processes".into());
        }
        let capacity = estimate as usize + headroom;
        let bytes = libc::c_int::try_from(capacity * size_of::<libc::pid_t>())
            .map_err(|_| "process list is too large".to_string())?;
        let mut buf: Vec<libc::pid_t> = vec![0; capacity];
        // SAFETY: the buffer is writable for `bytes` bytes and outlives the call.
        let listed = unsafe { libc::proc_listallpids(buf.as_mut_ptr().cast(), bytes) };
        if listed <= 0 {
            return Err("libproc could not list processes".into());
        }
        let listed = listed as usize;
        if listed < capacity {
            buf.truncate(listed);
            return Ok(buf
                .into_iter()
                .filter_map(|pid| u32::try_from(pid).ok().filter(|p| *p > 0))
                .collect());
        }
        // A full buffer may have been truncated; retry with more room.
        headroom *= 4;
    }
    Err("process list kept growing while it was read".into())
}

/// PIDs whose parent is `ppid`, whoever owns them. Listing needs no
/// permission over the processes listed, so this sees children this user
/// cannot otherwise read, such as a setuid program a watched process ran.
pub fn children(ppid: u32) -> Vec<u32> {
    let Ok(parent) = libc::pid_t::try_from(ppid) else {
        return Vec::new();
    };
    let mut headroom = 16usize;
    for _ in 0..4 {
        // SAFETY: a null buffer of size zero only asks for the PID count.
        let estimate = unsafe { libc::proc_listchildpids(parent, std::ptr::null_mut(), 0) };
        if estimate < 0 {
            return Vec::new();
        }
        let capacity = estimate as usize + headroom;
        let Ok(bytes) = libc::c_int::try_from(capacity * size_of::<libc::pid_t>()) else {
            return Vec::new();
        };
        let mut buf: Vec<libc::pid_t> = vec![0; capacity];
        // SAFETY: the buffer is writable for `bytes` bytes and outlives the call.
        let listed = unsafe { libc::proc_listchildpids(parent, buf.as_mut_ptr().cast(), bytes) };
        if listed < 0 {
            return Vec::new();
        }
        let listed = listed as usize;
        if listed < capacity {
            buf.truncate(listed);
            return buf
                .into_iter()
                .filter_map(|pid| u32::try_from(pid).ok().filter(|p| *p > 0))
                .collect();
        }
        headroom *= 4;
    }
    Vec::new()
}

/// Identities of every process this user may read. One syscall per PID;
/// a process that exits mid-scan is simply absent.
pub fn table() -> Result<Vec<ProcIdentity>, String> {
    Ok(pids()?.into_iter().filter_map(identity).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_identity_is_stable_and_names_a_real_parent() {
        let me = std::process::id();
        let first = identity(me).expect("own identity");
        assert_eq!(first.pid, me);
        assert!(first.ppid > 0);
        assert!(first.start.sec > 0);
        assert!(first.start.usec < 1_000_000);
        // Same process, same answer: the start time is the durable half.
        assert_eq!(identity(me), Some(first));
        // The parent is the test runner, which this user may also read.
        assert_eq!(identity(first.ppid).map(|p| p.pid), Some(first.ppid));
    }

    #[test]
    fn invalid_and_absent_pids_have_no_identity() {
        for pid in [0, i32::MAX as u32 + 1, u32::MAX] {
            assert_eq!(identity(pid), None, "PID {pid}");
        }
    }

    #[test]
    fn an_exited_child_that_is_not_reaped_yet_has_no_identity() {
        let mut child = std::process::Command::new("/usr/bin/true")
            .spawn()
            .expect("start a child that exits at once");
        let pid = child.id();
        // Not waited for: once it exits it is a zombie, and its PID cannot
        // be reused until the wait below.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while identity(pid).is_some() || !is_zombie(pid) {
            assert!(
                std::time::Instant::now() < deadline,
                "the child never read as an unreaped zombie with no identity"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!table().unwrap().iter().any(|p| p.pid == pid));
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn children_lists_a_running_child_and_not_its_parent() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .expect("start a child");
        let me = std::process::id();
        let listed = children(me);
        let found = listed.contains(&child.id());
        child.kill().ok();
        child.wait().ok();
        assert!(found, "{listed:?} lacks {}", child.id());
        assert!(!listed.contains(&me));
        assert!(children(0x7fff_fff0).is_empty());
    }

    #[test]
    fn table_lists_this_process_once() {
        let me = std::process::id();
        let table = table().expect("process table");
        assert_eq!(table.iter().filter(|p| p.pid == me).count(), 1);
        assert!(pids().unwrap().contains(&me));
    }
}
