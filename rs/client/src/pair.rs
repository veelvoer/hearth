//! Parsing pairing links and manually entered addresses.

use crate::model::Machine;
use url::Url;

/// Parse a pairing link (hearth://pair?url=…&token=…) or a typed address into a Machine.
pub fn parse_machine(input: &str, name: &str, token: &str) -> Option<Machine> {
    let s = input.trim();

    // Handle hearth:// links
    if s.starts_with("hearth://") {
        if let Ok(url) = Url::parse(s) {
            let url_param = url.query_pairs().find(|(k, _)| k == "url")?.1.to_string();
            let token_param = url.query_pairs().find(|(k, _)| k == "token")?.1.to_string();
            let name_param = url
                .query_pairs()
                .find(|(k, _)| k == "name")
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| name.to_string());
            return parse_machine(&url_param, &name_param, &token_param);
        }
        return None;
    }

    if s.is_empty() {
        return None;
    }

    // Strip scheme if present
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s)
        .trim_end_matches('/');

    // Parse host, port, and path
    let host_part = rest.split(':').next().unwrap_or("").split('/').next().unwrap_or("");

    // A domain name (not an IP) with no scheme is assumed secure
    let secure = s.starts_with("https://")
        || (!s.starts_with("http://")
            && host_part.contains('.')
            && host_part.chars().any(|c| c.is_alphabetic()));

    let host = host_part.to_string();
    let port_str = rest
        .split(':')
        .nth(1)
        .and_then(|p| p.split('/').next())
        .unwrap_or("");
    let port = port_str
        .parse::<u16>()
        .ok()
        .unwrap_or(if secure { 443 } else { 47601 });

    Some(Machine {
        name: if name.is_empty() { host.clone() } else { name.to_string() },
        host,
        port,
        token: token.to_string(),
        secure,
        guest: false,
        label: String::new(),
        share: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ip_no_scheme() {
        let m = parse_machine("192.168.1.1:47601", "laptop", "token123").unwrap();
        assert_eq!(m.host, "192.168.1.1");
        assert_eq!(m.port, 47601);
        assert!(!m.secure);
    }

    #[test]
    fn test_parse_domain_no_scheme() {
        let m = parse_machine("example.com", "server", "token123").unwrap();
        assert_eq!(m.host, "example.com");
        assert_eq!(m.port, 443);
        assert!(m.secure);
    }

    #[test]
    fn test_parse_https() {
        let m = parse_machine("https://example.com:8443", "srv", "tok").unwrap();
        assert_eq!(m.host, "example.com");
        assert_eq!(m.port, 8443);
        assert!(m.secure);
    }

    #[test]
    fn test_parse_http() {
        let m = parse_machine("http://localhost:3000", "local", "tok").unwrap();
        assert_eq!(m.host, "localhost");
        assert_eq!(m.port, 3000);
        assert!(!m.secure);
    }

    #[test]
    fn test_parse_empty() {
        assert!(parse_machine("", "name", "tok").is_none());
    }
}
