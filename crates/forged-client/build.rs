fn main() {
    // Re-run if protos change in the server crate
    println!("cargo:rerun-if-changed=../forged/proto");
    println!("cargo:rerun-if-changed=../forged/proto/gate.proto");
    println!("cargo:rerun-if-changed=../forged/proto/component.proto");
    println!("cargo:rerun-if-changed=../forged/proto/auth.proto");
    println!("cargo:rerun-if-changed=../forged/proto/git.proto");

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
        .expect("failed to compile protos for client");
}
