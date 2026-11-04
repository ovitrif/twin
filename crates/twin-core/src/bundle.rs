//! Twin.app's own helpers: the mirrors and CLIs inside its bundle, found in
//! the running app's bundle first, else in the installed app (for the CLI and
//! dev builds).

use std::path::{Path, PathBuf};

use crate::command_path::is_executable_file;

/// Where Twin.app is installed
pub const APP_BUNDLE_PATH: &str = "/Applications/Twin.app";

/// The first executable `relative` path (inside a bundle) of the running
/// app's bundle, then of the installed app
pub fn find(relative: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    candidates(exe.as_deref(), relative)
        .into_iter()
        .find(|path| is_executable_file(path))
}

pub fn candidates(exe: Option<&Path>, relative: &str) -> Vec<PathBuf> {
    // The menu bar app runs from <bundle>/Contents/MacOS/<binary>
    let running_bundle = exe
        .and_then(|exe| exe.parent()?.parent()?.parent())
        .filter(|bundle| bundle.extension().is_some_and(|ext| ext == "app"));

    running_bundle
        .into_iter()
        .map(|bundle| bundle.join(relative))
        .chain([Path::new(APP_BUNDLE_PATH).join(relative)])
        .collect()
}
