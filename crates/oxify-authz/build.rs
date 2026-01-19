fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "grpc")]
    {
        tonic_prost_build::configure()
            .protoc_arg("--experimental_allow_proto3_optional")
            .build_server(true)
            .build_client(true)
            .compile_protos(&["proto/authz.proto"], &["proto"])?;
    }
    Ok(())
}
