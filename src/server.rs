use std;
use std::io::Read;
use capnp_rpc::{RpcSystem, twoparty, rpc_twoparty_capnp};
use futures::{Future, Stream};
use tokio_io::AsyncRead;
use echo_capnp::echo;

use native_tls::Pkcs12;
use native_tls::TlsAcceptor;
use tokio_tls::TlsAcceptorExt;

fn load_pkcs12(filename: &str) -> Result<Pkcs12, ()> {
    let mut file = std::fs::File::open(filename).unwrap();
    let mut pkcs12 = vec![];
    file.read_to_end(&mut pkcs12).unwrap();
    let pkcs12 = Pkcs12::from_der(&pkcs12, "password").unwrap();
    Ok(pkcs12)
}

struct Echo;

impl echo::Server for Echo {
    fn echo(
        &mut self,
        params: echo::EchoParams,
        mut results: echo::EchoResults,
    ) -> ::capnp::capability::Promise<(), ::capnp::Error> {
        let input = pry!(pry!(params.get()).get_input());
        results.get().set_output(input);
        ::capnp::capability::Promise::ok(())
    }
}

pub fn main() {
    use std::net::ToSocketAddrs;
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} server HOST:PORT", args[0]);
        return;
    }

    let mut core = ::tokio_core::reactor::Core::new().unwrap();
    let handle = core.handle();

    let addr = args[2]
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("could not parse address");
    let socket = ::tokio_core::net::TcpListener::bind(&addr, &handle).unwrap();

    let echo_server = echo::ToClient::new(Echo).from_server::<::capnp_rpc::Server>();

    let tls_acceptor = TlsAcceptor::builder(load_pkcs12("certificate.pfx").unwrap())
        .unwrap()
        .build()
        .unwrap();

    let connections = socket.incoming();

    let tls_handshake = connections.map(|(socket, _addr)| {
        socket.set_nodelay(true).unwrap();
        tls_acceptor.accept_async(socket)
    });

    let server = tls_handshake.map(|acceptor| {
        let handle = handle.clone();
        let echo_server = echo_server.clone();
        acceptor.and_then(move |socket| {
            let (reader, writer) = socket.split();

            let network = twoparty::VatNetwork::new(
                reader,
                writer,
                rpc_twoparty_capnp::Side::Server,
                Default::default(),
            );

            let rpc_system = RpcSystem::new(Box::new(network), Some(echo_server.client));
            handle.spawn(rpc_system.map_err(|e| println!("{}", e)));
            Ok(())
        })
    });
    core.run(server.for_each(|client| {
        handle.spawn(client.map_err(|e| println!("{}", e)));
        Ok(())
    })).unwrap();
}
