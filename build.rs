//! Keeps the CEF build katnav compiled against where the binary can find it
//! at run time, so a bare `cargo install` runs.

use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-env-changed=DEP_CEF_DLL_WRAPPER_CEF_DIR");
    println!("cargo::rerun-if-env-changed=XDG_DATA_HOME");
    let built = PathBuf::from(env::var("DEP_CEF_DLL_WRAPPER_CEF_DIR")?);
    let out = PathBuf::from(env::var("OUT_DIR")?);
    // cef-dll-sys downloads into the target directory when CEF_PATH is
    // unset, and cargo install deletes that, so we keep our own copy
    let in_target = out
        .ancestors()
        .nth(4)
        .is_some_and(|target| built.starts_with(target));
    let os = env::var("CARGO_CFG_TARGET_OS")?;
    // windows loads libcef.dll before main runs, so katnav runs only from
    // its release folder and a copy would serve nothing
    let cef = if in_target && os != "windows" {
        keep(&built)?
    } else {
        built
    };
    println!("cargo::rustc-env=KATNAV_CEF_DIR={}", cef.display());
    if os == "linux" {
        println!(
            "cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN:{}",
            cef.display()
        );
    }
    Ok(())
}

/// Copy the CEF build into katnav's data directory, once per CEF version.
fn keep(built: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let kept = data_dir()?.join("cef").join(version(built)?);
    if kept.exists() {
        return Ok(kept);
    }
    let partial = kept.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(partial.parent().ok_or("CEF has no parent directory")?)?;
    run(Command::new("cp").arg("-R").arg(built).arg(&partial))?;
    let library = partial.join("libcef.so");
    if library.exists() {
        // the linux libcef ships its debug symbols, about 2 GB of the 2.5
        run(Command::new("strip").arg("--strip-unneeded").arg(library))?;
    }
    fs::rename(&partial, &kept)?;
    Ok(kept)
}

fn data_dir() -> Result<PathBuf, Box<dyn Error>> {
    let base = match env::var_os("XDG_DATA_HOME") {
        Some(data) => PathBuf::from(data),
        None => PathBuf::from(env::var_os("HOME").ok_or("HOME is unset")?).join(".local/share"),
    };
    Ok(base.join("katnav"))
}

/// The full CEF version, like `156.0.28+g57389f0+chromium-156.0.8078.88`.
fn version(built: &Path) -> Result<String, Box<dyn Error>> {
    let header = fs::read_to_string(built.join("include/cef_version.h"))?;
    header
        .lines()
        .find_map(|line| line.strip_prefix("#define CEF_VERSION \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .map(str::to_owned)
        .ok_or_else(|| "cef_version.h names no CEF_VERSION".into())
}

fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
    if command.status()?.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed").into())
    }
}
