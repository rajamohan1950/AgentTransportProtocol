fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_files = &[
        "../../proto/atp/v1/common.proto",
        "../../proto/atp/v1/identity.proto",
        "../../proto/atp/v1/handshake.proto",
        "../../proto/atp/v1/context.proto",
        "../../proto/atp/v1/routing.proto",
        "../../proto/atp/v1/fault.proto",
        "../../proto/atp/v1/task.proto",
        "../../proto/atp/v1/service.proto",
    ];

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(proto_files, &["../../proto"])?;

    Ok(())
}
