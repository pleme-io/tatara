fn main() {
    #[cfg(feature = "grpc")]
    {
        tonic_build::compile_protos("../proto/tatara/v1/tatara.proto")
            .expect("Failed to compile protobuf");
    }
}
