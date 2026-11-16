//! A bare katnav binary, the way `cargo install` leaves it, cannot load CEF
//! where it stands. It lays out a bundle beside the CEF build it was
//! compiled against and runs from there.

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Error;

const CEF: &str = env!("KATNAV_CEF_DIR");

/// Return when this executable already sits beside CEF, and otherwise run
/// katnav from its bundle, which only returns on failure.
pub(crate) fn run_beside_cef() -> Result<(), Error> {
    let executable = std::env::current_exe().map_err(Error::Executable)?;
    if beside_cef(&executable) {
        return Ok(());
    }
    if !Path::new(CEF).exists() {
        return Err(Error::CefGone(PathBuf::from(CEF)));
    }
    let main = refresh(&executable).map_err(Error::Bundle)?;
    let failure = Command::new(main).args(std::env::args_os().skip(1)).exec();
    Err(Error::Bundle(failure))
}

#[cfg(target_os = "macos")]
fn beside_cef(executable: &Path) -> bool {
    executable
        .parent()
        .is_some_and(|dir| dir.join("../Frameworks").join(FRAMEWORK).exists())
}

#[cfg(not(target_os = "macos"))]
fn beside_cef(executable: &Path) -> bool {
    executable
        .parent()
        .is_some_and(|dir| dir.join("icudtl.dat").exists())
}

/// The bundle's main executable, laid out again whenever this binary
/// differs from the copy inside it.
fn refresh(executable: &Path) -> io::Result<PathBuf> {
    let bundle = crate::engine::data_dir().join(BUNDLE);
    let main = bundle.join(MAIN);
    if fs::read(&main).ok() == Some(fs::read(executable)?) {
        return Ok(main);
    }
    let partial = bundle.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);
    lay_out(&partial, executable)?;
    let _ = fs::remove_dir_all(&bundle);
    fs::rename(&partial, &bundle)?;
    Ok(main)
}

#[cfg(target_os = "macos")]
const FRAMEWORK: &str = "Chromium Embedded Framework.framework";
#[cfg(target_os = "macos")]
const BUNDLE: &str = "katnav.app";
#[cfg(target_os = "macos")]
const MAIN: &str = "Contents/MacOS/katnav";

#[cfg(target_os = "macos")]
fn lay_out(bundle: &Path, executable: &Path) -> io::Result<()> {
    let contents = bundle.join("Contents");
    let frameworks = contents.join("Frameworks");
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::create_dir_all(&frameworks)?;
    fs::copy(executable, bundle.join(MAIN))?;
    fs::write(
        contents.join("Info.plist"),
        plist("katnav", "com.h4ks.katnav"),
    )?;
    symlink(Path::new(CEF).join(FRAMEWORK), frameworks.join(FRAMEWORK))?;
    for kind in ["", " (GPU)", " (Renderer)", " (Plugin)", " (Alerts)"] {
        let name = format!("katnav Helper{kind}");
        let helper = frameworks.join(format!("{name}.app/Contents"));
        fs::create_dir_all(helper.join("MacOS"))?;
        fs::hard_link(bundle.join(MAIN), helper.join("MacOS").join(&name))?;
        let id: String = format!("helper{kind}")
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();
        let id = format!("com.h4ks.katnav.{}", id.to_lowercase());
        fs::write(helper.join("Info.plist"), plist(&name, &id))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn plist(name: &str, id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>{name}</string>
<key>CFBundleIdentifier</key><string>{id}</string>
<key>CFBundleName</key><string>{name}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
"#
    )
}

#[cfg(not(target_os = "macos"))]
const BUNDLE: &str = "bundle";
#[cfg(not(target_os = "macos"))]
const MAIN: &str = "katnav";

#[cfg(not(target_os = "macos"))]
fn lay_out(bundle: &Path, executable: &Path) -> io::Result<()> {
    fs::create_dir_all(bundle)?;
    for entry in fs::read_dir(CEF)? {
        let entry = entry?;
        symlink(entry.path(), bundle.join(entry.file_name()))?;
    }
    fs::copy(executable, bundle.join(MAIN))?;
    Ok(())
}
