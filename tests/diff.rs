use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netboard::diff::{diff, Highlight, Row};
use netboard::{Dir, Endpoint, EndpointKey, IpVer, Proto, TcpState};

fn tcp(pid: u32, lport: u16, rport: u16, state: TcpState, dir: Dir) -> Endpoint {
    Endpoint {
        key: EndpointKey {
            pid,
            proto: Proto::Tcp,
            ip_ver: IpVer::V4,
            local: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), lport),
            remote: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), rport),
        },
        state: Some(state),
        dir,
        process: "t".into(),
        path: String::new(),
    }
}

fn listen(pid: u32, lport: u16) -> Endpoint {
    Endpoint {
        key: EndpointKey {
            pid,
            proto: Proto::Tcp,
            ip_ver: IpVer::V4,
            local: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), lport),
            remote: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        },
        state: Some(TcpState::Listen),
        dir: Dir::Listen,
        process: "t".into(),
        path: String::new(),
    }
}

#[test]
fn empty_to_established_out_is_new_out() {
    let next = vec![tcp(1, 50000, 443, TcpState::Established, Dir::Out)];
    let rows = diff(&[], &next);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].highlight, Highlight::NewOut);
}

#[test]
fn empty_to_listen_is_new_in() {
    let next = vec![listen(1, 80)];
    let rows = diff(&[], &next);
    assert_eq!(rows[0].highlight, Highlight::NewIn);
}

#[test]
fn established_to_close_wait_is_changed() {
    let a = tcp(1, 50000, 443, TcpState::Established, Dir::Out);
    let prev = diff(&[], &[a.clone()]);
    let mut b = a;
    b.state = Some(TcpState::CloseWait);
    let rows = diff(&prev, &[b]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].highlight, Highlight::Changed);
}

#[test]
fn gone_lingers_two_ticks_then_drops() {
    let a = tcp(1, 50000, 443, TcpState::Established, Dir::Out);
    let t1 = diff(&[], &[a]);
    assert_eq!(t1[0].highlight, Highlight::NewOut);

    let live = diff(&t1, &[t1[0].endpoint.clone()]);
    assert_eq!(live[0].highlight, Highlight::None);
    assert_eq!(live[0].linger, 0);

    let t2 = diff(&live, &[]);
    assert_eq!(t2.len(), 1);
    assert_eq!(t2[0].highlight, Highlight::Deleted);
    assert_eq!(t2[0].linger, 2);

    let t3 = diff(&t2, &[]);
    assert_eq!(t3.len(), 1);
    assert_eq!(t3[0].highlight, Highlight::Deleted);
    assert_eq!(t3[0].linger, 1);

    let t4 = diff(&t3, &[]);
    assert!(t4.is_empty());
}

#[test]
fn deleted_key_that_returns_is_new_not_changed() {
    let a = tcp(1, 50000, 443, TcpState::Established, Dir::Out);
    let t1 = diff(&[], &[a.clone()]);
    let t2 = diff(&t1, &[]);
    assert_eq!(t2[0].highlight, Highlight::Deleted);
    let t3 = diff(&t2, &[a]);
    assert_eq!(t3.len(), 1);
    assert_eq!(t3[0].highlight, Highlight::NewOut);
}

#[test]
fn expired_deleted_rows_do_not_underflow() {
    let expired = Row {
        endpoint: tcp(1, 50000, 443, TcpState::Established, Dir::Out),
        highlight: Highlight::Deleted,
        linger: 0,
    };
    assert!(diff(&[expired], &[]).is_empty());
}

#[test]
fn unknown_direction_has_no_outbound_flash_but_still_lingers() {
    let mut e = tcp(1, 50000, 0, TcpState::Established, Dir::Unknown);
    e.key.proto = Proto::Udp;
    e.state = None;
    let rows = diff(&[], &[e]);
    assert_eq!(rows[0].highlight, Highlight::None);
    assert_eq!(rows[0].endpoint.dir_label(), "Unknown");
    let gone = diff(&rows, &[]);
    assert_eq!(gone[0].highlight, Highlight::Deleted);
    assert_eq!(gone[0].linger, 2);
}
