extern crate capnp;
#[macro_use]
extern crate capnp_rpc;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

pub mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/echo_capnp.rs"));
}

pub mod server {
    use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
    use futures::{Future, Stream};
    use tokio_io::AsyncRead;
    use echo_capnp::echo;

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

        let connections = socket.incoming();
        let server = connections.for_each(|(stream, _addr)| {
            let handle = handle.clone();
            let echo_server = echo_server.clone();
            let (reader, writer) = stream.split();

            let network = twoparty::VatNetwork::new(
                reader,
                writer,
                rpc_twoparty_capnp::Side::Server,
                Default::default(),
            );

            let rpc_system = RpcSystem::new(Box::new(network), Some(echo_server.client));
            handle.spawn(rpc_system.map_err(|e| println!("rpc error: {:?}", e)));
            Ok(())
        });
        core.run(server).unwrap();
    }
}

pub mod client {
    use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
    use echo_capnp::echo;
    use capnp::capability::Promise;

    use futures::Future;
    use tokio_io::AsyncRead;

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

        let socket = ::tokio_core::net::TcpStream::connect(&addr, &handle);
        let stream = core.run(socket).unwrap();
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
