//! libp2p Identify `protocol_version` compatibility for Catalyst peers.
//!
//! The Identify behaviour advertises a short string (e.g. `catalyst/1.0`) that must align with
//! operator TOML defaults in `catalyst-cli`. Remote peers may still run older builds (`catalyst/1`);
//! we accept the same **major** family and reject incompatible majors.

/// Version string this node advertises in [`libp2p::identify::Behaviour`].
pub const CATALYST_IDENTIFY_PROTOCOL_VERSION: &str = "catalyst/1.0";

/// Returns true if the remote Identify `protocol_version` is in the supported Catalyst major family.
///
/// Accepted examples: `catalyst/1`, `catalyst/1.0`, `catalyst/1.2-beta`.
/// Rejected: wrong prefix, empty, or major ≠ 1 (e.g. `catalyst/10`, `catalyst/2`).
pub fn catalyst_identify_protocol_major_ok(version: &str) -> bool {
    let rest = match version.strip_prefix("catalyst/") {
        Some(r) if !r.is_empty() => r,
        _ => return false,
    };
    let major_str = rest
        .split(|c| c == '.' || c == '-' || c == '/')
        .next()
        .unwrap_or("");
    major_str.parse::<u32>() == Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_ok_accepts_cli_and_legacy_strings() {
        assert!(catalyst_identify_protocol_major_ok("catalyst/1"));
        assert!(catalyst_identify_protocol_major_ok("catalyst/1.0"));
        assert!(catalyst_identify_protocol_major_ok("catalyst/1.2"));
        assert!(catalyst_identify_protocol_major_ok("catalyst/1-rc1"));
    }

    #[test]
    fn major_ok_rejects_other_majors_or_garbage() {
        assert!(!catalyst_identify_protocol_major_ok("catalyst/10"));
        assert!(!catalyst_identify_protocol_major_ok("catalyst/2.0"));
        assert!(!catalyst_identify_protocol_major_ok("eth/1.0"));
        assert!(!catalyst_identify_protocol_major_ok("catalyst/"));
        assert!(!catalyst_identify_protocol_major_ok(""));
    }
}
