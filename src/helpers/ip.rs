/// Returns `true` when `addr` is an RFC1918 / loopback / link-local address
/// that the Stacker cloud install service cannot reach from the internet.
///
/// Also treats bare hostnames (no dots) and `.local` / `.lan` / `.internal`
/// suffixes as private, since those only resolve on the local network.
pub fn is_private_host(addr: &str) -> bool {
    use std::net::IpAddr;
    if let Ok(ip) = addr.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_link_local()
                    || v4.octets()[0] == 10
                    || (v4.octets()[0] == 172 && (16..=31).contains(&v4.octets()[1]))
                    || (v4.octets()[0] == 192 && v4.octets()[1] == 168)
            }
            IpAddr::V6(v6) => v6.is_loopback(),
        };
    }
    !addr.contains('.')
        || addr.ends_with(".local")
        || addr.ends_with(".internal")
        || addr.ends_with(".lan")
}

pub(crate) fn extract_ipv4_from_text(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find_map(|candidate| {
            let trimmed = candidate.trim_matches('.');
            if trimmed.parse::<std::net::Ipv4Addr>().is_ok() {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_private_host ──────────────────────────────────────────────────────

    #[test]
    fn private_rfc1918_addresses_are_private() {
        assert!(is_private_host("192.168.100.245"));
        assert!(is_private_host("192.168.1.1"));
        assert!(is_private_host("10.0.0.1"));
        assert!(is_private_host("10.255.255.255"));
        assert!(is_private_host("172.16.0.1"));
        assert!(is_private_host("172.31.255.255"));
        assert!(is_private_host("127.0.0.1"));
    }

    #[test]
    fn private_hostnames_are_private() {
        assert!(is_private_host("myserver.local"));
        assert!(is_private_host("host.internal"));
        assert!(is_private_host("box.lan"));
        assert!(is_private_host("localhost"));
    }

    #[test]
    fn public_ips_are_not_private() {
        assert!(!is_private_host("203.0.113.10"));
        assert!(!is_private_host("1.2.3.4"));
        assert!(!is_private_host("178.104.222.170"));
        assert!(!is_private_host("8.8.8.8"));
    }

    #[test]
    fn public_hostnames_are_not_private() {
        assert!(!is_private_host("example.com"));
        assert!(!is_private_host("my-server.example.com"));
        assert!(!is_private_host("hetzner.cloud"));
    }

    // ── extract_ipv4_from_text ───────────────────────────────────────────────

    #[test]
    fn extracts_ipv4_from_status_message_prefix() {
        assert_eq!(
            extract_ipv4_from_text("178.104.222.170: Copy files is done"),
            Some("178.104.222.170".to_string())
        );
    }

    #[test]
    fn ignores_text_without_valid_ipv4() {
        assert_eq!(extract_ipv4_from_text("Deployment still in progress"), None);
        assert_eq!(
            extract_ipv4_from_text("invalid 999.104.222.170: message"),
            None
        );
    }
}
