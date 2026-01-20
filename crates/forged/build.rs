fn main() {
    // Re-run if protos change
    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=proto/api_v2.proto");

    tonic_build::configure()
        // Enable serde on messages for convenience
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_well_known_types(true)
        .build_server(true)
        .build_client(false) // Server doesn't need client
        .compile(&["proto/api_v2.proto"], &["proto"])
        .expect("failed to compile protos");
}
