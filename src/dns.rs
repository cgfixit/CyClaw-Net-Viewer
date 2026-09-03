use std::collections::HashMap;
use std::ffi::CStr;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub host: String,
    pub ipv4: Option<Ipv4Addr>,
}

/// Background reverse-DNS cache. Unspecified addresses are never queued.
pub struct Resolver {
    cache: Arc<Mutex<HashMap<IpAddr, Option<Resolved>>>>,
    tx: SyncSender<IpAddr>,
}

fn cache_lock(
    m: &Mutex<HashMap<IpAddr, Option<Resolved>>>,
) -> std::sync::MutexGuard<'_, HashMap<IpAddr, Option<Resolved>>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Resolver {
    pub fn new() -> Self {
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = mpsc::sync_channel::<IpAddr>(256);
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..8 {
            let cache = cache.clone();
            let rx = rx.clone();
            let _ = thread::Builder::new()
                .name(format!("netboard-dns-{i}"))
                .spawn(move || loop {
                    let ip = { rx.lock().ok().and_then(|r| r.recv().ok()) };
                    match ip {
                        Some(ip) => {
                            let name = lookup_ip(ip);
                            cache_lock(&cache).insert(ip, name);
                        }
                        None => break,
                    }
                });
        }
        Self { cache, tx }
    }

    pub fn request(&self, ip: IpAddr) {
        if ip.is_unspecified() {
            return;
        }
        let mut cache = cache_lock(&self.cache);
        if cache.contains_key(&ip) {
            return;
        }
        if self.tx.try_send(ip).is_ok() {
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

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

fn socket_ipv4(ip: IpAddr) -> Option<Ipv4Addr> {
    match ip {
        IpAddr::V4(v) => Some(v),
        IpAddr::V6(v) => v.to_ipv4_mapped(),
    }
}

fn forward_ipv4(host: &str) -> Option<Ipv4Addr> {
    (host, 0u16)
        .to_socket_addrs()
        .ok()?
        .find_map(|sa| match sa.ip() {
            IpAddr::V4(v) => Some(v),
            _ => None,
        })
}

pub fn lookup_ip(ip: IpAddr) -> Option<Resolved> {
    if ip.is_unspecified() {
        return None;
    }
    let host = reverse_dns(ip)?;
    let ipv4 = socket_ipv4(ip).or_else(|| forward_ipv4(&host));
    Some(Resolved { host, ipv4 })
}

fn reverse_dns(ip: IpAddr) -> Option<String> {
    let sa = SocketAddr::new(ip, 0);
    let mut host = [0i8; 1025];
    let ret = unsafe {
        match sa {
            SocketAddr::V4(v4) => {
                let sin = libc::sockaddr_in {
                    sin_len: std::mem::size_of::<libc::sockaddr_in>() as u8,
                    sin_family: libc::AF_INET as u8,
                    sin_port: 0,
                    sin_addr: libc::in_addr {
                        s_addr: u32::from(*v4.ip()).to_be(),
                    },
                    sin_zero: [0; 8],
                };
                libc::getnameinfo(
                    &sin as *const _ as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_in>() as u32,
                    host.as_mut_ptr(),
                    host.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    libc::NI_NAMEREQD,
                )
            }
            SocketAddr::V6(v6) => {
                let sin6 = libc::sockaddr_in6 {
                    sin6_len: std::mem::size_of::<libc::sockaddr_in6>() as u8,
                    sin6_family: libc::AF_INET6 as u8,
                    sin6_port: 0,
                    sin6_flowinfo: 0,
                    sin6_addr: libc::in6_addr {
                        s6_addr: v6.ip().octets(),
                    },
                    sin6_scope_id: 0,
                };
                libc::getnameinfo(
                    &sin6 as *const _ as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_in6>() as u32,
                    host.as_mut_ptr(),
                    host.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    libc::NI_NAMEREQD,
                )
            }
        }
    };
    if ret != 0 {
        return None;
    }
    unsafe { CStr::from_ptr(host.as_ptr()) }
        .to_str()
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

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
                ipv4: Some(Ipv4Addr::new(1, 2, 3, 4)),
            }),
        );
        let got = r.get(ip).expect("cached");
        assert_eq!(got.host, "example.test");
        assert_eq!(got.ipv4, Some(Ipv4Addr::new(1, 2, 3, 4)));
    }
}
