// SPDX-License-Identifier: AGPL-3.0-only
//! Embeds every `src/Builds/Edges/<class>.toml` as `EdgeFiles.rs`, so adding a class edge is one new file.

use std::{env, fs, path::PathBuf};

fn main() {
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"))
        .join("src/Builds/Edges");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("Edges directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let mut out = String::from("const EDGE_FILES: &[(&str, &str)] = &[\n");
    for p in files {
        let class = p.file_stem().expect("stem").to_string_lossy();
        out.push_str(&format!(
            "    (\"{class}\", include_str!({:?})),\n",
            p.display().to_string()
        ));
    }
    out.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").expect("out dir")).join("EdgeFiles.rs"),
        out,
    )
    .expect("write EdgeFiles.rs");
}
