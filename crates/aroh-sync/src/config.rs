use crate::{Error, Result};

const SERVICE: &str = "aroh";

/// The Supabase project the app talks to: `url` is an origin without a
/// trailing slash (https, or http on localhost), `anon_key` the public key.
/// Built only through `Config::new`, which validates both.
#[derive(Clone)]
pub struct Config {
    pub url: String,
    pub anon_key: String,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config").field("url", &self.url).finish_non_exhaustive()
    }
}

impl Config {
    /// Validates the project URL with the domain URL rule and the anon key
    /// (20 to 4096 visible ASCII chars). Input: both as the owner entered them.
    /// Output: the config with the URL's trailing slash removed.
    pub fn new(url: &str, anon_key: &str) -> Result<Config> {
        let url = url.trim().trim_end_matches('/');
        aroh_domain::rows::check_url("supabase_url", url).map_err(|e| Error::Config(e.to_string()))?;
        let anon_key = anon_key.trim();
        aroh_domain::rows::check_public_key("supabase_anon_key", anon_key).map_err(|e| Error::Config(e.to_string()))?;
        Ok(Config { url: url.to_string(), anon_key: anon_key.to_string() })
    }
}

/// Reads one keychain entry of service `aroh`; `None` when absent.
pub(crate) fn read(account: &str) -> Result<Option<String>> {
    match keyring::Entry::new(SERVICE, account)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Writes one keychain entry of service `aroh`, replacing the old value.
pub(crate) fn write(account: &str, value: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, account)?.set_password(value)?;
    Ok(())
}

/// Deletes one keychain entry of service `aroh`; a missing entry is fine.
pub(crate) fn delete(account: &str) -> Result<()> {
    match keyring::Entry::new(SERVICE, account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn config_rules() {
        let key = "eyJhbGciOiJIUzI1NiJ9.e30.sig";
        assert_eq!(Config::new("https://abc.supabase.co/", key).unwrap().url, "https://abc.supabase.co");
        assert_eq!(Config::new("http://localhost:54321", key).unwrap().url, "http://localhost:54321");
        assert_eq!(Config::new("http://127.0.0.1:54321", key).unwrap().url, "http://127.0.0.1:54321");
        for bad in ["http://abc.supabase.co", "ftp://x.co", "abc.supabase.co", "https://user@x.co"] {
            assert!(Config::new(bad, key).is_err(), "{bad}");
        }
        assert!(Config::new("https://abc.supabase.co", "short").is_err());
        assert!(Config::new("https://abc.supabase.co", "has a space in it, twenty chars").is_err());
        assert!(!format!("{:?}", Config::new("https://abc.supabase.co", key).unwrap()).contains(key));
    }
}
