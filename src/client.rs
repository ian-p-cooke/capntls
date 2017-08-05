use std;
use std::io::Read;
use capnp_rpc::{RpcSystem, twoparty, rpc_twoparty_capnp};
use echo_capnp::echo;
use capnp::capability::Promise;

use futures::Future;
use tokio_io::AsyncRead;

use native_tls::Certificate;
use native_tls::TlsConnector;
use tokio_tls::TlsConnectorExt;

fn load_certificate(filename: &str) -> Result<Certificate, ()> {
    let mut file = std::fs::File::open(filename).unwrap();
    let mut bytes = vec![];
    file.read_to_end(&mut bytes).unwrap();
    let cert = Certificate::from_der(&bytes).unwrap();
    Ok(cert)
}

pub fn main() {
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} client HOST:PORT", args[0]);
        return;
    }

    try_main(args).unwrap();
}

fn try_main(args: Vec<String>) -> Result<(), ::capnp::Error> {
    use std::net::ToSocketAddrs;

    let mut core = try!(::tokio_core::reactor::Core::new());
    let handle = core.handle();

    let addr = try!(args[2].to_socket_addrs())
        .next()
        .expect("could not parse address");

    let ca_cert = load_certificate("certificate.der").unwrap();

    let socket = ::tokio_core::net::TcpStream::connect(&addr, &handle);
    let mut builder = TlsConnector::builder().unwrap();
    match builder.add_root_certificate(ca_cert) {
        Ok(_) => {}
        Err(e) => panic!("{:?}", e),
    }
    let cx = builder.build().unwrap();
    let tls_handshake = socket.and_then(|socket| {
        socket.set_nodelay(true).unwrap();
        cx.connect_async("localhost", socket)
            .map_err(|e| ::std::io::Error::new(::std::io::ErrorKind::Other, e))
    });

    let stream = core.run(tls_handshake).unwrap();
    let (reader, writer) = stream.split();

    let network = Box::new(twoparty::VatNetwork::new(
        reader,
        writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));
    let mut rpc_system = RpcSystem::new(network, None);
    let echo_client: echo::Client = rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);
    let rpc_disconnector = rpc_system.get_disconnector();
    handle.spawn(rpc_system.map_err(|e| println!("{}", e)));

    let mut request = echo_client.echo_request();
    request.get().set_input("hello");
    try!(core.run(request.send().promise.and_then(|response| {
        let output = pry!(response.get()).get_output().unwrap();
        println!("{}", output);
        Promise::ok(())
    })));

    try!(core.run(rpc_disconnector));
    Ok(())
}
