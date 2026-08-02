//! Embeds every manifest in `providers/` into the binary.
//!
//! Enumerating the directory at build time — rather than keeping a hand-written
//! list — is what makes "add an agent" a single-file change. A contributor drops
//! `providers/<agent>.toml` in place and it is compiled in, validated by the
//! registry tests, and shown by `agentlink providers` with no Rust edits at all.
//!
//! The directory lives *inside* this crate rather than at the workspace root.
//! `cargo publish` only packages files under the crate root — it has no
//! mechanism to pull in a workspace-sibling directory — so `providers/` living
//! two levels up would build fine from a git checkout but produce a tarball on
//! crates.io whose `build.rs` panics on `cargo install`. Keeping it here makes
//! the local workspace build and the published crate identical by construction.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let providers_dir = manifest_dir.join("providers");

    println!("cargo:rerun-if-changed={}", providers_dir.display());
    println!("cargo:rerun-if-changed=build.rs");

    let read = fs::read_dir(&providers_dir).unwrap_or_else(|err| {
        panic!("cannot read {}: {err}", providers_dir.display());
    });

    // Sorted so the generated file — and therefore the build — is reproducible.
    let manifests: BTreeSet<PathBuf> = read
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();

    assert!(
        !manifests.is_empty(),
        "no provider manifests found in {}",
        providers_dir.display()
    );

    let mut generated = String::from(
        "/// Provider manifests embedded from `providers/*.toml` at build time.\n\
         pub static BUILTIN_MANIFESTS: &[(&str, &str)] = &[\n",
    );
    for path in &manifests {
        println!("cargo:rerun-if-changed={}", path.display());

        let name = path
            .file_name()
            .expect("directory entries have names")
            .to_string_lossy();
        // Forward slashes are accepted by the Windows APIs and avoid having to
        // reason about escaping in the generated source.
        let include_path = path.to_string_lossy().replace('\\', "/");
        writeln!(generated, "    ({name:?}, include_str!({include_path:?})),")
            .expect("writing to a String cannot fail");
    }
    generated.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"))
        .join("builtin_manifests.rs");
    fs::write(&out, generated).unwrap_or_else(|err| {
        panic!("cannot write {}: {err}", out.display());
    });
}
