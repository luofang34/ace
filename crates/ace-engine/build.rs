//! Compile reproducible workspace and toolchain identity into execution evidence.

use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> io::Result<()> {
    generate_blocking()
}

fn generate_blocking() -> io::Result<()> {
    let manifest = PathBuf::from(required("CARGO_MANIFEST_DIR")?);
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .filter(|root| root.join("Cargo.lock").is_file())
        .unwrap_or(&manifest);
    let mut files = Vec::new();
    for directory in [root.join("src"), root.join("crates")] {
        if directory.is_dir() {
            collect_blocking(&directory, &mut files)?;
        }
    }
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "rust-toolchain.toml",
    ] {
        let path = root.join(name);
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    let mut source = Sha256::new();
    let mut directives = io::stdout().lock();
    for path in &files {
        writeln!(directives, "cargo:rerun-if-changed={}", path.display())?;
        source.update(
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .as_bytes(),
        );
        source.update([0]);
        source.update(fs::read(path)?);
        source.update([0]);
    }
    for directory in [root.join("src"), root.join("crates")] {
        writeln!(directives, "cargo:rerun-if-changed={}", directory.display())?;
    }
    let parts = build_parts_blocking(root, hex::encode(source.finalize()))?;
    let mut fingerprint = Sha256::new();
    for part in &parts {
        fingerprint.update(part.as_bytes());
        fingerprint.update([0]);
    }
    let digest = format!("sha256:{}", hex::encode(fingerprint.finalize()));
    let generated = format!(
        "const BUILD_PARTS: [&str; 5] = {:?};\nconst BUILD_DIGEST: &str = {:?};\n",
        parts, digest
    );
    fs::write(
        PathBuf::from(required("OUT_DIR")?).join("implementation.rs"),
        generated,
    )
}

fn required(key: &str) -> io::Result<String> {
    env::var(key).map_err(|error| io::Error::other(format!("build configuration {key}: {error}")))
}

fn collect_blocking(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if entry.file_name() != "target" && entry.file_name() != ".git" {
                collect_blocking(&entry.path(), files)?;
            }
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn build_parts_blocking(root: &Path, source_digest: String) -> io::Result<[String; 5]> {
    let compiler = Command::new(required("RUSTC")?).arg("-vV").output()?;
    if !compiler.status.success() {
        return Err(io::Error::other("cannot identify Rust compiler"));
    }
    let compiler = String::from_utf8(compiler.stdout).map_err(io::Error::other)?;
    let lock = root.join("Cargo.lock");
    let dependencies = if lock.is_file() {
        hex::encode(Sha256::digest(fs::read(lock)?))
    } else {
        "unavailable".into()
    };
    let target = required("TARGET")?;
    let profile = required("PROFILE")?;
    let flags = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let mut features: Vec<_> = env::vars()
        .filter(|(key, _)| key.starts_with("CARGO_FEATURE_"))
        .collect();
    features.sort();
    let options = format!(
        "profile={profile};opt={};debug={};flags={flags};features={features:?}",
        required("OPT_LEVEL")?,
        required("DEBUG")?
    );
    Ok([source_digest, dependencies, compiler, target, options])
}
