use crossbeam_channel::{bounded, Sender};
use socket2::SockAddr;
use std::collections::HashMap;
use std::ffi::CStr;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

const WORKER_COUNT: usize = 8;
const QUEUE_CAPACITY: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub host: String,
}

/// Background reverse-DNS cache. Unspecified addresses are never queued.
pub struct Resolver {
    cache: Arc<Mutex<HashMap<IpAddr, Option<Resolved>>>>,
    tx: Option<Sender<IpAddr>>,
    workers: Vec<JoinHandle<()>>,
}

fn cache_lock(
    m: &Mutex<HashMap<IpAddr, Option<Resolved>>>,
) -> std::sync::MutexGuard<'_, HashMap<IpAddr, Option<Resolved>>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Resolver {
    pub fn new() -> Self {
        Self::with_lookup(WORKER_COUNT, QUEUE_CAPACITY, lookup_ip)
    }

    fn with_lookup<F>(worker_count: usize, capacity: usize, lookup: F) -> Self
    where
        F: Fn(IpAddr) -> Option<Resolved> + Send + Sync + 'static,
    {
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = bounded::<IpAddr>(capacity);
        let lookup = Arc::new(lookup);
        let mut workers = Vec::with_capacity(worker_count);
        for i in 0..worker_count {
            let cache = Arc::clone(&cache);
            let rx = rx.clone();
            let lookup = Arc::clone(&lookup);
            match thread::Builder::new()
                .name(format!("netboard-dns-{i}"))
                .spawn(move || {
                    // Each receiver competes for work without a shared receive lock.
                    while let Ok(ip) = rx.recv() {
                        let name = lookup(ip);
                        cache_lock(&cache).insert(ip, name);
                    }
                }) {
                Ok(worker) => workers.push(worker),
                Err(e) => eprintln!("could not start DNS worker {i}: {e}"),
            }
        }
        // If all spawns fail, dropping rx disconnects tx; requests then remain
        // uncached instead of being marked pending forever.
        Self {
            cache,
            tx: Some(tx),
            workers,
        }
    }

    pub fn request(&self, ip: IpAddr) {
        if ip.is_unspecified() {
            return;
        }
        let mut cache = cache_lock(&self.cache);
        if cache.contains_key(&ip) {
            return;
        }
        // Keep the cache lock across enqueue + pending insertion so an instant
        // lookup cannot publish its result before pending overwrites it.
        if self.tx.as_ref().is_some_and(|tx| tx.try_send(ip).is_ok()) {
            cache.insert(ip, None);
        }
    }

    pub fn get(&self, ip: IpAddr) -> Option<Resolved> {
        cache_lock(&self.cache).get(&ip).cloned().flatten()
    }

    #[cfg(test)]
    pub fn contains(&self, ip: IpAddr) -> bool {
        cache_lock(&self.cache).contains_key(&ip)
    }

    #[cfg(test)]
    pub fn insert_test(&self, ip: IpAddr, name: Option<Resolved>) {
        cache_lock(&self.cache).insert(ip, name);
    }
}

impl Drop for Resolver {
    fn drop(&mut self) {
        // Disconnect first, then drain queued work and join every worker without
        // holding the cache lock. System DNS calls cannot be cancelled: shutdown
        // can wait for the system resolver's timeout on an in-flight lookup.
        drop(self.tx.take());
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

pub fn lookup_ip(ip: IpAddr) -> Option<Resolved> {
    if ip.is_unspecified() {
        return None;
    }
    let host = reverse_dns(ip)?;
    Some(Resolved { host })
}

fn reverse_dns(ip: IpAddr) -> Option<String> {
    let sa = SockAddr::from(SocketAddr::new(ip, 0));
    let mut host = [0u8; libc::NI_MAXHOST as usize];
    // SAFETY: SockAddr owns correctly aligned IPv4/IPv6 storage with Darwin's
    // family, length and network byte order. Both it and the writable host
    // buffer outlive this call. No service buffer is requested (null, zero).
    let ret = unsafe {
        libc::getnameinfo(
            sa.as_ptr(),
            sa.len(),
            host.as_mut_ptr().cast(),
            host.len() as libc::socklen_t,
            std::ptr::null_mut(),
            0,
            libc::NI_NAMEREQD,
        )
    };
    if ret != 0 {
        return None;
    }
    // Require NUL within the supplied buffer; never scan past its bounds.
    CStr::from_bytes_until_nul(&host)
        .ok()?
        .to_str()
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn workers_receive_concurrently_and_suppress_pending_duplicates() {
        let (started_tx, started_rx) = bounded(2);
        let (release_tx, release_rx) = bounded(2);
        let calls = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&calls);
        let resolver = Resolver::with_lookup(2, 4, move |ip| {
            count.fetch_add(1, Ordering::SeqCst);
            started_tx.send(ip).unwrap();
            release_rx.recv_timeout(TIMEOUT).unwrap();
            Some(Resolved {
                host: "example.test".into(),
            })
        });
        let first = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let second = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2));
        resolver.request(first);
        resolver.request(first);
        resolver.request(second);
        // Both workers must enter lookup before either is released.
        let mut started = [
            started_rx.recv_timeout(TIMEOUT).unwrap(),
            started_rx.recv_timeout(TIMEOUT).unwrap(),
        ];
        started.sort();
        assert_eq!(started, [first, second]);
        let cache = Arc::clone(&resolver.cache);
        release_tx.send(()).unwrap();
        release_tx.send(()).unwrap();
        drop(resolver); // Joins in-flight workers and publishes both answers.
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(cache_lock(&cache).values().all(Option::is_some));
    }

    #[test]
    fn full_queue_is_retryable_and_shutdown_drains_it() {
        let (started_tx, started_rx) = bounded(3);
        let (release_tx, release_rx) = bounded(3);
        let resolver = Resolver::with_lookup(1, 1, move |ip| {
            started_tx.send(ip).unwrap();
            release_rx.recv_timeout(TIMEOUT).unwrap();
            None
        });
        let first = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let second = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2));
        let third = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 3));
        resolver.request(first);
        assert_eq!(started_rx.recv_timeout(TIMEOUT).unwrap(), first);
        resolver.request(second);
        resolver.request(third);
        assert!(resolver.contains(second));
        assert!(!resolver.contains(third));
        release_tx.send(()).unwrap();
        assert_eq!(started_rx.recv_timeout(TIMEOUT).unwrap(), second);
        // The first negative result is cached and must not be retried.
        resolver.request(first);
        resolver.request(third);
        assert!(resolver.contains(third));
        release_tx.send(()).unwrap();
        release_tx.send(()).unwrap();
        drop(resolver);
        assert_eq!(started_rx.recv_timeout(TIMEOUT).unwrap(), third);
        assert!(started_rx.try_recv().is_err());
    }

    #[test]
    fn idle_shutdown_and_no_workers_do_not_leave_pending_requests() {
        drop(Resolver::with_lookup(2, 1, |_| panic!("no work expected")));
        let resolver = Resolver::with_lookup(0, 1, |_| panic!("no worker expected"));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        resolver.request(ip);
        assert!(!resolver.contains(ip));
    }

    #[test]
    fn sockaddr_conversion_preserves_ipv4_and_ipv6() {
        use std::net::{Ipv6Addr, SocketAddrV6};
        let v4 = SocketAddr::new(Ipv4Addr::new(192, 0, 2, 1).into(), 0x1234);
        let v6 = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0x1234, 7, 3));
        for addr in [v4, v6] {
            let sa = SockAddr::from(addr);
            assert_eq!(sa.as_socket(), Some(addr));
            let (family, len) = if addr.is_ipv4() {
                (libc::AF_INET, std::mem::size_of::<libc::sockaddr_in>())
            } else {
                (libc::AF_INET6, std::mem::size_of::<libc::sockaddr_in6>())
            };
            assert_eq!(i32::from(sa.family()), family);
            assert_eq!(sa.len() as usize, len);
            let storage = sa.as_storage();
            assert_eq!(storage.ss_len as usize, len);
        }
    }

    #[test]
    fn unspecified_never_queued() {
        let r = Resolver::new();
        let ip = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        r.request(ip);
        assert!(!r.contains(ip));
        assert_eq!(lookup_ip(ip), None);
    }

    #[test]
    fn fake_insert_is_returned() {
        let r = Resolver::new();
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        r.insert_test(
            ip,
            Some(Resolved {
                host: "example.test".into(),
            }),
        );
        let got = r.get(ip).expect("cached");
        assert_eq!(got.host, "example.test");
    }
}
