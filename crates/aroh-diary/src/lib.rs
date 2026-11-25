//! End-to-end encrypted daily diary (Q85): the diary key, AES-256-GCM sealing
//! bound to account, table, row and key version, the plaintext entry and card
//! types with strict bounds, and the structuring prompt and output parser.
//! Shared by the app (device) and the engine (agents read toggle).

pub mod card;
pub mod structure;

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub use card::{AgentDay, Card, Entry, Source};
pub use structure::{parse, prompt, NumericField, Prompt, Structure, TrackerValue};

/// Length of a diary key in bytes (AES-256).
pub const KEY_LEN: usize = 32;
/// Length of a GCM nonce in bytes (96 bits, random per write).
pub const NONCE_LEN: usize = 12;
/// Highest key version accepted anywhere. Rotation adds one per toggle off.
pub const MAX_KEY_VERSION: i64 = 1_000_000;
/// Largest sealed entry ciphertext, base64 characters.
pub const ENTRY_CIPHERTEXT_MAX: usize = 16 * 1024;
/// Largest sealed day card or proposal summary ciphertext, base64 characters.
pub const CARD_CIPHERTEXT_MAX: usize = 96 * 1024;
/// Largest sealed proposal diff, base64, in characters.
pub const DIFF_CIPHERTEXT_MAX: usize = 512 * 1024;

/// What can go wrong. Messages never carry plaintext, key bytes or model text.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("cannot open the sealed value")]
    Crypto,
    #[error("sealed with key version {found}, this key is version {expected}")]
    Version { expected: i64, found: i64 },
    #[error("invalid {field}: {reason}")]
    Invalid { field: &'static str, reason: &'static str },
}

/// Result of every diary operation.
pub type Result<T> = std::result::Result<T, Error>;

fn invalid(field: &'static str, reason: &'static str) -> Error {
    Error::Invalid { field, reason }
}

/// The per-account diary key with its version. The bytes are zeroized on
/// drop and never printed (Debug shows `[redacted]`). Made on the device;
/// the engine only holds it while the agents read toggle is on.
#[derive(Clone)]
pub struct DiaryKey {
    version: i64,
    bytes: Zeroizing<[u8; KEY_LEN]>,
}

impl std::fmt::Debug for DiaryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DiaryKey(v{}, [redacted])", self.version)
    }
}

fn check_version(version: i64) -> Result<()> {
    if (1..=MAX_KEY_VERSION).contains(&version) {
        Ok(())
    } else {
        Err(invalid("key_version", "must be between 1 and 1000000"))
    }
}

impl DiaryKey {
    /// Draws a fresh random key for `version` from the OS generator.
    /// Returns `Invalid` for a version outside 1..=MAX_KEY_VERSION.
    pub fn generate(version: i64) -> Result<DiaryKey> {
        check_version(version)?;
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        OsRng.fill_bytes(bytes.as_mut());
        Ok(DiaryKey { version, bytes })
    }

    /// Builds a key from raw bytes, as read from the keychain or the vault.
    /// Returns `Invalid` for a wrong length or version; never echoes the bytes.
    pub fn from_bytes(version: i64, raw: &[u8]) -> Result<DiaryKey> {
        check_version(version)?;
        let bytes: [u8; KEY_LEN] = raw.try_into().map_err(|_| invalid("key", "must be 32 bytes"))?;
        Ok(DiaryKey { version, bytes: Zeroizing::new(bytes) })
    }

    /// Parses a standard base64 key of exactly 32 bytes for `version`.
    /// Returns `Invalid` for bad base64, length or version; never echoes input.
    pub fn from_base64(version: i64, text: &str) -> Result<DiaryKey> {
        if text.len() > 64 {
            return Err(invalid("key", "must be 32 bytes"));
        }
        let raw = Zeroizing::new(B64.decode(text.trim()).map_err(|_| invalid("key", "not base64"))?);
        DiaryKey::from_bytes(version, &raw)
    }

    /// Returns the key as standard base64 in zeroizing memory, for the keychain
    /// or the one upload to the engine vault.
    pub fn to_base64(&self) -> Zeroizing<String> {
        Zeroizing::new(B64.encode(self.bytes.as_slice()))
    }

    /// Returns the raw key bytes, for the engine vault seal.
    pub fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }

    /// Returns the key version.
    pub fn version(&self) -> i64 {
        self.version
    }
}

/// Which table a sealed value lives in; part of the associated data, so a
/// ciphertext cannot be moved between tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Entries,
    Days,
    Proposals,
    ProposalDiffs,
}

impl Table {
    /// The table name as used in the associated data and in SQL.
    pub fn name(self) -> &'static str {
        match self {
            Table::Entries => "diary_entries",
            Table::Days => "diary_days",
            Table::Proposals => "proposals",
            Table::ProposalDiffs => "proposals.diff",
        }
    }

    fn max_ciphertext(self) -> usize {
        match self {
            Table::Entries => ENTRY_CIPHERTEXT_MAX,
            Table::Days | Table::Proposals => CARD_CIPHERTEXT_MAX,
            Table::ProposalDiffs => DIFF_CIPHERTEXT_MAX,
        }
    }
}

/// One sealed value as stored and synced: the key version, the base64 nonce
/// and the base64 ciphertext with its GCM tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    pub key_version: i64,
    pub nonce: String,
    pub ciphertext: String,
}

impl Sealed {
    /// Checks the stored shape for `table`: version range, a 12-byte base64
    /// nonce and a base64 ciphertext within the table's cap.
    /// Returns `Invalid` naming the bad column.
    pub fn check(&self, table: Table) -> Result<()> {
        check_version(self.key_version)?;
        let nonce = B64.decode(&self.nonce).map_err(|_| invalid("nonce", "not base64"))?;
        if nonce.len() != NONCE_LEN {
            return Err(invalid("nonce", "must be 12 bytes"));
        }
        if self.ciphertext.is_empty() || self.ciphertext.len() > table.max_ciphertext() {
            return Err(invalid("ciphertext", "empty or too long"));
        }
        B64.decode(&self.ciphertext).map_err(|_| invalid("ciphertext", "not base64"))?;
        Ok(())
    }
}

fn aad(user: &str, table: Table, row: &str, version: i64) -> Vec<u8> {
    format!("aroh/diary/v1/{}/{user}/{row}/{version}", table.name()).into_bytes()
}

/// Encrypts `plain` for row `row` of `table` owned by `user` under `key`,
/// with a fresh random 96-bit nonce and the associated data binding account,
/// table, row and key version. Returns the sealed value.
pub fn seal(key: &DiaryKey, user: &str, table: Table, row: &str, plain: &[u8]) -> Result<Sealed> {
    let cipher = Aes256Gcm::new_from_slice(key.bytes.as_slice()).map_err(|_| Error::Crypto)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    assert_eq!(nonce.len(), NONCE_LEN);
    let ct = cipher
        .encrypt(&nonce, Payload { msg: plain, aad: &aad(user, table, row, key.version) })
        .map_err(|_| Error::Crypto)?;
    let sealed = Sealed { key_version: key.version, nonce: B64.encode(nonce), ciphertext: B64.encode(ct) };
    sealed.check(table)?;
    Ok(sealed)
}

/// Decrypts `sealed` for row `row` of `table` owned by `user`.
/// Returns the plaintext in zeroizing memory, `Version` when `key` is another
/// version, or `Crypto` when key, account, table, row or bytes do not match.
pub fn open(key: &DiaryKey, user: &str, table: Table, row: &str, sealed: &Sealed) -> Result<Zeroizing<Vec<u8>>> {
    if sealed.key_version != key.version {
        return Err(Error::Version { expected: key.version, found: sealed.key_version });
    }
    sealed.check(table)?;
    let nonce = B64.decode(&sealed.nonce).map_err(|_| Error::Crypto)?;
    let ct = B64.decode(&sealed.ciphertext).map_err(|_| Error::Crypto)?;
    let cipher = Aes256Gcm::new_from_slice(key.bytes.as_slice()).map_err(|_| Error::Crypto)?;
    cipher
        .decrypt(Nonce::from_slice(&nonce), Payload { msg: &ct, aad: &aad(user, table, row, key.version) })
        .map(Zeroizing::new)
        .map_err(|_| Error::Crypto)
}

/// Re-encrypts one sealed value from `old` to `new` for the same row.
/// The plaintext only lives in zeroizing memory in between.
/// Returns the value sealed under `new`, or the error of opening it.
pub fn rotate(old: &DiaryKey, new: &DiaryKey, user: &str, table: Table, row: &str, sealed: &Sealed) -> Result<Sealed> {
    assert!(new.version > old.version, "rotation moves to a newer key version");
    let plain = open(old, user, table, row, sealed)?;
    seal(new, user, table, row, &plain)
}

/// The id of a day's card row: uuid v5 of the account and the local day, so
/// two devices converge on one row. Returns the lowercase hyphenated uuid.
pub fn day_id(user: &str, day: &str) -> String {
    const NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x6a0f_4c1e_9b8d_4e57_a1d2_5c3b_7e90_d1a4);
    uuid::Uuid::new_v5(&NAMESPACE, format!("{user}/{day}").as_bytes()).hyphenated().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "8f6d2a10-5b7c-4e3d-9a1b-2c3d4e5f6a7b";
    const OTHER: &str = "1f6d2a10-5b7c-4e3d-9a1b-2c3d4e5f6a7b";
    const ROW: &str = "0193a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    const ROW2: &str = "0193a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5c";

    fn key(version: i64) -> DiaryKey {
        DiaryKey::generate(version).unwrap()
    }

    #[test]
    fn round_trip() {
        let k = key(1);
        let sealed = seal(&k, USER, Table::Entries, ROW, b"slept badly").unwrap();
        assert_eq!(sealed.key_version, 1);
        assert!(!sealed.ciphertext.contains("slept"));
        assert_eq!(open(&k, USER, Table::Entries, ROW, &sealed).unwrap().as_slice(), b"slept badly");
    }

    #[test]
    fn each_seal_draws_a_new_nonce() {
        let k = key(1);
        let a = seal(&k, USER, Table::Entries, ROW, b"same").unwrap();
        let b = seal(&k, USER, Table::Entries, ROW, b"same").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[test]
    fn wrong_key_fails() {
        let sealed = seal(&key(1), USER, Table::Entries, ROW, b"x").unwrap();
        assert_eq!(open(&key(1), USER, Table::Entries, ROW, &sealed), Err(Error::Crypto));
    }

    #[test]
    fn wrong_associated_data_fails() {
        let k = key(1);
        let sealed = seal(&k, USER, Table::Entries, ROW, b"x").unwrap();
        assert_eq!(open(&k, USER, Table::Entries, ROW2, &sealed), Err(Error::Crypto));
        assert_eq!(open(&k, OTHER, Table::Entries, ROW, &sealed), Err(Error::Crypto));
        assert_eq!(open(&k, USER, Table::Days, ROW, &sealed), Err(Error::Crypto));
        let same_bytes_v2 = DiaryKey::from_bytes(2, k.bytes()).unwrap();
        let relabelled = Sealed { key_version: 2, ..sealed.clone() };
        assert_eq!(open(&same_bytes_v2, USER, Table::Entries, ROW, &relabelled), Err(Error::Crypto));
        assert_eq!(open(&same_bytes_v2, USER, Table::Entries, ROW, &sealed), Err(Error::Version { expected: 2, found: 1 }));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let k = key(1);
        let sealed = seal(&k, USER, Table::Entries, ROW, b"hello there").unwrap();
        let mut raw = B64.decode(&sealed.ciphertext).unwrap();
        raw[0] ^= 1;
        let tampered = Sealed { ciphertext: B64.encode(&raw), ..sealed.clone() };
        assert_eq!(open(&k, USER, Table::Entries, ROW, &tampered), Err(Error::Crypto));
        let mut n = B64.decode(&sealed.nonce).unwrap();
        n[0] ^= 1;
        let renonced = Sealed { nonce: B64.encode(&n), ..sealed };
        assert_eq!(open(&k, USER, Table::Entries, ROW, &renonced), Err(Error::Crypto));
    }

    #[test]
    fn rotation_moves_to_the_new_key_and_the_old_one_no_longer_opens() {
        let (v1, v2) = (key(1), key(2));
        let sealed = seal(&v1, USER, Table::Days, ROW, b"card").unwrap();
        let rotated = rotate(&v1, &v2, USER, Table::Days, ROW, &sealed).unwrap();
        assert_eq!(rotated.key_version, 2);
        assert_eq!(open(&v2, USER, Table::Days, ROW, &rotated).unwrap().as_slice(), b"card");
        assert!(open(&v1, USER, Table::Days, ROW, &rotated).is_err());
    }

    #[test]
    fn key_bounds_and_redaction() {
        assert!(DiaryKey::generate(0).is_err());
        assert!(DiaryKey::generate(MAX_KEY_VERSION + 1).is_err());
        assert!(DiaryKey::from_bytes(1, &[0u8; 31]).is_err());
        assert!(DiaryKey::from_base64(1, "not base64!").is_err());
        assert!(DiaryKey::from_base64(1, &B64.encode([1u8; 16])).is_err());
        let k = key(3);
        let text = k.to_base64();
        let back = DiaryKey::from_base64(3, &text).unwrap();
        assert_eq!(back.bytes(), k.bytes());
        assert_eq!(format!("{k:?}"), "DiaryKey(v3, [redacted])");
        let err = DiaryKey::from_base64(1, &B64.encode([9u8; 16])).unwrap_err().to_string();
        assert!(!err.contains(&B64.encode([9u8; 16])));
    }

    #[test]
    fn stored_shape_is_checked() {
        let k = key(1);
        let good = seal(&k, USER, Table::Entries, ROW, b"x").unwrap();
        assert!(Sealed { nonce: B64.encode([0u8; 8]), ..good.clone() }.check(Table::Entries).is_err());
        assert!(Sealed { ciphertext: String::new(), ..good.clone() }.check(Table::Entries).is_err());
        assert!(Sealed { ciphertext: "A".repeat(ENTRY_CIPHERTEXT_MAX + 4), ..good.clone() }.check(Table::Entries).is_err());
        assert!(Sealed { key_version: 0, ..good }.check(Table::Entries).is_err());
        assert!(seal(&k, USER, Table::Entries, ROW, &vec![b'a'; ENTRY_CIPHERTEXT_MAX]).is_err());
    }

    #[test]
    fn day_id_is_stable_per_account_and_day() {
        assert_eq!(day_id(USER, "2026-11-26"), day_id(USER, "2026-11-26"));
        assert_ne!(day_id(USER, "2026-11-26"), day_id(USER, "2026-11-27"));
        assert_ne!(day_id(USER, "2026-11-26"), day_id(OTHER, "2026-11-26"));
        assert_eq!(uuid::Uuid::parse_str(&day_id(USER, "2026-11-26")).unwrap().get_version_num(), 5);
    }
}
