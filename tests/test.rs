#![allow(dead_code, unused_imports)]

extern crate shadowsocks;
extern crate tokio_core;
extern crate futures;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::thread;
use std::net::SocketAddr;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use tokio_core::reactor::Core;
use tokio_core::io::{read_to_end, write_all, flush};
use futures::Future;

use shadowsocks::relay::tcprelay::client::Socks5Client;
use shadowsocks::config::{Config, ServerConfig};
use shadowsocks::crypto::CipherType;
use shadowsocks::relay::{RelayLocal, RelayServer};
use shadowsocks::relay::socks5::{Address, UdpAssociateHeader};

const SERVER_ADDR: &'static str = "127.0.0.1:8092";
const LOCAL_ADDR: &'static str = "127.0.0.1:8290";

const UDP_ECHO_SERVER_ADDR: &'static str = "127.0.0.1:50403";
const UDP_LOCAL_ADDR: &'static str = "127.0.0.1:9011";

const PASSWORD: &'static str = "test-password";
const METHOD: CipherType = CipherType::Aes128Cfb;

fn get_config() -> Config {
    let mut cfg = Config::new();
    cfg.local = Some(LOCAL_ADDR.parse().unwrap());
    cfg.server = vec![ServerConfig::basic(SERVER_ADDR.parse().unwrap(), PASSWORD.to_owned(), METHOD)];
    cfg.enable_udp = true;
    cfg
}

fn get_client_addr() -> SocketAddr {
    LOCAL_ADDR.parse().unwrap()
}

fn start_server(bar: Arc<Barrier>) {
    thread::spawn(move || {
        drop(env_logger::init());
        bar.wait();
        RelayServer::run(get_config()).unwrap();
    });
}

fn start_local(bar: Arc<Barrier>) {
    thread::spawn(move || {
        drop(env_logger::init());
        bar.wait();
        RelayLocal::run(get_config()).unwrap();
    });
}

fn start_udp_echo_server(bar: Arc<Barrier>) {
    use std::net::UdpSocket;

    thread::spawn(move || {
        drop(env_logger::init());

        let l = UdpSocket::bind(UDP_ECHO_SERVER_ADDR).unwrap();

        bar.wait();

        let mut buf = [0u8; 65536];
        let (amt, src) = l.recv_from(&mut buf).unwrap();

        l.send_to(&buf[..amt], &src).unwrap();
    });
}

#[test]
fn socks5_relay() {
    drop(env_logger::init());

    let bar = Arc::new(Barrier::new(3));

    start_server(bar.clone());
    start_local(bar.clone());

    bar.wait();

    // Wait until all server starts
    thread::sleep(Duration::from_secs(1));

    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let c = Socks5Client::connect(Address::DomainNameAddress("www.example.com".to_owned(), 80),
                                  get_client_addr(),
                                  handle);
    let fut = c.and_then(|c| {
        let req = b"GET / HTTP/1.0\r\nHost: www.example.com\r\nAccept: */*\r\n\r\n";
        write_all(c, req.to_vec())
            .and_then(|(c, _)| flush(c))
            .and_then(|c| read_to_end(c, Vec::new()))
            .map(|(_, buf)| {
                println!("Got reply from server: {}", String::from_utf8(buf).unwrap());
            })
    });

    lp.run(fut).unwrap();
}

fn start_udp_request_holder(bar: Arc<Barrier>, addr: Address) {
    thread::spawn(move || {
        let mut lp = Core::new().unwrap();
        let handle = lp.handle();

        let c = Socks5Client::udp_associate(addr, get_client_addr(), handle);
        let fut = c.and_then(|(c, addr)| {
            assert_eq!(addr, Address::SocketAddress(LOCAL_ADDR.parse().unwrap()));

            // Holds it forever
            read_to_end(c, Vec::new()).map(|_| ())
        });

        bar.wait();

        lp.run(fut).unwrap();
    });
}

// #[test]
// fn udp_relay() {
//     use std::net::UdpSocket;

//     let remote_addr = Address::SocketAddress(UDP_ECHO_SERVER_ADDR.parse().unwrap());

//     let bar = Arc::new(Barrier::new(4));

//     start_server(bar.clone());
//     start_local(bar.clone());

//     start_udp_echo_server(bar.clone());

//     bar.wait();

//     // Wait until all server starts
//     thread::sleep(Duration::from_secs(1));

//     let bar = Arc::new(Barrier::new(2));

//     start_udp_request_holder(bar.clone(), remote_addr.clone());

//     bar.wait();

//     let l = UdpSocket::bind(UDP_LOCAL_ADDR).unwrap();

//     let mut buf = UdpAssociateHeader::new(0, remote_addr)
//         .write_to(Vec::new())
//         .wait()
//         .unwrap();

//     buf.extend_from_slice(b"Hello world");

//     let local_addr = LOCAL_ADDR.parse::<SocketAddr>().unwrap();
//     l.send_to(&buf[..], &local_addr).unwrap();
//     println!("Sent data to proxy");

//     let mut buf = [0u8; 65536];
//     let (amt, src) = l.recv_from(&mut buf).unwrap();
//     println!("{} {:?}", amt, src);
// }