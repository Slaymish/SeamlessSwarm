fn main() {
    prost_build::compile_protos(&["../../proto/topology.proto"], &["../../proto/"]).unwrap();
}
