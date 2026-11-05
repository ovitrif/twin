//! iOS simulators that are running, through Xcode's `simctl`. Called by path
//! inside Xcode, never through `xcrun`, which on a Mac without developer
//! tools shows macOS's install prompt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::emulator::output_with_timeout;

const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Simulator {
    pub udid: String,
    pub name: String,
    /// "iOS 27.0", from the runtime identifier
    pub runtime: String,
}

#[derive(Deserialize)]
struct List {
    devices: BTreeMap<String, Vec<Entry>>,
}

#[derive(Deserialize)]
struct Entry {
    udid: String,
    name: String,
    state: String,
}

pub struct Simctl {
    path: PathBuf,
}

impl Simctl {
    /// simctl in the Xcode whose developer folder is `developer`
    /// (`Xcode.app/Contents/Developer`)
    pub fn in_developer_dir(developer: &Path) -> Option<Self> {
        let path = developer.join("usr/bin/simctl");
        path.is_file().then_some(Self { path })
    }

    /// The booted iOS simulators
    pub fn booted(&self) -> Result<Vec<Simulator>> {
        let mut command = Command::new(&self.path);
        command
            .args(["list", "devices", "booted", "-j"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // A wedged CoreSimulator service must not hold up the device list
        let output = output_with_timeout(command, LIST_TIMEOUT, "simctl list")?;
        if !output.status.success() {
            bail!("simctl list failed (exit {})", output.status);
        }
        parse_booted(&String::from_utf8_lossy(&output.stdout))
    }

    pub fn shutdown(&self, udid: &str) -> Result<()> {
        let mut command = Command::new(&self.path);
        command
            .args(["shutdown", udid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let output = output_with_timeout(command, SHUTDOWN_TIMEOUT, "simctl shutdown")?;
        if !output.status.success() {
            bail!("simctl shutdown {udid} failed (exit {})", output.status);
        }
        Ok(())
    }
}

/// Brings Simulator.app forward on `udid`'s window
pub fn show(udid: &str) -> Result<()> {
    let status = Command::new("/usr/bin/open")
        .args(["-a", "Simulator", "--args", "-CurrentDeviceUDID", udid])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("opening Simulator")?;
    if !status.success() {
        bail!("open -a Simulator failed (exit {status})");
    }
    Ok(())
}

pub fn parse_booted(json: &str) -> Result<Vec<Simulator>> {
    let list: List = serde_json::from_str(json).context("reading simctl list")?;
    Ok(list
        .devices
        .into_iter()
        .filter_map(|(runtime, entries)| Some((runtime_label(&runtime)?, entries)))
        .flat_map(|(runtime, entries)| {
            entries
                .into_iter()
                .filter(|entry| entry.state == "Booted")
                .map(move |entry| Simulator {
                    udid: entry.udid,
                    name: entry.name,
                    runtime: runtime.clone(),
                })
        })
        .collect())
}

/// "com.apple.CoreSimulator.SimRuntime.iOS-27-0" reads "iOS 27.0"; runtimes
/// other than iOS (watchOS, tvOS, visionOS) are not phones and give `None`
fn runtime_label(identifier: &str) -> Option<String> {
    let version = identifier.rsplit('.').next()?.strip_prefix("iOS-")?;
    Some(format!("iOS {}", version.replace('-', ".")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_booted_ios_simulators_only() {
        let json = r#"{"devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-27-0": [
                {"udid": "A", "name": "iPhone 17 Pro", "state": "Booted"},
                {"udid": "B", "name": "iPhone Air", "state": "Shutdown"}
            ],
            "com.apple.CoreSimulator.SimRuntime.watchOS-12-0": [
                {"udid": "C", "name": "Apple Watch", "state": "Booted"}
            ]
        }}"#;
        assert_eq!(
            parse_booted(json).unwrap(),
            vec![Simulator {
                udid: "A".into(),
                name: "iPhone 17 Pro".into(),
                runtime: "iOS 27.0".into(),
            }]
        );
    }
}
