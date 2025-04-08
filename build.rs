use std::{env, fs, path::PathBuf, str::FromStr};

use cargo_metadata::MetadataCommand;

const TEMPLATE_FILE: &str = "README.template.md";
const README_FILE: &str = "README.md";

fn main() {
    let metadata = MetadataCommand::new().exec().unwrap();

    // Get version from Cargo.toml
    let crate_version =
        env::var("CARGO_PKG_VERSION").expect("environment variable CARGO_PKG_VERSION not found");

    let rfs_test_macro_version = metadata
        .packages
        .iter()
        .find(|p| p.name == "rfs_test_macro")
        .map(|p| p.version.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Path to source template
    let src_path =
        PathBuf::from_str("README.template.md").expect("incorrect filename for README.template.md");

    // PATH to destination file
    let dst_path = PathBuf::from_str("README.md")
        .expect(format!("incorrect filename for {README_FILE}").as_str());

    // reading the source template file
    let content = fs::read_to_string(src_path)
        .expect(format!("Failed to read template {TEMPLATE_FILE}").as_str());

    // Replace placeholders in template
    let new_content = content
        .replace("{{CRATE_VERSION}}", &crate_version)
        .replace("{{RFS_TEST_MACRO_VERSION}}", &rfs_test_macro_version);

    // Write processed readme content to file
    fs::write(&dst_path, new_content)
        .expect(format!("Failed to write processed {README_FILE} file").as_str());

    // Rerun cargo again if template was changed
    println!("cargo::rerun-if-changed={TEMPLATE_FILE}");
}
