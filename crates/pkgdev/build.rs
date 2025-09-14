fn main() {
    // Re-run if the proto file changes
    println!("cargo:rerun-if-changed=../forged/proto/auth.proto");
    println!("cargo:rerun-if-changed=../forged/proto/gate.proto");
    println!("cargo:rerun-if-changed=../forged/proto/component.proto");

    // Compile the protobuf definitions for the client
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .out_dir(std::env::var("OUT_DIR").unwrap())
        .compile(
            &[
                "../forged/proto/auth.proto",
                "../forged/proto/gate.proto",
                "../forged/proto/component.proto",
            ],
            &["../forged/proto"],
        ) // input, include path
        .expect("failed to compile protos for pkgdev client");
}
