use serde::{Deserialize, Serialize};

use crate::Error;

const SERVICE: &str = "aroh";
const CONFIG_ACCOUNT: &str = "supabase-config";
pub(crate) const SESSION_ACCOUNT: &str = "supabase-session";

/// The Supabase project the client talks to. `url` is a validated origin
/// without a trailing slash.
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub url: String,
    pub anon_key: String,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config").field("url", &self.url).finish_non_exhaustive()
    }
}

/// Checks a Supabase project URL and returns its origin.
/// Input: the URL the owner entered. Output: `scheme://host[:port]`.
/// Only `https` is allowed, except `http` for `localhost` and `127.0.0.1`.
pub fn validate_url(raw: &str) -> Result<String, Error> {
    let bad = |why: &str| Error::Config(format!("project URL {why}"));
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| bad("is not a URL"))?;
    let host = url.host_str().ok_or_else(|| bad("has no host"))?;
    let local = host == "localhost" || host == "127.0.0.1";
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return Err(bad("must use https")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(bad("must not carry credentials"));
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(bad("must be a bare origin"));
    }
    let origin = url.origin().ascii_serialization();
    assert!(!origin.ends_with('/'));
    Ok(origin)
}

/// Loads the project config: env `AROH_SUPABASE_URL` and
/// `AROH_SUPABASE_ANON_KEY` when both are set, else the keychain.
pub fn load() -> Result<Config, Error> {
    let config = match (std::env::var("AROH_SUPABASE_URL"), std::env::var("AROH_SUPABASE_ANON_KEY")) {
        (Ok(url), Ok(anon_key)) => Config { url, anon_key },
        _ => {
            let json = read(CONFIG_ACCOUNT)?.ok_or(Error::NotConfigured)?;
            serde_json::from_str(&json).map_err(|_| Error::Config("stored config is corrupt".into()))?
        }
    };
    Ok(Config { url: validate_url(&config.url)?, anon_key: config.anon_key })
}

/// Validates and stores the project URL and anon key in the keychain.
pub fn save(url: &str, anon_key: &str) -> Result<(), Error> {
    if anon_key.trim().is_empty() {
        return Err(Error::Config("anon key is empty".into()));
    }
    let config = Config { url: validate_url(url)?, anon_key: anon_key.trim().to_string() };
    write(CONFIG_ACCOUNT, &serde_json::to_string(&config).expect("config serializes"))
}

pub(crate) fn read(account: &str) -> Result<Option<String>, Error> {
    match keyring::Entry::new(SERVICE, account)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn write(account: &str, value: &str) -> Result<(), Error> {
    keyring::Entry::new(SERVICE, account)?.set_password(value)?;
    Ok(())
}

pub(crate) fn delete(account: &str) -> Result<(), Error> {
    match keyring::Entry::new(SERVICE, account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_url;

    #[test]
    fn url_rules() {
        assert_eq!(validate_url("https://abc.supabase.co/").unwrap(), "https://abc.supabase.co");
        assert_eq!(validate_url("http://localhost:54321").unwrap(), "http://localhost:54321");
        assert_eq!(validate_url("http://127.0.0.1:54321").unwrap(), "http://127.0.0.1:54321");
        for bad in [
            "http://abc.supabase.co",
            "http://localhost.evil.com",
            "ftp://abc.supabase.co",
            "https://user:pw@abc.supabase.co",
            "https://abc.supabase.co/rest/v1",
            "https://abc.supabase.co?x=1",
            "abc.supabase.co",
        ] {
            assert!(validate_url(bad).is_err(), "{bad}");
        }
    }
}
