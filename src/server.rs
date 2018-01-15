use std::sync::Arc;
use capnp_rpc::{RpcSystem, twoparty, rpc_twoparty_capnp};
use futures::{Future, Stream};
use tokio_io::AsyncRead;
use echo_capnp::echo;

use rustls::{ ServerConfig, RootCertStore, Session };
use rustls::AllowAnyAuthenticatedClient;
use tokio_rustls::ServerConfigExt;

use openssl::x509::X509;

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

    let echo_server = echo::ToClient::new(Echo {}).from_server::<::capnp_rpc::Server>();

    let mut client_auth_roots = RootCertStore::empty();
    let roots = ::load_certs("test-ca/rsa/end.fullchain");
    for root in &roots {
         client_auth_roots.add(&root).unwrap();
    }
    let client_auth = AllowAnyAuthenticatedClient::new(client_auth_roots);

    let mut config = ServerConfig::new(client_auth);
    config.set_single_cert(roots, ::load_private_key("test-ca/rsa/end.key"));
    let config = Arc::new(config);

    let connections = socket.incoming();

    let tls_handshake = connections.map(|(socket, _addr)| {
        socket.set_nodelay(true).unwrap();
        config.accept_async(socket)
    });

    let server = tls_handshake.map(|acceptor| {        
        let handle = handle.clone();
        let echo_server = echo_server.clone();
        acceptor.and_then(move |socket| {
            let email = 
            {                
                let ( _, session ) = socket.get_ref();
                let mut email : Option<String> = None;
                if let Some(certs) = session.get_peer_certificates()                 {
                    for cert in certs {
                        let x509 = X509::from_der(&cert.0).unwrap();
                        if let Some(sans) = x509.subject_alt_names() {
                            for san in sans {
                                if let Some(e) = san.email() {
                                    email = Some(e.to_owned());
                                    break;
                                }
                            }
                        }
                    }
                }
                email
            };
            println!("email: {:?}", email);

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
