use crate::db;
use crate::error::ServerError;
use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::Serialize;
use crate::db::PgPool;
use std::sync::Arc;
use troupekit_core::secret::Secret;
use utoipa::ToSchema;
use uuid::Uuid;
use zeroize::Zeroizing;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const KEY_VERSION: i32 = 1;
const MIN_SECRET_CHARS: usize = 8;
const MAX_SECRET_CHARS: usize = 512;

/// The 32-byte AES-256-GCM master key that wraps every user's data key.
/// Read once from the environment; the bytes are zeroized on drop and
/// never printed (Debug shows `[redacted]`).
pub struct MasterKey(Zeroizing<[u8; KEY_LEN]>);

impl MasterKey {
    /// Parses a standard base64 string holding exactly 32 bytes.
    /// Returns `Config` for bad base64 or a wrong length; the message never
    /// echoes the input.
    pub fn from_base64(text: &str) -> Result<MasterKey, ServerError> {
        let raw = Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .decode(text.trim())
                .map_err(|_| ServerError::Config("master key is not base64".into()))?,
        );
        let bytes: [u8; KEY_LEN] = raw
            .as_slice()
            .try_into()
            .map_err(|_| ServerError::Config("master key must be 32 bytes".into()))?;
        Ok(MasterKey(Zeroizing::new(bytes)))
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MasterKey([redacted])")
    }
}

/// Encrypts `plain` under `key` with AES-256-GCM, binding `aad`.
/// Returns the ciphertext (with tag) and the fresh random 96-bit nonce.
/// The same key never reuses a nonce in practice: each call draws a new one.
pub fn seal(key: &[u8; KEY_LEN], aad: &[u8], plain: &[u8]) -> Result<(Vec<u8>, Vec<u8>), ServerError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| ServerError::Crypto("bad key length".into()))?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, Payload { msg: plain, aad })
        .map_err(|_| ServerError::Crypto("encryption failed".into()))?;
    assert_eq!(nonce.len(), NONCE_LEN);
    Ok((ct, nonce.to_vec()))
}

/// Decrypts `ct` sealed by `seal` under `key` with the same `aad` and `nonce`.
/// Returns the plaintext in zeroizing memory, or `Crypto` when the key,
/// associated data, nonce or ciphertext do not match.
pub fn open(key: &[u8; KEY_LEN], aad: &[u8], nonce: &[u8], ct: &[u8]) -> Result<Zeroizing<Vec<u8>>, ServerError> {
    if nonce.len() != NONCE_LEN {
        return Err(ServerError::Crypto("bad nonce length".into()));
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| ServerError::Crypto("bad key length".into()))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad })
        .map(Zeroizing::new)
        .map_err(|_| ServerError::Crypto("cannot open sealed value".into()))
}

fn dek_aad(user: Uuid) -> Vec<u8> {
    format!("aroh/dek/v1/{user}").into_bytes()
}

fn key_aad(user: Uuid, setting: Uuid) -> Vec<u8> {
    format!("aroh/key/v1/{user}/{setting}").into_bytes()
}

/// What the API shows about a stored key: its setting, vendor, last four
/// characters and when it was set. Never the key itself.
/// Listing never decrypts anything.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct KeyInfo {
    pub setting_id: Uuid,
    pub vendor: String,
    pub last4: String,
    pub updated_at: DateTime<Utc>,
}

/// Checks a key a user typed: 8 to 512 characters, no whitespace or
/// control characters. Returns `BadRequest` otherwise; the message never
/// contains the key.
pub fn check_secret(key: &Secret) -> Result<(), ServerError> {
    let s = key.expose();
    let n = s.chars().count();
    if !(MIN_SECRET_CHARS..=MAX_SECRET_CHARS).contains(&n) || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ServerError::BadRequest(format!(
            "key must be {MIN_SECRET_CHARS} to {MAX_SECRET_CHARS} characters without spaces"
        )));
    }
    Ok(())
}

/// Checks a vendor id: 1 to 64 of `[a-z0-9_-]`.
/// Returns `BadRequest` otherwise.
/// Callers also check that the vendor exists.
pub fn check_vendor(vendor: &str) -> Result<(), ServerError> {
    let ok = !vendor.is_empty()
        && vendor.len() <= 64
        && vendor.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    if !ok {
        return Err(ServerError::BadRequest("vendor must be 1 to 64 of a-z, 0-9, _ and -".into()));
    }
    Ok(())
}

/// Provider and search keys at rest, envelope encrypted: one random data key
/// per user wrapped by the master key, each credential sealed by its user's
/// data key and bound to its user and setting. Keys leave only as `Secret`.
#[derive(Clone)]
pub struct Vault {
    pool: PgPool,
    master: Arc<MasterKey>,
}

impl Vault {
    /// Builds a vault over `pool` and `master`.
    /// Cheap to clone; clones share the master key.
    /// Returns the vault.
    pub fn new(pool: PgPool, master: Arc<MasterKey>) -> Vault {
        Vault { pool, master }
    }

    async fn data_key(&self, user: Uuid, create: bool) -> Result<Option<Zeroizing<[u8; KEY_LEN]>>, ServerError> {
        let select = "select wrapped, nonce from public.user_keys where user_id = $1";
        let mut row: Option<(Vec<u8>, Vec<u8>)> = db::query_as(select).bind(user).fetch_optional(&self.pool).await?;
        if row.is_none() && create {
            let mut fresh = Zeroizing::new([0u8; KEY_LEN]);
            OsRng.fill_bytes(fresh.as_mut());
            let (wrapped, nonce) = seal(&self.master.0, &dek_aad(user), fresh.as_slice())?;
            db::query(
                "insert into public.user_keys (user_id, wrapped, nonce, key_version) values ($1, $2, $3, $4) on conflict (user_id) do nothing",
            )
            .bind(user)
            .bind(wrapped)
            .bind(nonce)
            .bind(KEY_VERSION)
            .execute(&self.pool)
            .await?;
            row = db::query_as(select).bind(user).fetch_optional(&self.pool).await?;
        }
        let Some((wrapped, nonce)) = row else {
            return Ok(None);
        };
        let plain = open(&self.master.0, &dek_aad(user), &nonce, &wrapped)?;
        let bytes: [u8; KEY_LEN] =
            plain.as_slice().try_into().map_err(|_| ServerError::Crypto("data key has the wrong length".into()))?;
        Ok(Some(Zeroizing::new(bytes)))
    }

    /// Stores `key` for `user` and `setting` under `vendor`, replacing an
    /// earlier one. Returns what the API may show about it, or `BadRequest`
    /// for a malformed key or vendor.
    pub async fn set(&self, user: Uuid, setting: Uuid, vendor: &str, key: &Secret) -> Result<KeyInfo, ServerError> {
        check_vendor(vendor)?;
        check_secret(key)?;
        let dek = self.data_key(user, true).await?.ok_or_else(|| ServerError::Crypto("no data key".into()))?;
        let (ct, nonce) = seal(&dek, &key_aad(user, setting), key.expose().as_bytes())?;
        let chars: Vec<char> = key.expose().chars().collect();
        let last4: String = chars[chars.len() - 4..].iter().collect();
        let updated_at: DateTime<Utc> = db::query_scalar(
            "insert into public.provider_credentials (user_id, setting_id, ciphertext, nonce, key_version, vendor, last4)
             values ($1, $2, $3, $4, $5, $6, $7)
             on conflict (user_id, setting_id) do update set ciphertext = excluded.ciphertext, nonce = excluded.nonce,
               key_version = excluded.key_version, vendor = excluded.vendor, last4 = excluded.last4, updated_at = now()
             returning updated_at",
        )
        .bind(user)
        .bind(setting)
        .bind(ct)
        .bind(nonce)
        .bind(KEY_VERSION)
        .bind(vendor)
        .bind(&last4)
        .fetch_one(&self.pool)
        .await?;
        Ok(KeyInfo { setting_id: setting, vendor: vendor.to_string(), last4, updated_at })
    }

    /// Lists the keys of `user`, oldest first, without decrypting any.
    /// Returns an empty list for a user with no keys.
    /// Only rows of `user` are read.
    pub async fn list(&self, user: Uuid) -> Result<Vec<KeyInfo>, ServerError> {
        let rows: Vec<(Uuid, String, String, DateTime<Utc>)> = db::query_as(
            "select setting_id, vendor, last4, updated_at from public.provider_credentials where user_id = $1 order by updated_at, setting_id",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(setting_id, vendor, last4, updated_at)| KeyInfo { setting_id, vendor, last4, updated_at })
            .collect())
    }

    /// Returns what the API may show about the key of `user` for `setting`.
    /// `None` when there is none.
    /// Never decrypts.
    pub async fn info(&self, user: Uuid, setting: Uuid) -> Result<Option<KeyInfo>, ServerError> {
        Ok(self.list(user).await?.into_iter().find(|k| k.setting_id == setting))
    }

    /// Deletes the key of `user` for `setting`.
    /// Returns true when a key was deleted.
    /// Never touches another user's rows.
    pub async fn delete(&self, user: Uuid, setting: Uuid) -> Result<bool, ServerError> {
        let done = db::query("delete from public.provider_credentials where user_id = $1 and setting_id = $2")
            .bind(user)
            .bind(setting)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Decrypts the key of `user` for `setting` for one call. Returns `None`
    /// when no key is stored, or `Crypto` when the row does not open under
    /// this user's data key and setting (a copied or tampered row).
    pub async fn load(&self, user: Uuid, setting: Uuid) -> Result<Option<Secret>, ServerError> {
        let row: Option<(Vec<u8>, Vec<u8>, i32)> = db::query_as(
            "select ciphertext, nonce, key_version from public.provider_credentials where user_id = $1 and setting_id = $2",
        )
        .bind(user)
        .bind(setting)
        .fetch_optional(&self.pool)
        .await?;
        let Some((ct, nonce, version)) = row else {
            return Ok(None);
        };
        if version != KEY_VERSION {
            return Err(ServerError::Crypto(format!("key version {version} is not supported")));
        }
        let dek = self.data_key(user, false).await?.ok_or_else(|| ServerError::Crypto("no data key".into()))?;
        let plain = open(&dek, &key_aad(user, setting), &nonce, &ct)?;
        let text = std::str::from_utf8(&plain).map_err(|_| ServerError::Crypto("key is not text".into()))?;
        Secret::new(text.to_string()).map(Some).map_err(|_| ServerError::Crypto("stored key is blank".into()))
    }
}

impl Vault {
    /// Seals `plain` under `user`'s data key (made on first use) with `aad`.
    /// Returns the ciphertext and nonce. Used for the checkpoints and events
    /// of runs that read the diary, so no diary text rests in plaintext.
    pub(crate) async fn seal_for(&self, user: Uuid, aad: &[u8], plain: &[u8]) -> Result<(Vec<u8>, Vec<u8>), ServerError> {
        let dek = self.data_key(user, true).await?.ok_or_else(|| ServerError::Crypto("no data key".into()))?;
        seal(&dek, aad, plain)
    }

    /// Opens a value sealed by `seal_for` for `user` with the same `aad`.
    /// Returns the plaintext in zeroizing memory, or `Crypto` on any mismatch.
    pub(crate) async fn open_for(&self, user: Uuid, aad: &[u8], nonce: &[u8], ct: &[u8]) -> Result<Zeroizing<Vec<u8>>, ServerError> {
        let dek = self.data_key(user, false).await?.ok_or_else(|| ServerError::Crypto("no data key".into()))?;
        open(&dek, aad, nonce, ct)
    }

    /// Stores `user`'s diary key sealed by their data key, replacing an older
    /// copy (Q94: the server always holds it). A new copy starts with agents
    /// reading off; a replaced one keeps the toggle. Returns the stored version.
    pub async fn diary_set(&self, user: Uuid, key: &aroh_diary::DiaryKey) -> Result<i64, ServerError> {
        let (sealed, nonce) = self.seal_for(user, &diary_aad(user, key.version()), key.bytes()).await?;
        db::query(
            "insert into public.diary_keys (user_id, key_version, sealed, nonce) values ($1, $2, $3, $4)
             on conflict (user_id) do update set key_version = excluded.key_version, sealed = excluded.sealed,
               nonce = excluded.nonce, created_at = now()",
        )
        .bind(user)
        .bind(key.version())
        .bind(sealed)
        .bind(nonce)
        .execute(&self.pool)
        .await?;
        Ok(key.version())
    }

    /// Sets whether planning agents may read `user`'s diary. Returns false
    /// when the server holds no diary key for `user`, so nothing changed.
    pub async fn diary_agents(&self, user: Uuid, on: bool) -> Result<bool, ServerError> {
        let done = db::query("update public.diary_keys set agents_read = $2 where user_id = $1").bind(user).bind(on).execute(&self.pool).await?;
        Ok(done.rows_affected() == 1)
    }

    /// The version of `user`'s server copy of the diary key and whether
    /// agents may read the diary; None when no copy is held. Never decrypts.
    pub async fn diary_state(&self, user: Uuid) -> Result<Option<(i64, bool)>, ServerError> {
        Ok(db::query_as("select key_version, agents_read from public.diary_keys where user_id = $1").bind(user).fetch_optional(&self.pool).await?)
    }

    /// Opens `user`'s diary key for one call, None when no copy is held.
    /// Returns `Crypto` when the row does not open under this user.
    pub async fn diary_get(&self, user: Uuid) -> Result<Option<aroh_diary::DiaryKey>, ServerError> {
        let row: Option<(i64, Vec<u8>, Vec<u8>)> =
            db::query_as("select key_version, sealed, nonce from public.diary_keys where user_id = $1").bind(user).fetch_optional(&self.pool).await?;
        let Some((version, sealed, nonce)) = row else {
            return Ok(None);
        };
        let plain = self.open_for(user, &diary_aad(user, version), &nonce, &sealed).await?;
        aroh_diary::DiaryKey::from_bytes(version, &plain).map(Some).map_err(|_| ServerError::Crypto("stored diary key is malformed".into()))
    }
}

fn diary_aad(user: Uuid, version: i64) -> Vec<u8> {
    format!("aroh/diary/v1/{user}/{version}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7u8; 32];

    #[test]
    fn seal_open_round_trip() {
        let (ct, nonce) = seal(&KEY, b"aad", b"sk-test-123456").unwrap();
        assert_eq!(open(&KEY, b"aad", &nonce, &ct).unwrap().as_slice(), b"sk-test-123456");
        let (ct2, nonce2) = seal(&KEY, b"aad", b"sk-test-123456").unwrap();
        assert_ne!(nonce, nonce2);
        assert_ne!(ct, ct2);
    }

    #[test]
    fn wrong_key_or_associated_data_fails() {
        let (ct, nonce) = seal(&KEY, b"aroh/key/v1/a/s", b"secret-value").unwrap();
        assert!(open(&KEY, b"aroh/key/v1/b/s", &nonce, &ct).is_err());
        assert!(open(&[8u8; 32], b"aroh/key/v1/a/s", &nonce, &ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let (mut ct, nonce) = seal(&KEY, b"aad", b"secret-value").unwrap();
        ct[0] ^= 1;
        assert!(open(&KEY, b"aad", &nonce, &ct).is_err());
        let (ct, mut nonce) = seal(&KEY, b"aad", b"secret-value").unwrap();
        nonce[3] ^= 1;
        assert!(open(&KEY, b"aad", &nonce, &ct).is_err());
        assert!(open(&KEY, b"aad", &nonce[..4], &ct).is_err());
    }

    #[test]
    fn master_key_parses_and_never_prints() {
        let text = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let key = MasterKey::from_base64(&text).unwrap();
        assert_eq!(format!("{key:?}"), "MasterKey([redacted])");
        assert!(MasterKey::from_base64("not base64!").is_err());
        let short = base64::engine::general_purpose::STANDARD.encode([9u8; 16]);
        let err = MasterKey::from_base64(&short).unwrap_err().to_string();
        assert!(!err.contains(&short));
    }

    #[test]
    fn secret_and_vendor_checks() {
        assert!(check_secret(&Secret::new("short".into()).unwrap()).is_err());
        assert!(check_secret(&Secret::new("has space inside".into()).unwrap()).is_err());
        assert!(check_secret(&Secret::new("sk-ant-0123456789".into()).unwrap()).is_ok());
        assert!(check_vendor("anthropic").is_ok());
        assert!(check_vendor("Anthropic").is_err());
        assert!(check_vendor("").is_err());
    }
}
