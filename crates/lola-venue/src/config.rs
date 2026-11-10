//! `config/venue.yaml`.
//!
//! Credentials are deliberately `mode: none`: key management is a carved-out
//! discussion (`docs/KEY_MANAGEMENT.md`). Nothing in v0.1 reads secrets, and
//! no adapter may invent a credential format before that discussion happens.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct VenueConfig {
    pub venue: String,
    pub network: String,
    pub products: Vec<String>,
    pub watchlist: Vec<String>,
    pub credentials: CredentialsPolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CredentialsPolicy {
    /// Only `none` is valid in v0.1.
    pub mode: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const VENUE_YAML: &str = include_str!("../../../config/venue.yaml");

    #[test]
    fn shipped_venue_yaml_parses_with_no_credentials() {
        let config: VenueConfig = serde_yaml::from_str(VENUE_YAML).expect("parse venue.yaml");
        assert_eq!(config.venue, "hyperliquid");
        assert_eq!(config.network, "testnet");
        assert!(config.products.contains(&"perp".to_string()));
        assert!(config.watchlist.iter().any(|s| s.ends_with("PERP")));
        assert_eq!(config.credentials.mode, "none");
    }
}
