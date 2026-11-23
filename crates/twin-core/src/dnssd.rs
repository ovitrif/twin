use std::io::{BufRead, BufReader};
use std::net::Ipv4Addr;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

const PAIRING_SERVICE_TYPE: &str = "_adb-tls-pairing._tcp";
const CONNECT_SERVICE_TYPE: &str = "_adb-tls-connect._tcp";
const LOCAL_DOMAIN: &str = "local";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedService {
    host: String,
    port: u16,
}

/// Whether `dns-sd` (macOS Bonjour) is there to ask; elsewhere only adb's
/// own mDNS discovery runs
pub const AVAILABLE: bool = cfg!(target_os = "macos");

pub fn discover_pairing_endpoint(instance: &str, timeout: Duration) -> Result<Option<String>> {
    if !AVAILABLE {
        return Ok(None);
    }

    let Some(service) = resolve_instance(instance, PAIRING_SERVICE_TYPE, timeout)? else {
        return Ok(None);
    };

    let host = resolve_host_to_ipv4(&service.host, timeout)?
        .unwrap_or_else(|| normalize_dns_name(&service.host));

    Ok(Some(format!("{host}:{}", service.port)))
}

pub fn discover_connect_endpoints(pairing_address: &str, timeout: Duration) -> Result<Vec<String>> {
    if !AVAILABLE {
        return Ok(Vec::new());
    }

    let browse_timeout = timeout.min(Duration::from_secs(4));
    let resolve_timeout = timeout.min(Duration::from_secs(3));
    let instances = browse_connect_instances(browse_timeout)?;
    let pairing_host = endpoint_host(pairing_address);
    let mut endpoints = Vec::new();

    for instance in instances {
        let Some(service) = resolve_instance(&instance, CONNECT_SERVICE_TYPE, resolve_timeout)?
        else {
            continue;
        };

        let host = resolve_host_to_ipv4(&service.host, resolve_timeout)?
            .unwrap_or_else(|| normalize_dns_name(&service.host));
        let endpoint = format!("{host}:{}", service.port);

        if !endpoints.contains(&endpoint) {
            endpoints.push(endpoint);
        }
    }

    Ok(connect_endpoints_for_pairing_host(endpoints, &pairing_host))
}

fn browse_connect_instances(timeout: Duration) -> Result<Vec<String>> {
    // Browsing has no last answer to wait for: it runs the whole time
    let output = run_dns_sd_until(["-B", CONNECT_SERVICE_TYPE, LOCAL_DOMAIN], timeout, |_| {
        false
    })?;
    Ok(parse_browse_instances(&output))
}

fn resolve_instance(
    instance: &str,
    service_type: &str,
    timeout: Duration,
) -> Result<Option<ResolvedService>> {
    let output = run_dns_sd_until(
        ["-L", instance, service_type, LOCAL_DOMAIN],
        timeout,
        |output| parse_resolved_service(output).is_some(),
    )?;
    Ok(parse_resolved_service(&output))
}

fn resolve_host_to_ipv4(host: &str, timeout: Duration) -> Result<Option<String>> {
    let output = run_dns_sd_until(["-G", "v4", host], timeout, |output| {
        parse_ipv4_address(output).is_some()
    })?;
    Ok(parse_ipv4_address(&output))
}

/// Runs `dns-sd`, which never exits on its own, until `timeout` or until
/// `done` accepts what it has printed so far, then stops it
fn run_dns_sd_until<const N: usize>(
    args: [&str; N],
    timeout: Duration,
    done: impl Fn(&str) -> bool,
) -> Result<String> {
    let mut child = Command::new("dns-sd")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to run dns-sd")?;

    let stdout = child
        .stdout
        .take()
        .context("failed to capture dns-sd stdout")?;
    let (lines, received) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });

    let output = collect_until(&received, Instant::now() + timeout, done);
    let _ = child.kill();
    let _ = child.wait();
    Ok(output)
}

/// The lines `received` delivers before `deadline`, or up to the first one
/// after which `done` accepts the output
fn collect_until(
    received: &Receiver<String>,
    deadline: Instant,
    done: impl Fn(&str) -> bool,
) -> String {
    let mut output = String::new();

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return output;
        }

        match received.recv_timeout(remaining) {
            Ok(line) => {
                output.push_str(&line);
                output.push('\n');
                if done(&output) {
                    return output;
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return output,
        }
    }
}

fn parse_browse_instances(output: &str) -> Vec<String> {
    let mut instances = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        let action = add_or_remove(line);

        if action.is_none() {
            continue;
        }

        let Some((_before, after_type)) = line.split_once(CONNECT_SERVICE_TYPE) else {
            continue;
        };

        let instance = after_type
            .trim_start_matches('.')
            .trim()
            .trim_end_matches('.')
            .trim();

        if action == Some("Rmv") {
            instances.retain(|existing| existing != instance);
        } else if !instance.is_empty() && !instances.iter().any(|existing| existing == instance) {
            instances.push(instance.to_string());
        }
    }

    instances
}

/// The A/R column of a dns-sd result line: "Add" or "Rmv"
fn add_or_remove(line: &str) -> Option<&str> {
    line.split_whitespace()
        .nth(1)
        .filter(|action| matches!(*action, "Add" | "Rmv"))
}

fn parse_resolved_service(output: &str) -> Option<ResolvedService> {
    for line in output.lines() {
        let Some((_before, after)) = line.split_once(" can be reached at ") else {
            continue;
        };

        // One garbled line must not hide a good one after it
        let Some((host, port)) = after
            .split_whitespace()
            .next()
            .and_then(|target| target.rsplit_once(':'))
        else {
            continue;
        };
        let Ok(port) = port.parse::<u16>() else {
            continue;
        };

        return Some(ResolvedService {
            host: normalize_dns_name(host),
            port,
        });
    }

    None
}

/// The first routable address a `dns-sd -G v4` lookup added
fn parse_ipv4_address(output: &str) -> Option<String> {
    output
        .lines()
        .filter(|line| add_or_remove(line) == Some("Add"))
        .flat_map(str::split_whitespace)
        .filter_map(|token| token.parse::<Ipv4Addr>().ok())
        .find(|address| !address.is_unspecified() && !address.is_loopback())
        .map(|address| address.to_string())
}

fn normalize_dns_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_string()
}

fn endpoint_host(endpoint: &str) -> String {
    endpoint
        .rsplit_once(':')
        .map(|(host, _port)| normalize_dns_name(host))
        .unwrap_or_else(|| endpoint.to_string())
}

fn connect_endpoints_for_pairing_host(endpoints: Vec<String>, pairing_host: &str) -> Vec<String> {
    endpoints
        .into_iter()
        .filter(|endpoint| endpoint_host(endpoint) == pairing_host)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_browse_instances() {
        let output = r#"
Browsing for _adb-tls-connect._tcp.local
DATE: ---Wed 06 May 2026---
10:44:01.123  Add        2  4 local. _adb-tls-connect._tcp. adb-5C020DLCH0007Q-tfPgZw
10:44:02.123  Rmv        2  4 local. _adb-tls-connect._tcp. adb-old
10:44:03.123  Add        2  4 local. _other._tcp. ignored
"#;

        assert_eq!(
            parse_browse_instances(output),
            vec!["adb-5C020DLCH0007Q-tfPgZw"]
        );
    }

    #[test]
    fn drops_browse_instances_a_later_line_removes() {
        let output = r#"
Browsing for _adb-tls-connect._tcp.local
DATE: ---Mon 23 Nov 2026---
14:02:11.481  ...STARTING...
Timestamp     A/R    Flags  if Domain               Service Type         Instance Name
14:02:11.482  Add        3  14 local.               _adb-tls-connect._tcp. adb-gone-a1b2c3
14:02:11.482  Add        2  14 local.               _adb-tls-connect._tcp. adb-back-d4e5f6
14:02:12.903  Rmv        1  14 local.               _adb-tls-connect._tcp. adb-gone-a1b2c3
14:02:12.903  Rmv        0  14 local.               _adb-tls-connect._tcp. adb-back-d4e5f6
14:02:13.120  Add        2  14 local.               _adb-tls-connect._tcp. adb-back-d4e5f6
"#;

        assert_eq!(parse_browse_instances(output), vec!["adb-back-d4e5f6"]);
    }

    #[test]
    fn ignores_unroutable_and_removed_addresses() {
        let output = r#"
DATE: ---Mon 23 Nov 2026---
14:02:11.481  ...STARTING...
Timestamp     A/R    Flags         IF  Hostname                               Address                                      TTL
14:02:11.482  Rmv    40000000      14  pixel.local.                           192.168.68.20                                120
14:02:11.482  Add    40000002       0  pixel.local.                           0.0.0.0                                      108   No Such Record
14:02:11.483  Add    40000002       1  pixel.local.                           127.0.0.1                                    120
"#;
        assert_eq!(parse_ipv4_address(output), None);

        let output = format!(
            "{output}14:02:11.500  Add    40000002      14  pixel.local.                           192.168.68.54                                120\n"
        );
        assert_eq!(
            parse_ipv4_address(&output).as_deref(),
            Some("192.168.68.54")
        );
    }

    #[test]
    fn parses_resolved_service() {
        let output = r#"
Lookup adb-5C020DLCH0007Q-tfPgZw._adb-tls-connect._tcp.local
10:44:01.123  adb-5C020DLCH0007Q-tfPgZw._adb-tls-connect._tcp.local. can be reached at pixel.local.:37197 (interface 4)
"#;

        assert_eq!(
            parse_resolved_service(output),
            Some(ResolvedService {
                host: "pixel.local".to_string(),
                port: 37197
            })
        );
    }

    #[test]
    fn skips_a_garbled_resolve_line() {
        let output = "\
10:44:01.123  x._adb-tls-connect._tcp.local. can be reached at 
10:44:01.124  x._adb-tls-connect._tcp.local. can be reached at pixel.local.:notaport (interface 4)
10:44:01.125  x._adb-tls-connect._tcp.local. can be reached at pixel.local.:37197 (interface 4)
";

        assert_eq!(
            parse_resolved_service(output),
            Some(ResolvedService {
                host: "pixel.local".to_string(),
                port: 37197
            })
        );
    }

    #[test]
    fn stops_collecting_once_the_answer_is_in() {
        let (lines, received) = mpsc::channel();
        for line in [
            "DATE: ---Mon 23 Nov 2026---",
            "14:02:11.482  Add    40000002      14  pixel.local.   192.168.68.54   120",
            "14:02:11.483  Add    40000002      14  pixel.local.   192.168.68.99   120",
        ] {
            lines.send(line.to_string()).unwrap();
        }
        // The sender stays open, as with a dns-sd still running
        let started = Instant::now();
        let output = collect_until(
            &received,
            Instant::now() + Duration::from_secs(5),
            |output| parse_ipv4_address(output).is_some(),
        );

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(!output.contains("192.168.68.99"));
        assert_eq!(
            parse_ipv4_address(&output).as_deref(),
            Some("192.168.68.54")
        );
        drop(lines);
    }

    #[test]
    fn collects_until_the_deadline_without_an_answer() {
        let (lines, received) = mpsc::channel::<String>();
        let started = Instant::now();
        let output = collect_until(
            &received,
            Instant::now() + Duration::from_millis(100),
            |_| false,
        );

        assert!(output.is_empty());
        assert!(started.elapsed() >= Duration::from_millis(100));
        drop(lines);
    }

    /// Random dns-sd-like lines never panic the parsers
    #[test]
    fn parsers_survive_generated_input() {
        use rand::{Rng, SeedableRng};

        let pieces = [
            "Add",
            "Rmv",
            "  ",
            "\t",
            "\n",
            "_adb-tls-connect._tcp.",
            "local.",
            " can be reached at ",
            "pixel.local.:",
            "37197",
            ":",
            "0.0.0.0",
            "192.168.1.5",
            "127.0.0.1",
            "é",
            "[",
            "]",
            ".",
        ];
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        for _ in 0..2_000 {
            let input: String = (0..rng.gen_range(0..14))
                .map(|_| pieces[rng.gen_range(0..pieces.len())])
                .collect();
            let _ = parse_browse_instances(&input);
            let _ = parse_resolved_service(&input);
            if let Some(address) = parse_ipv4_address(&input) {
                let address: Ipv4Addr = address.parse().unwrap();
                assert!(!address.is_unspecified() && !address.is_loopback());
            }
            let _ = endpoint_host(&input);
        }
    }

    #[test]
    fn parses_ipv4_address() {
        let output = r#"
DATE: ---Wed 06 May 2026---
10:44:01.123  Add     2  4 pixel.local. 192.168.68.54  120
"#;

        assert_eq!(parse_ipv4_address(output).as_deref(), Some("192.168.68.54"));
    }

    #[test]
    fn filters_connect_endpoints_to_pairing_host() {
        assert_eq!(
            connect_endpoints_for_pairing_host(
                vec![
                    "192.168.68.54:37197".to_string(),
                    "192.168.68.99:37197".to_string(),
                ],
                "192.168.68.54",
            ),
            vec!["192.168.68.54:37197"]
        );
    }
}
