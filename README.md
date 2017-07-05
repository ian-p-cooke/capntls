# capntls
rough proof of concept for asynchronous Cap'n Proto RPC over TLS

```
cargo build 
./target/debug/capntls server localhost:3276
./target/debug/capntls client localhost:3276

cargo build --examples
./target/debug/examples/no_tls server localhost:3277
./target/debug/examples/no_tls client localhost:3277
```
