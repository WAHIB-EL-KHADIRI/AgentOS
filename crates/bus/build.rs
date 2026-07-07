fn main() {
    // Proto types are manually defined in grpc.rs with prost annotations
    // to avoid protoc dependency at build time.
    // Keep the proto file as the source of truth for the wire format.
    println!("cargo:rerun-if-changed=proto/agent_bus.proto");
    println!("cargo:rerun-if-changed=src/grpc.rs");
}
