use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

pub fn resolve_program(name: &str, override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        if is_executable_file(&path) {
            return Ok(path);
        }

        bail!(
            "{} was not found or is not executable at {}",
            name,
            path.display()
        );
    }

    find_on_path(name).with_context(|| {
        format!("{name} was not found. Install it and make sure it is available on PATH.")
    })
}

fn find_on_path(name: &str) -> Result<PathBuf> {
    if let Some(path_var) = env::var_os("PATH") {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if is_executable_file(&candidate) {
                return Ok(candidate);
            }
        }
    }

    // GUI launches (Finder, Open-at-Login) don't inherit the shell PATH, so the
    // menu bar app would otherwise miss Homebrew or Android SDK tools.
    for dir in standard_dirs() {
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return Ok(candidate);
        }
    }

    bail!("{name} not found on PATH")
}

/// PATH for a tool Twin starts: the user's own PATH first, so their
/// toolchains and stubs win, then `extra_dirs` and the standard locations a
/// launchd PATH lacks. The adb Twin resolved reaches scrcpy through `ADB`.
pub(crate) fn path_env_with_tool_dirs(
    extra_dirs: impl IntoIterator<Item = PathBuf>,
) -> Option<OsString> {
    join_tool_path(
        env::var_os("PATH"),
        extra_dirs.into_iter().chain(standard_dirs()),
    )
}

fn join_tool_path(
    user_path: Option<OsString>,
    tool_dirs: impl IntoIterator<Item = PathBuf>,
) -> Option<OsString> {
    let mut dirs = Vec::new();

    for dir in user_path.iter().flat_map(env::split_paths).chain(tool_dirs) {
        push_unique_dir(&mut dirs, dir);
    }

    env::join_paths(dirs).ok()
}

/// Common macOS and Linux install locations for adb, emulator, scrcpy and dmb, probed when PATH is the
/// minimal launchd environment rather than the user's shell.
fn standard_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];

    if let Some(home) = env::var_os("HOME") {
        dirs.extend(home_tool_dirs(Path::new(&home)));
    }

    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(sdk) = env::var_os(var) {
            dirs.extend(android_sdk_tool_dirs(&PathBuf::from(sdk)));
        }
    }

    dirs
}

fn home_tool_dirs(home: &Path) -> Vec<PathBuf> {
    // Where twin installs itself
    let mut dirs = vec![home.join(".local/bin")];
    // Android Studio's SDK: macOS, then Linux
    for sdk in ["Library/Android/sdk", "Android/Sdk"] {
        dirs.extend(android_sdk_tool_dirs(&home.join(sdk)));
    }
    dirs
}

fn android_sdk_tool_dirs(sdk: &Path) -> [PathBuf; 3] {
    [
        sdk.join("platform-tools"),
        sdk.join("emulator"),
        sdk.join("cmdline-tools/latest/bin"),
    ]
}

fn push_unique_dir(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dirs.contains(&dir) {
        dirs.push(dir);
    }
}

pub(crate) fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_dirs_include_common_macos_locations() {
        let dirs = standard_dirs();
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(dirs.contains(&PathBuf::from("/usr/local/bin")));
    }

    #[test]
    fn user_path_comes_before_tool_dirs() {
        let user_path = env::join_paths(["/home/me/bin", "/usr/bin"]).unwrap();
        let joined = join_tool_path(
            Some(user_path),
            [
                PathBuf::from("/sdk/platform-tools"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/bin"),
            ],
        )
        .unwrap();

        assert_eq!(
            env::split_paths(&joined).collect::<Vec<_>>(),
            [
                PathBuf::from("/home/me/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/sdk/platform-tools"),
                PathBuf::from("/opt/homebrew/bin"),
            ]
        );
    }

    #[test]
    fn home_dirs_include_the_macos_and_linux_android_sdks() {
        let dirs = home_tool_dirs(Path::new("/home/me"));

        assert!(dirs.contains(&PathBuf::from("/home/me/.local/bin")));
        assert!(dirs.contains(&PathBuf::from(
            "/home/me/Library/Android/sdk/platform-tools"
        )));
        assert!(dirs.contains(&PathBuf::from("/home/me/Android/Sdk/platform-tools")));
    }

    #[test]
    fn android_sdk_dirs_include_adb_emulator_and_command_line_tools() {
        assert_eq!(
            android_sdk_tool_dirs(Path::new("/sdk")),
            [
                PathBuf::from("/sdk/platform-tools"),
                PathBuf::from("/sdk/emulator"),
                PathBuf::from("/sdk/cmdline-tools/latest/bin")
            ]
        );
    }
}
