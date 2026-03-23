use std::io::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    // Use vendored protoc
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("protoc not found");
    std::env::set_var("PROTOC", protoc);
    
    // Get output directory
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    
    // Compile protobuf files
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .file_descriptor_set_path(out_dir.join("wootype.bin"))
        .compile_protos(&["proto/wootype.proto"], &["proto"])?;
    
    // Re-run if proto files change
    println!("cargo:rerun-if-changed=proto/wootype.proto");
    
    Ok(())
}
