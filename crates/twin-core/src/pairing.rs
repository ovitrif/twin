use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use crate::adb;
use crate::dnssd;

#[derive(Debug, Default)]
pub struct PairingEndpointDiscovery {
    pub endpoints: Vec<String>,
    pub adb_error: Option<String>,
    pub bonjour_error: Option<String>,
}

/// The endpoints advertising pairing `instance`: from the `adb mdns
/// services` the caller already ran (`Err` holds its failure), and from
/// Bonjour, which is asked only when adb found no routable IPv4 endpoint
pub fn discover_pairing_endpoint_candidates(
    services: Result<&[adb::MdnsService], String>,
    instance: &str,
    bonjour_timeout: Duration,
) -> PairingEndpointDiscovery {
    let mut discovery = PairingEndpointDiscovery::default();

    match services {
        Ok(services) => {
            for service in services
                .iter()
                .filter(|service| service.instance == instance && service.is_pairing_service())
            {
                push_endpoint(&mut discovery.endpoints, service.address.clone());
            }
        }
        Err(error) => discovery.adb_error = Some(error),
    }

    if needs_bonjour(&discovery.endpoints) {
        match dnssd::discover_pairing_endpoint(instance, bonjour_timeout) {
            Ok(Some(endpoint)) => push_endpoint(&mut discovery.endpoints, endpoint),
            Ok(None) => {}
            Err(error) => discovery.bonjour_error = Some(format!("{error:#}")),
        }
    }

    order_endpoints(&mut discovery.endpoints);
    discovery
}

/// Bonjour blocks for seconds; skip it once adb has an endpoint to pair with
fn needs_bonjour(endpoints: &[String]) -> bool {
    dnssd::AVAILABLE
        && !endpoints
            .iter()
            .any(|endpoint| endpoint_preference(endpoint) == 0)
}

fn push_endpoint(endpoints: &mut Vec<String>, endpoint: String) {
    let endpoint = endpoint.trim();

    if endpoint.is_empty() || endpoints.iter().any(|existing| existing == endpoint) {
        return;
    }

    endpoints.push(endpoint.to_string());
}

fn order_endpoints(endpoints: &mut [String]) {
    endpoints.sort_by_key(|endpoint| endpoint_preference(endpoint));
}

fn endpoint_preference(endpoint: &str) -> u8 {
    let host = adb::endpoint_host(endpoint)
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();

    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return if ip.is_link_local() || ip.is_loopback() {
            2
        } else {
            0
        };
    }

    if host.parse::<Ipv6Addr>().is_ok() {
        return 3;
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_bonjour_once_adb_has_a_routable_endpoint() {
        let adb_found = |endpoints: &[&str]| {
            needs_bonjour(
                &endpoints
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            )
        };

        assert!(!adb_found(&["192.168.0.42:37000"]));
        assert!(!adb_found(&["Android.local:37000", "192.168.0.42:37000"]));
        assert_eq!(adb_found(&[]), dnssd::AVAILABLE);
        assert_eq!(adb_found(&["169.254.1.42:37000"]), dnssd::AVAILABLE);
        assert_eq!(adb_found(&["Android.local:37000"]), dnssd::AVAILABLE);
    }

    #[test]
    fn takes_pairing_endpoints_from_the_given_adb_services() {
        let services = [
            adb::MdnsService {
                instance: "twin-abc123".to_string(),
                service_type: "_adb-tls-pairing._tcp".to_string(),
                address: "192.168.0.42:37000".to_string(),
            },
            adb::MdnsService {
                instance: "someone-else".to_string(),
                service_type: "_adb-tls-pairing._tcp".to_string(),
                address: "192.168.0.43:37000".to_string(),
            },
        ];

        let discovery =
            discover_pairing_endpoint_candidates(Ok(&services), "twin-abc123", Duration::ZERO);
        assert_eq!(discovery.endpoints, vec!["192.168.0.42:37000"]);
        assert_eq!(discovery.adb_error, None);
        assert_eq!(discovery.bonjour_error, None);
    }

    #[test]
    fn prefers_routable_ipv4_pairing_endpoints() {
        let mut endpoints = vec![
            "[fe80::1234]:37000".to_string(),
            "Android.local:37000".to_string(),
            "192.168.0.42:37000".to_string(),
            "169.254.1.42:37000".to_string(),
        ];

        order_endpoints(&mut endpoints);

        assert_eq!(endpoints[0], "192.168.0.42:37000");
        assert_eq!(endpoints[1], "Android.local:37000");
        assert_eq!(endpoints[2], "169.254.1.42:37000");
        assert_eq!(endpoints[3], "[fe80::1234]:37000");
    }
}
