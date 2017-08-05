extern crate capnp;
#[macro_use]
extern crate capnp_rpc;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate native_tls;
extern crate tokio_tls;

pub mod echo_capnp;
mod server;
mod client;

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