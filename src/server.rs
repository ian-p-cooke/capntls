use std::sync::Arc;

use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use futures::{Future, Stream};
use tokio_io::AsyncRead;
use echo_capnp::echo;

use rustls::{RootCertStore, ServerConfig, Session};
use rustls::AllowAnyAuthenticatedClient;
use tokio_rustls::{ServerConfigExt, TlsStream};

use openssl::x509::X509;

struct Echo {
    email: String,
}

impl echo::Server for Echo {
    fn echo(
        &mut self,
        params: echo::EchoParams,
        mut results: echo::EchoResults,
    ) -> ::capnp::capability::Promise<(), ::capnp::Error> {
        let input = pry!(pry!(params.get()).get_input());
        results
            .get()
            .set_output(&format!("{}:{}", self.email, input));
        ::capnp::capability::Promise::ok(())
    }
}

fn get_email_from_stream<I, S: Session>(stream: &TlsStream<I, S>) -> Option<String> {
    let (_, session) = stream.get_ref();
    if let Some(certs) = session.get_peer_certificates() {
        for cert in certs {
            let x509 = X509::from_der(&cert.0).unwrap();
            if let Some(sans) = x509.subject_alt_names() {
                for san in sans {
                    if let Some(e) = san.email() {
                        return Some(e.to_owned());
                    }
                }
            }
        }
    }
    None
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
        acceptor.and_then(move |stream| {
            let email = get_email_from_stream(&stream);
            let echo = Echo {
                email: email.unwrap(),
            };
            let echo_client = echo::ToClient::new(echo).from_server::<::capnp_rpc::Server>();

            let (reader, writer) = stream.split();
            let network = twoparty::VatNetwork::new(
                reader,
                writer,
                rpc_twoparty_capnp::Side::Server,
                Default::default(),
            );

            let rpc_system = RpcSystem::new(Box::new(network), Some(echo_client.client));
            handle.spawn(rpc_system.map_err(|e| println!("{}", e)));
            Ok(())
        })
    });
    core.run(server.for_each(|client| {
        handle.spawn(client.map_err(|e| println!("{}", e)));
        Ok(())
    })).unwrap();
}
