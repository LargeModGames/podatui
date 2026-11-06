use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: [&str; 7] = [
    "windows",
    "windows_sys",
    "bluer",
    "dbus",
    "tokio",
    "rusqlite",
    "serde",
];

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn manifest_has_no_os_dependencies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let mut in_deps = false;
    for line in manifest.lines().map(str::trim) {
        let mut entry = line;
        if line.starts_with('[') {
            let header = line.split_once("dependencies");
            in_deps = header.is_some();
            entry = header.map_or("", |(_, rest)| rest);
        }
        if !in_deps || entry.starts_with('#') {
            continue;
        }
        for token in entry.split(|c| !is_ident(c)) {
            let name = token.replace('-', "_");
            assert!(
                !FORBIDDEN.contains(&name.as_str()),
                "Cargo.toml depends on {name}: {line}"
            );
        }
    }
}

#[test]
fn sources_use_no_os_crates() {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(!files.is_empty());
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for name in FORBIDDEN {
            let path = format!("{name}::");
            for (i, _) in text.match_indices(&path) {
                assert!(
                    text[..i].chars().next_back().is_some_and(is_ident),
                    "{} uses {path}",
                    file.display()
                );
            }
        }
    }
}
