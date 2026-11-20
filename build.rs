use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn validate_man_page(path: &Path) {
    if Command::new("groff")
        .args(["-man", "-Tutf8"])
        .arg(path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(true)
    {
        return;
    }
    println!(
        "cargo:warning=groff reported issues while rendering {}",
        path.display()
    );
}

fn copy_man_page(src: &Path, dst: &Path) -> bool {
    let Some(parent) = dst.parent() else {
        return false;
    };
    if fs::create_dir_all(parent).is_err() {
        return false;
    }
    fs::copy(src, dst).is_ok()
}

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_INSTALL_ROOT");

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|_| "target".to_string()));

    let man_pages: [(&str, &str); 4] = [
        ("docs/catpm.1", "man1/catpm.1"),
        ("docs/cpkg.1", "man1/cpkg.1"),
        ("docs/catpm-config.5", "man5/catpm-config.5"),
        ("docs/catpm-repository.5", "man5/catpm-repository.5"),
    ];

    let install_root = env::var("CARGO_INSTALL_ROOT").ok().filter(|s| !s.trim().is_empty());

    for (src_rel, dst_rel) in man_pages {
        let src = manifest_dir.join(src_rel);
        println!("cargo:rerun-if-changed={}", src.display());

        if !src.exists() {
            continue;
        }

        validate_man_page(&src);

        let out_dst = out_dir.join("man").join(dst_rel);
        let _ = copy_man_page(&src, &out_dst);

        if let Some(ref root) = install_root {
            let target = Path::new(root).join("share/man").join(dst_rel);
            if copy_man_page(&src, &target) {
                println!(
                    "cargo:warning=Installed man page {} -> {}",
                    src_rel,
                    target.display()
                );
            }
        }
    }
}
