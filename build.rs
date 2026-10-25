use std::path::Path;
use std::process::Command;

fn main() {
    glib_build_tools::compile_resources(
        &["data/resources", "data"],
        "data/resources/resources.gresource.xml",
        "sluis.gresource",
    );

    // For `cargo run`: compile the GSettings schema where .cargo/config.toml
    // points GSETTINGS_SCHEMA_DIR. Installed builds use the system schema dir.
    let schemas = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/gschemas");
    std::fs::create_dir_all(&schemas).expect("cannot create target/gschemas");
    let status = Command::new("glib-compile-schemas")
        .args(["--strict", "--targetdir"])
        .arg(&schemas)
        .arg("data")
        .status()
        .expect("glib-compile-schemas not found (install glib2-devel)");
    assert!(status.success(), "glib-compile-schemas failed");
    println!("cargo::rerun-if-changed=data/io.github.rayytor.Sluis.gschema.xml");
}
