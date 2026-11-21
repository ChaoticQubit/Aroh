# aroh-sync design

Offline-first last-write-wins row sync between local SQLite and Supabase (Postgres + Auth + RLS). Single user. No PowerSync, no ElectricSQL, no self-run server.

## HLC

`updated_at` is an HLC string, fixed width, compared as a plain string:
`<unix millis, 13 digits zero-padded>-<counter, 4 lowercase hex>-<node id, 16 lowercase hex>`,
for example `1763683200000-0000-9f2c4a1b7e3d5c60`. Format agreed with foundation and recorded in `docs/decisions.md`. `aroh-core` provides `tick()` and `receive(remote)`. Until `aroh-core` lands, a tiny local helper exists for tests only.

## Merge rule

Rows are JSON maps with a primary key (`id`, or `key` for `settings`), `updated_at` and `deleted` (absent on `settings`, treated as 0).

- No local row: insert the remote row. Tombstones are inserted too.
- `remote.updated_at > local.updated_at`: remote wins.
- `remote.updated_at < local.updated_at`: local kept.
- Equal: tombstone wins (`deleted = 1` beats 0), otherwise local kept. Equal HLC includes the node id, so equal strings mean the same write.
- Tombstones are never purged in the MVP.

## Supabase schema (`supabase/migrations/`)

- Tables `goals, actions, plans, replans, tracker_entries, plugins, settings` with the same column names as SQLite. Types `text`, `integer`, `real`. `deleted` and `enabled` are `integer` so JSON matches SQLite.
- Every table: `user_id uuid not null default auth.uid() references auth.users on delete cascade`, primary key `(user_id, id)` (`settings`: `(user_id, key)`).
- `server_seq bigint` set from one shared sequence by a `BEFORE INSERT OR UPDATE` trigger, index on `(user_id, server_seq)`.
- The same trigger enforces LWW on the server: on UPDATE, skip (return NULL) when `NEW.updated_at < OLD.updated_at`, or when equal and not (`NEW.deleted = 1` and `OLD.deleted = 0`).
- RLS on every table. Owner-only policies for select, insert, update, delete: `user_id = auth.uid()` in `using` and `with check`. Grants to `authenticated` only; `anon` gets nothing.

## Client (`crates/aroh-sync`)

Dependencies: `reqwest` (default-features off, `rustls-tls`, `json`), `tokio`, `rusqlite` (bundled), `keyring`, `serde`, `serde_json`, `thiserror`.

- Cursors live in a local, non-synced table `sync_state(key TEXT PRIMARY KEY, value TEXT)`, keys `push:<table>` and `pull:<table>`.
- Push: per table, local rows with `updated_at > push cursor`, sent to `POST /rest/v1/<table>?on_conflict=user_id,id` (`user_id,key` for settings) with `Prefer: resolution=merge-duplicates`, batches of at most 500. Advance the push cursor to the max pushed `updated_at` only after a 2xx.
- Pull: `GET /rest/v1/<table>?server_seq=gt.<cursor>&order=server_seq.asc&limit=500`, loop until a short page. Drop `user_id` and `server_seq`, merge each row, call HLC `receive`, and store the pull cursor in the same SQLite transaction as the page.
- Auth through GoTrue: password sign-in (`POST /auth/v1/token?grant_type=password`), email OTP code (`POST /auth/v1/otp`, then `POST /auth/v1/verify` with `type=email`), refresh (`grant_type=refresh_token`) when the access token is expired.
- Keychain (`keyring`), service `aroh`: account `supabase-session` holds the session JSON, account `supabase-config` holds the project URL and anon key. Dev override: env `AROH_SUPABASE_URL`, `AROH_SUPABASE_ANON_KEY`.
- The project URL must be `https`; `http` only for `localhost` / `127.0.0.1`.
- Never log tokens or keys. Never commit keys.
- Public API, Tauri-agnostic: `configure(url, anon_key)`, `sign_in_password`, `request_otp`, `verify_otp`, `sign_out`, `sync_now(&mut Connection, on_remote: &mut dyn FnMut(&str)) -> SyncReport { pushed, pulled, at }` where `on_remote` feeds each remote HLC to `Hlc::receive`, `status(&Connection) -> SyncStatus { signed_in, email, last_sync_at, last_error }`. `last_sync_at` (unix millis) and `last_error` live in `sync_state`.

## Tauri commands

`apps/aroh/src-tauri/src/commands/sync.rs` with `sync_sign_in`, `sync_now`, `sync_status`, registered after foundation lands. UI: a "Sync now" button and status in `/settings`.

## Tests (critical logic only, in Docker)

- Merge: newer remote wins; older remote loses; newer tombstone beats an older edit; an edit older than a tombstone does not resurrect the row; equal HLC is deterministic and merge order converges.
- Cursor roundtrip in in-memory SQLite, and a pull page commits atomically with its cursor.
- `supabase/tests/run.sh`: starts `postgres:16`, stubs Supabase auth (schema `auth`, `auth.users`, `auth.uid()` reading `request.jwt.claim.sub`, roles `anon` and `authenticated`), applies the migrations, runs `supabase/tests/rls.sql`: user A sees only A's rows, cannot write with B's `user_id`, `anon` sees nothing, the server LWW trigger skips an older update. Exits non-zero on failure and removes its container.

## Known limits

- `server_seq` gaps from concurrent transactions can make a pull skip a row committed late. Single user makes this rare; upgrade to a commit-time watermark if multi-device writes become concurrent.
