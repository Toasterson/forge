fn main() {
    // Re-run if protos change in the server crate
    println!("cargo:rerun-if-changed=../forged/proto");
    println!("cargo:rerun-if-changed=../forged/proto/gate.proto");
    println!("cargo:rerun-if-changed=../forged/proto/component.proto");
    println!("cargo:rerun-if-changed=../forged/proto/auth.proto");
    println!("cargo:rerun-if-changed=../forged/proto/git.proto");
    println!("cargo:rerun-if-changed=../forged/proto/api_v2.proto");

    // Compile v1 protos
    tonic_build::configure()
        .compile_well_known_types(true)
        .compile(
            &[
                "../forged/proto/gate.proto",
                "../forged/proto/component.proto",
                "../forged/proto/auth.proto",
                "../forged/proto/git.proto",
            ],
            &["../forged/proto"],
        )
        .expect("failed to compile v1 protos for client");

    // Compile v2 protos (requires proto3 optional support)
    tonic_build::configure()
        .compile_well_known_types(true)
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile(&["../forged/proto/api_v2.proto"], &["../forged/proto"])
        .expect("failed to compile v2 protos for client");
}
