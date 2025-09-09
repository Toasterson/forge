fn main() {
    // Re-run if protos change
    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=proto/gate.proto");
    println!("cargo:rerun-if-changed=proto/component.proto");
    println!("cargo:rerun-if-changed=proto/auth.proto");

    tonic_build::configure()
        // Enable serde on messages for convenience
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_well_known_types(true)
        .compile(
            &[
                "proto/gate.proto",
                "proto/component.proto",
                "proto/auth.proto",
            ],
            &["proto"],
        )
        .expect("failed to compile protos");
}
