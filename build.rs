extern crate capnpc;

fn main() {
    ::capnpc::CompilerCommand::new()
        .file("schema/echo.capnp")
        .run()
        .expect("compiling schema");
}
