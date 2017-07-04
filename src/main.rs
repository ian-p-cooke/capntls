extern crate capnp;
#[macro_use]
extern crate capnp_rpc;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate native_tls;
extern crate tokio_tls;

pub mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/echo_capnp.rs"));
}

use native_tls::{Pkcs12,Certificate};
use std::io::Read;

fn load_pkcs12(filename: &str) -> Result<Pkcs12, ()> {
    let mut file = std::fs::File::open(filename).unwrap();
    let mut pkcs12 = vec![];
    file.read_to_end(&mut pkcs12).unwrap();
    let pkcs12 = Pkcs12::from_der(&pkcs12, "password").unwrap();
    Ok(pkcs12)
}

fn load_certificate(filename: &str) -> Result<Certificate, ()> {
    let mut file = std::fs::File::open(filename).unwrap();
    let mut bytes = vec![];
    file.read_to_end(&mut bytes).unwrap();
    let cert = Certificate::from_der(&bytes).unwrap();
    Ok(cert)
}

pub mod server {
    use capnp_rpc::{RpcSystem, twoparty, rpc_twoparty_capnp};
    use futures::{Future, Stream};
    use tokio_io::AsyncRead;
    use echo_capnp::echo;

    use native_tls::TlsAcceptor;
    use tokio_tls::TlsAcceptorExt;

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

        let tls_acceptor = TlsAcceptor::builder(::load_pkcs12("certificate.pfx").unwrap())
            .unwrap()
            .build()
            .unwrap();

        let connections = socket.incoming();

        let tls_handshake = connections.map(|(socket, _addr)| {
            println!("connected");
            socket.set_nodelay(true).unwrap();
            tls_acceptor.accept_async(socket).map_err(|e| {println!("error: {:?}", e)})
        });

        let server = tls_handshake.map(|acceptor| {
            let handle = handle.clone();
            let echo_server = echo_server.clone();
            println!("got acceptor");
            acceptor.and_then(move |socket| {
                println!("got socket.  splitting...");
                let (reader, writer) = socket.split();

                let network = twoparty::VatNetwork::new(
                    reader,
                    writer,
                    rpc_twoparty_capnp::Side::Server,
                    Default::default(),
                );

                let rpc_system = RpcSystem::new(Box::new(network), Some(echo_server.client));
                handle.spawn(rpc_system.map_err(|e| {
                    println!("rpc error: {:?}", e)
                }));
                Ok(())
            })
        });
        core.run(server.for_each(|and_then| {
            let next = and_then.map(|o| println!("o:{:?}",o)).map_err(|e| println!("e:{:?}", e));
            handle.spawn(next);
            Ok(())
        }
        )).unwrap();
    }
}

pub mod client {
    use capnp_rpc::{RpcSystem, twoparty, rpc_twoparty_capnp};
    use echo_capnp::echo;
    use capnp::capability::Promise;

    use futures::Future;
    use tokio_io::AsyncRead;

    use native_tls::TlsConnector;
    use tokio_tls::TlsConnectorExt;

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

        let ca_cert = ::load_certificate("certificate.der").unwrap();

        let socket = ::tokio_core::net::TcpStream::connect(&addr, &handle);
        let mut builder = TlsConnector::builder().unwrap();
        match builder.add_root_certificate(ca_cert) {
            Ok(_) => {},
            Err(e) => panic!("{:?}", e)
        }
        let cx = builder.build().unwrap();
        let tls_handshake = socket.and_then(|socket| {
            socket.set_nodelay(true).unwrap();
            cx.connect_async("localhost", socket).map_err(|e| {
                ::std::io::Error::new(::std::io::ErrorKind::Other, e)
            })
        });

        let stream = core.run(tls_handshake)
            .unwrap();
        let (reader, writer) = stream.split();

        let network = Box::new(twoparty::VatNetwork::new(
            reader,
            writer,
            rpc_twoparty_capnp::Side::Client,
            Default::default(),
        ));
        let mut rpc_system = RpcSystem::new(network, None);
        let echo_client: echo::Client = rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);
        handle.spawn(rpc_system.map_err(|e| println!("rpc error: {:?}", e)));

        let mut request = echo_client.echo_request();
        request.get().set_input("hello");
        try!(core.run(request.send().promise.and_then(|response| {
            let output = pry!(response.get()).get_output().unwrap();
            println!("{}", output);
            Promise::ok(())
        })));
        Ok(())
    }
}

pub fn main() {
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() >= 2 {
        match &args[1][..] {
            "client" => return client::main(),
            "server" => return server::main(),
            _ => (),
        }
    }

    println!("usage: {} [client | server] HOST:PORT", args[0]);
}
