use chrono::{DateTime, NaiveDate, Utc};
use deadpool_postgres::{Hook, HookError, Manager, ManagerConfig, Object, Pool, PoolError, RecyclingMethod, Runtime};
use futures_util::TryStreamExt;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;
use tokio_postgres::config::SslMode;
use tokio_postgres::error::DbError;
use tokio_postgres::types::{FromSql, ToSql, Type};
use tokio_postgres::{Client, Row};
use tokio_postgres_rustls::MakeRustlsConnect;
use uuid::Uuid;

/// Longest a pooled connection's liveness check may take before the pool
/// drops it and opens another, so a socket frozen with a Vercel instance
/// never hangs a request.
const RECYCLE_TIMEOUT: Duration = Duration::from_secs(3);

/// A database error: from Postgres, the pool, the connection URL, or a
/// query that expected a row and got none. `Display` keeps the server's
/// message (like sqlx did) and never includes the connection URL.
#[derive(Debug)]
pub enum Error {
    Database(tokio_postgres::Error),
    RowNotFound,
    Pool(String),
    Config(String),
}

impl Error {
    /// The Postgres error behind this one, when the server raised it.
    /// Returns `None` for pool, URL, I/O and row-count errors.
    pub fn as_database_error(&self) -> Option<&DbError> {
        match self {
            Error::Database(e) => e.as_db_error(),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Database(e) => match e.as_db_error() {
                Some(db) => write!(f, "error returned from database: {}", db.message()),
                None => {
                    write!(f, "{e}")?;
                    let mut source = std::error::Error::source(e);
                    while let Some(cause) = source {
                        write!(f, ": {cause}")?;
                        source = cause.source();
                    }
                    Ok(())
                }
            },
            Error::RowNotFound => f.write_str("no rows returned by a query that expected to return at least one row"),
            Error::Pool(m) => write!(f, "database pool: {m}"),
            Error::Config(m) => write!(f, "database url: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<tokio_postgres::Error> for Error {
    fn from(e: tokio_postgres::Error) -> Self {
        Error::Database(e)
    }
}

impl From<PoolError> for Error {
    fn from(e: PoolError) -> Self {
        match e {
            PoolError::Backend(e) => Error::Database(e),
            other => Error::Pool(other.to_string()),
        }
    }
}

/// The one Postgres type a bound Rust value is sent as, mirroring sqlx's
/// encode table. `pg_type` returns it; the value's `ToSql` must accept it,
/// which the unit tests check for every impl.
pub trait Encode {
    fn pg_type() -> Type;
}

/// A bindable type that can also be an array element.
/// `array_type` returns the Postgres array type of `Vec<Self>` and `[Self]`.
pub trait ArrayElem: Encode {
    fn array_type() -> Type;
}

macro_rules! encode {
    ($($t:ty => $scalar:ident $(, $array:ident)?;)*) => {
        $(
            impl Encode for $t {
                fn pg_type() -> Type {
                    Type::$scalar
                }
            }
            $(
                impl ArrayElem for $t {
                    fn array_type() -> Type {
                        Type::$array
                    }
                }
            )?
        )*
    };
}

encode! {
    bool => BOOL, BOOL_ARRAY;
    i16 => INT2, INT2_ARRAY;
    i32 => INT4, INT4_ARRAY;
    i64 => INT8, INT8_ARRAY;
    f32 => FLOAT4, FLOAT4_ARRAY;
    f64 => FLOAT8, FLOAT8_ARRAY;
    String => TEXT, TEXT_ARRAY;
    str => TEXT, TEXT_ARRAY;
    Uuid => UUID, UUID_ARRAY;
    DateTime<Utc> => TIMESTAMPTZ, TIMESTAMPTZ_ARRAY;
    NaiveDate => DATE, DATE_ARRAY;
    serde_json::Value => JSONB, JSONB_ARRAY;
}

impl Encode for Vec<u8> {
    fn pg_type() -> Type {
        Type::BYTEA
    }
}

impl Encode for [u8] {
    fn pg_type() -> Type {
        Type::BYTEA
    }
}

impl<T: ArrayElem + ?Sized> ArrayElem for &T {
    fn array_type() -> Type {
        T::array_type()
    }
}

impl<T: ArrayElem> Encode for Vec<T> {
    fn pg_type() -> Type {
        T::array_type()
    }
}

impl<T: ArrayElem> Encode for [T] {
    fn pg_type() -> Type {
        T::array_type()
    }
}

impl<T: Encode> Encode for Option<T> {
    fn pg_type() -> Type {
        T::pg_type()
    }
}

impl<T: Encode + ?Sized> Encode for &T {
    fn pg_type() -> Type {
        T::pg_type()
    }
}

/// Decodes one result row by column position into a tuple, the way sqlx's
/// tuple `FromRow` did. Extra columns are ignored; a missing or mistyped
/// column is an error.
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self, Error>;
}

macro_rules! from_row {
    ($($i:tt $t:ident),+) => {
        impl<$($t),+> FromRow for ($($t,)+)
        where
            $($t: for<'r> FromSql<'r>),+
        {
            fn from_row(row: &Row) -> Result<Self, Error> {
                Ok(($(row.try_get::<usize, $t>($i)?,)+))
            }
        }
    };
}

from_row!(0 A);
from_row!(0 A, 1 B);
from_row!(0 A, 1 B, 2 C);
from_row!(0 A, 1 B, 2 C, 3 D);
from_row!(0 A, 1 B, 2 C, 3 D, 4 E);
from_row!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F);
from_row!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G);
from_row!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H);
from_row!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H, 8 I);
from_row!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H, 8 I, 9 J);
from_row!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H, 8 I, 9 J, 10 K);
from_row!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H, 8 I, 9 J, 10 K, 11 L);

/// Where a statement runs: on a connection checked out of the pool for
/// this one statement, or on the connection a transaction holds.
pub enum Target<'e> {
    Pool(&'e PgPool),
    Held(&'e PgConnection),
}

/// What a statement may run on: `&PgPool` checks out a connection for it,
/// `&mut *tx` runs it on the transaction's connection.
/// `target` returns which.
pub trait Executor<'e> {
    fn target(self) -> Target<'e>;
}

impl<'e> Executor<'e> for &'e PgPool {
    fn target(self) -> Target<'e> {
        Target::Pool(self)
    }
}

impl<'e> Executor<'e> for &'e mut PgConnection {
    fn target(self) -> Target<'e> {
        Target::Held(self)
    }
}

/// The number of rows a statement changed, read from its command tag.
#[derive(Debug, Clone, Copy)]
pub struct PgQueryResult {
    rows_affected: u64,
}

impl PgQueryResult {
    /// Rows inserted, updated or deleted; 0 for other statements.
    pub fn rows_affected(&self) -> u64 {
        self.rows_affected
    }
}

type Arg<'q> = (Box<dyn ToSql + Sync + Send + 'q>, Type);

/// A statement and its bound values. It runs as one unnamed extended
/// protocol message (`query_typed`): every value is a typed parameter,
/// never part of the SQL text, and no named statement is prepared.
pub struct Query<'q> {
    sql: &'q str,
    args: Vec<Arg<'q>>,
}

/// A `Query` whose rows decode into the tuple `O` by position.
pub struct QueryAs<'q, O> {
    inner: Query<'q>,
    out: PhantomData<fn() -> O>,
}

/// A `Query` whose first column decodes into `T`.
pub struct QueryScalar<'q, T> {
    inner: Query<'q>,
    out: PhantomData<fn() -> T>,
}

/// Starts a statement over `sql` with no bound values yet.
pub fn query(sql: &str) -> Query<'_> {
    Query { sql, args: Vec::new() }
}

/// Starts a statement over `sql` whose rows decode into the tuple `O`.
pub fn query_as<O: FromRow>(sql: &str) -> QueryAs<'_, O> {
    QueryAs { inner: query(sql), out: PhantomData }
}

/// Starts a statement over `sql` whose first column decodes into `T`.
pub fn query_scalar<T>(sql: &str) -> QueryScalar<'_, T> {
    QueryScalar { inner: query(sql), out: PhantomData }
}

impl<'q> Query<'q> {
    /// Binds `value` to the next `$n` parameter, sent as `T::pg_type()`.
    pub fn bind<T: Encode + ToSql + Sync + Send + 'q>(mut self, value: T) -> Self {
        self.args.push((Box::new(value), T::pg_type()));
        self
    }

    /// Runs the statement on `executor` and returns the rows it changed.
    pub async fn execute<'e, E: Executor<'e>>(self, executor: E) -> Result<PgQueryResult, Error> {
        let (_, rows_affected) = self.run(executor).await?;
        Ok(PgQueryResult { rows_affected })
    }

    /// Runs the statement, reads the stream to its end (the command tag
    /// arrives last) and returns every row and the rows-affected count.
    async fn run<'e, E: Executor<'e>>(self, executor: E) -> Result<(Vec<Row>, u64), Error> {
        let pooled;
        let client: &Client = match executor.target() {
            Target::Pool(pool) => {
                pooled = pool.pool.get().await?;
                &pooled
            }
            Target::Held(conn) => &conn.0,
        };
        let params = self.args.iter().map(|(value, ty)| (&**value as &(dyn ToSql + Sync), ty.clone()));
        let stream = client.query_typed_raw(self.sql, params).await?;
        let mut stream = std::pin::pin!(stream);
        let mut rows = Vec::new();
        while let Some(row) = stream.try_next().await? {
            rows.push(row);
        }
        let affected = stream.rows_affected().unwrap_or(0);
        Ok((rows, affected))
    }
}

impl<'q, O: FromRow> QueryAs<'q, O> {
    /// Binds `value` to the next `$n` parameter, sent as `T::pg_type()`.
    pub fn bind<T: Encode + ToSql + Sync + Send + 'q>(mut self, value: T) -> Self {
        self.inner = self.inner.bind(value);
        self
    }

    /// The first row, or `Error::RowNotFound` when there is none.
    pub async fn fetch_one<'e, E: Executor<'e>>(self, executor: E) -> Result<O, Error> {
        self.fetch_optional(executor).await?.ok_or(Error::RowNotFound)
    }

    /// The first row, or `None` when there is none.
    pub async fn fetch_optional<'e, E: Executor<'e>>(self, executor: E) -> Result<Option<O>, Error> {
        let (rows, _) = self.inner.run(executor).await?;
        rows.first().map(O::from_row).transpose()
    }

    /// Every row, in the order the server sent them.
    pub async fn fetch_all<'e, E: Executor<'e>>(self, executor: E) -> Result<Vec<O>, Error> {
        let (rows, _) = self.inner.run(executor).await?;
        rows.iter().map(O::from_row).collect()
    }
}

impl<'q, T> QueryScalar<'q, T>
where
    T: for<'r> FromSql<'r>,
{
    /// Binds `value` to the next `$n` parameter, sent as `V::pg_type()`.
    pub fn bind<V: Encode + ToSql + Sync + Send + 'q>(mut self, value: V) -> Self {
        self.inner = self.inner.bind(value);
        self
    }

    /// The first row's first column, or `Error::RowNotFound`.
    pub async fn fetch_one<'e, E: Executor<'e>>(self, executor: E) -> Result<T, Error> {
        self.fetch_optional(executor).await?.ok_or(Error::RowNotFound)
    }

    /// The first row's first column, or `None` when there is no row.
    pub async fn fetch_optional<'e, E: Executor<'e>>(self, executor: E) -> Result<Option<T>, Error> {
        let (rows, _) = self.inner.run(executor).await?;
        Ok(rows.first().map(|row| row.try_get::<usize, T>(0)).transpose()?)
    }

    /// Every row's first column.
    pub async fn fetch_all<'e, E: Executor<'e>>(self, executor: E) -> Result<Vec<T>, Error> {
        let (rows, _) = self.inner.run(executor).await?;
        Ok(rows.iter().map(|row| row.try_get::<usize, T>(0)).collect::<Result<_, _>>()?)
    }
}

/// One pooled connection held for a transaction. Statements reach it
/// through `&mut *tx`.
pub struct PgConnection(Object);

/// A transaction on one checked-out connection: `begin` sends BEGIN on it,
/// every statement through `&mut *tx` runs on it, `commit` sends COMMIT.
/// Dropped without a successful commit, its connection is closed instead of
/// going back to the pool, so the server rolls the transaction back.
pub struct Transaction {
    conn: Option<PgConnection>,
}

impl Deref for Transaction {
    type Target = PgConnection;

    fn deref(&self) -> &PgConnection {
        self.conn.as_ref().expect("an open transaction holds its connection")
    }
}

impl DerefMut for Transaction {
    fn deref_mut(&mut self) -> &mut PgConnection {
        self.conn.as_mut().expect("an open transaction holds its connection")
    }
}

impl Transaction {
    /// Sends COMMIT and returns the connection to the pool.
    /// On error the connection is discarded with the transaction.
    pub async fn commit(mut self) -> Result<(), Error> {
        query("commit").execute(&mut *self).await?;
        let conn = self.conn.take();
        assert!(conn.is_some(), "a committed transaction held its connection");
        drop(conn);
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(Object::take(conn.0));
        }
    }
}

/// A lazy Postgres pool (deadpool-postgres): no connection until the first
/// checkout, each reused connection checked alive first, idle ones dropped.
/// Cheap to clone; clones share the pool.
#[derive(Clone)]
pub struct PgPool {
    pool: Pool,
}

impl PgPool {
    /// Checks out one connection and sends BEGIN on it.
    /// Returns the open transaction.
    pub async fn begin(&self) -> Result<Transaction, Error> {
        let mut tx = Transaction { conn: Some(PgConnection(self.pool.get().await?)) };
        query("begin").execute(&mut *tx).await?;
        Ok(tx)
    }

    /// Closes the pool: idle connections close now, checked-out ones when
    /// they come back, and later checkouts fail.
    pub async fn close(&self) {
        self.pool.close();
    }
}

/// Builds a `PgPool`, with the sqlx builder methods the engine uses.
/// Defaults: 10 connections, 30 s acquire timeout, 10 min idle timeout.
#[derive(Debug, Clone)]
pub struct PgPoolOptions {
    max_connections: usize,
    acquire_timeout: Duration,
    idle_timeout: Duration,
}

impl Default for PgPoolOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl PgPoolOptions {
    /// The defaults above.
    pub fn new() -> Self {
        Self { max_connections: 10, acquire_timeout: Duration::from_secs(30), idle_timeout: Duration::from_secs(600) }
    }

    /// Most connections the pool opens at once; at least 1.
    pub fn max_connections(mut self, max: u32) -> Self {
        assert!(max >= 1, "a pool needs at least one connection");
        self.max_connections = usize::try_from(max).expect("u32 fits usize");
        self
    }

    /// The pool is lazy and keeps no connection open at rest, so `min`
    /// must be 0. Kept so callers state that intent.
    pub fn min_connections(self, min: u32) -> Self {
        assert_eq!(min, 0, "the pool is lazy and keeps no minimum");
        self
    }

    /// A connection unused this long is closed instead of reused.
    pub fn idle_timeout(mut self, idle: Duration) -> Self {
        self.idle_timeout = idle;
        self
    }

    /// Longest a checkout may wait for a free slot, and separately for a
    /// new connection to open.
    pub fn acquire_timeout(mut self, timeout: Duration) -> Self {
        self.acquire_timeout = timeout;
        self
    }

    /// Builds the pool for `url`; the same as `connect_lazy`, async for the
    /// callers written against sqlx. Opens no connection.
    pub async fn connect(self, url: &str) -> Result<PgPool, Error> {
        self.connect_lazy(url)
    }

    /// Builds the pool for `url` without connecting. `sslmode` in the URL:
    /// absent or `disable` = plain TCP, `prefer` = TLS if offered, `require`
    /// = TLS. Errors name a bad URL setting, never the URL itself.
    pub fn connect_lazy(self, url: &str) -> Result<PgPool, Error> {
        let config = pg_config(url)?;
        let manager = Manager::from_config(config, tls(), ManagerConfig { recycling_method: RecyclingMethod::Verified });
        let idle = self.idle_timeout;
        let pool = Pool::builder(manager)
            .max_size(self.max_connections)
            .runtime(Runtime::Tokio1)
            .wait_timeout(Some(self.acquire_timeout))
            .create_timeout(Some(self.acquire_timeout))
            .recycle_timeout(Some(RECYCLE_TIMEOUT))
            .pre_recycle(Hook::sync_fn(move |_, metrics| {
                if metrics.last_used() >= idle {
                    return Err(HookError::message("idle too long"));
                }
                Ok(())
            }))
            .build()
            .map_err(|e| Error::Pool(e.to_string()))?;
        Ok(PgPool { pool })
    }
}

/// Parses `url` into a driver config. tokio-postgres defaults to
/// `sslmode=prefer`; sqlx and the engine's URLs without `sslmode` meant
/// plain TCP, so an absent `sslmode` becomes `disable`. Only URLs are
/// accepted: a `key=value` string would hide its `sslmode` from that check.
fn pg_config(url: &str) -> Result<tokio_postgres::Config, Error> {
    if !(url.starts_with("postgres://") || url.starts_with("postgresql://")) {
        return Err(Error::Config("expected a postgres:// or postgresql:// URL".into()));
    }
    let mut config: tokio_postgres::Config = url.parse().map_err(|e: tokio_postgres::Error| Error::Config(Error::Database(e).to_string()))?;
    let has_sslmode = url.split_once('?').is_some_and(|(_, q)| q.split('&').any(|kv| kv.starts_with("sslmode=")));
    if !has_sslmode {
        config.ssl_mode(SslMode::Disable);
    }
    Ok(config)
}

/// The TLS connector: rustls with an explicit ring provider (never the
/// process default) that encrypts without verifying the certificate, as
/// libpq and sqlx do under `sslmode=require`.
fn tls() -> MakeRustlsConnect {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Unverified(provider)))
        .with_no_client_auth();
    MakeRustlsConnect::new(config)
}

/// Accepts any server certificate (`sslmode=require` semantics) while still
/// checking the handshake signatures with the ring provider's algorithms.
#[derive(Debug)]
struct Unverified(Arc<CryptoProvider>);

impl ServerCertVerifier for Unverified {
    fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    fn sends<T: Encode + ToSql>(value: T, expected: Type) {
        let ty = T::pg_type();
        assert_eq!(ty, expected, "{}", std::any::type_name::<T>());
        let mut buf = BytesMut::new();
        value.to_sql_checked(&ty, &mut buf).unwrap_or_else(|e| panic!("{} encodes as {ty}: {e}", std::any::type_name::<T>()));
    }

    #[test]
    fn every_bound_type_is_sent_as_its_sqlx_type_and_encodes() {
        let id = Uuid::now_v7();
        let at = DateTime::<Utc>::from_timestamp(1_795_348_800, 0).unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let words = vec!["a".to_string(), "b".to_string()];
        sends(true, Type::BOOL);
        sends(7i16, Type::INT2);
        sends(7i32, Type::INT4);
        sends(7i64, Type::INT8);
        sends(1.5f32, Type::FLOAT4);
        sends(1.5f64, Type::FLOAT8);
        sends("text", Type::TEXT);
        sends("text".to_string(), Type::TEXT);
        sends::<&String>(&"text".to_string(), Type::TEXT);
        sends(id, Type::UUID);
        sends::<&Uuid>(&id, Type::UUID);
        sends(at, Type::TIMESTAMPTZ);
        sends(day, Type::DATE);
        sends(serde_json::json!({"k": [1, 2]}), Type::JSONB);
        sends(&serde_json::json!(null), Type::JSONB);
        sends(words.clone(), Type::TEXT_ARRAY);
        sends(&words, Type::TEXT_ARRAY);
        sends(words.as_slice(), Type::TEXT_ARRAY);
        sends(vec![id], Type::UUID_ARRAY);
        sends(vec![1i64], Type::INT8_ARRAY);
        sends(&["a", "b"][..], Type::TEXT_ARRAY);
        sends(vec![1u8, 2], Type::BYTEA);
        sends(&[1u8, 2][..], Type::BYTEA);
        sends(None::<String>, Type::TEXT);
        sends(Some(id), Type::UUID);
        sends(None::<DateTime<Utc>>, Type::TIMESTAMPTZ);
        sends(Some(serde_json::json!({})), Type::JSONB);
        sends(None::<Vec<String>>, Type::TEXT_ARRAY);
    }

    #[test]
    fn a_url_without_sslmode_is_plain_tcp_and_sslmode_is_kept() {
        let plain = pg_config("postgres://u:p@h:5432/d").unwrap();
        assert_eq!(plain.get_ssl_mode(), SslMode::Disable);
        let required = pg_config("postgres://u:p@h:6543/d?sslmode=require").unwrap();
        assert_eq!(required.get_ssl_mode(), SslMode::Require);
        let preferred = pg_config("postgres://u:p@h:6543/d?application_name=x&sslmode=prefer").unwrap();
        assert_eq!(preferred.get_ssl_mode(), SslMode::Prefer);
    }

    #[test]
    fn a_bad_url_error_never_echoes_the_url() {
        let err = pg_config("postgres://u:hunter2@h:5432/d?sslmode=verify-full").unwrap_err().to_string();
        assert!(!err.contains("hunter2") && !err.contains("verify-full"), "{err}");
        let err = pg_config("postgres://u:hunter2@h:5432/d?bogus=1").unwrap_err().to_string();
        assert!(!err.contains("hunter2"), "{err}");
        let err = pg_config("host=h user=u password=hunter2 sslmode=require").unwrap_err().to_string();
        assert!(!err.contains("hunter2") && err.contains("postgres://"), "{err}");
    }

    #[test]
    fn the_tls_connector_builds_without_a_process_default_provider() {
        let _ = tls();
    }
}
