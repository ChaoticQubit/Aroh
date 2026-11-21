use crate::error::ServerError;
use crate::vault::MasterKey;
use std::collections::BTreeMap;

/// Process settings read from the environment once at start: database,
/// master key, JWT verification, bind address and the operator switches.
/// `Debug` never prints the master key or the JWT secret.
pub struct Config {
    pub database_url: String,
    pub master_key: MasterKey,
    pub jwt_secret: Option<String>,
    pub jwks_url: Option<String>,
    pub jwt_audience: String,
    pub bind: String,
    pub allow_custom_base_url: bool,
    pub search_toml: Option<String>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("jwt_secret", &self.jwt_secret.as_ref().map(|_| "[redacted]"))
            .field("jwks_url", &self.jwks_url)
            .field("bind", &self.bind)
            .field("allow_custom_base_url", &self.allow_custom_base_url)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// Reads the config from `vars` (the process environment in `main`):
    /// `DATABASE_URL`, `AROH_MASTER_KEY`, `AROH_JWT_SECRET` or `AROH_JWKS_URL`,
    /// and the optional rest. Returns `Config` naming the first missing one.
    pub fn from_vars(vars: &BTreeMap<String, String>) -> Result<Config, ServerError> {
        let get = |k: &str| vars.get(k).filter(|v| !v.trim().is_empty()).cloned();
        let need = |k: &str| get(k).ok_or_else(|| ServerError::Config(format!("{k} is not set")));
        let jwt_secret = get("AROH_JWT_SECRET");
        let jwks_url = get("AROH_JWKS_URL");
        if jwt_secret.is_none() && jwks_url.is_none() {
            return Err(ServerError::Config("set AROH_JWT_SECRET or AROH_JWKS_URL".into()));
        }
        let bind = get("AROH_BIND").unwrap_or_else(|| format!("0.0.0.0:{}", get("PORT").unwrap_or_else(|| "8080".into())));
        let search_toml = match get("AROH_SEARCH_TOML") {
            Some(path) => Some(
                std::fs::read_to_string(&path).map_err(|e| ServerError::Config(format!("AROH_SEARCH_TOML {path}: {e}")))?,
            ),
            None => None,
        };
        Ok(Config {
            database_url: need("DATABASE_URL")?,
            master_key: MasterKey::from_base64(&need("AROH_MASTER_KEY")?)?,
            jwt_secret,
            jwks_url,
            jwt_audience: get("AROH_JWT_AUDIENCE").unwrap_or_else(|| "authenticated".into()),
            bind,
            allow_custom_base_url: get("AROH_ALLOW_CUSTOM_BASE_URL").as_deref() == Some("1"),
            search_toml,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_requires_vars() {
        let mut vars = BTreeMap::new();
        assert!(Config::from_vars(&vars).is_err());
        vars.insert("DATABASE_URL".to_string(), "postgres://x".to_string());
        vars.insert("AROH_MASTER_KEY".to_string(), "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string());
        assert!(Config::from_vars(&vars).unwrap_err().to_string().contains("AROH_JWT_SECRET"));
        vars.insert("AROH_JWT_SECRET".to_string(), "super-secret-value".to_string());
        vars.insert("PORT".to_string(), "9000".to_string());
        let c = Config::from_vars(&vars).unwrap();
        assert_eq!(c.bind, "0.0.0.0:9000");
        assert!(!c.allow_custom_base_url);
        assert!(!format!("{c:?}").contains("super-secret-value"));
    }
}
