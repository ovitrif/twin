//! Twin.app's own helpers: the mirrors and CLIs inside its bundle, found in
//! the running app's bundle first, else in the installed app (for the CLI and
//! dev builds).

use std::path::{Path, PathBuf};

use crate::command_path::is_executable_file;

/// Where Twin.app is installed
pub const APP_BUNDLE_PATH: &str = "/Applications/Twin.app";

/// Where Twin.app may be installed: `/Applications`, else `~/Applications`
/// (install.sh's fallback when `/Applications` is not writable)
pub fn installed_bundles() -> Vec<PathBuf> {
    let mut bundles = vec![PathBuf::from(APP_BUNDLE_PATH)];
    if let Some(home) = std::env::var_os("HOME") {
        bundles.push(PathBuf::from(home).join("Applications/Twin.app"));
    }
    bundles
}

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
        .map(Path::to_path_buf)
        .chain(installed_bundles())
        .map(|bundle| bundle.join(relative))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_in_the_running_bundle_then_both_applications_folders() {
        let exe = Path::new("/tmp/Dev/Twin.app/Contents/MacOS/twin-app");
        let mut expected = vec![
            PathBuf::from("/tmp/Dev/Twin.app/Contents/Helpers/dmb"),
            PathBuf::from("/Applications/Twin.app/Contents/Helpers/dmb"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            expected.push(PathBuf::from(home).join("Applications/Twin.app/Contents/Helpers/dmb"));
        }

        assert_eq!(candidates(Some(exe), "Contents/Helpers/dmb"), expected);
    }
}
