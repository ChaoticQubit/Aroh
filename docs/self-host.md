# Self-host Aroh

Aroh needs two things on the server side: a Supabase backend (Postgres, Auth, REST, Realtime and Storage) and the Aroh engine (`crates/aroh-server`), which runs the agents and stores each account's model provider key encrypted. Pick one path:

- **A. Docker compose**: everything on one machine, the engine in worker mode. One command after a one-time setup.
- **B. Supabase cloud and Vercel**: hosted Supabase, the engine as a Vercel function in step mode, woken by `pg_cron`.

Then point the app at your server (section C). A build from source has no server set: on first launch, setup ends on **Use my own server**, which opens Settings.

## A. Docker compose

You need Docker with Compose v2 (or a compatible CLI) and Python 3. The stack is in `selfhost/`: Postgres 17 with pgvector and wal2json, GoTrue (Auth), PostgREST, Realtime, Storage (files on a volume), a one-shot migration job, the engine, and an nginx gateway on one port.

1. Clone the repository and open a terminal at its root.
2. Write the secrets once. The argument is the public address the app will use: `https://<your host>` behind TLS, or `http://localhost:8000` for a trial on this machine:

    ```sh
    python3 selfhost/setup.py https://aroh.example.com
    ```

    It writes `selfhost/.env` (mode 0600) with a database password, the JWT secret, the anon and service role keys signed with it, the engine master key and the Realtime keys. It prints no secret and refuses to overwrite an existing file. Keep a copy of `selfhost/.env` in your password manager: losing `AROH_MASTER_KEY` makes every stored provider key unreadable, and changing `POSTGRES_PASSWORD` after the first start locks the services out of the database.
3. Choose who may sign up. Add any of these to `selfhost/.env`:
    - `DISABLE_SIGNUP=true` once your own account exists, to close sign-ups.
    - `MAILER_AUTOCONFIRM=true` to skip email confirmation, for a private server without mail.
    - Otherwise set `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS` and `SMTP_ADMIN_EMAIL`, so Auth can send confirmation mails.
4. Optional push to iPhones: set `AROH_APNS_KEY`, `AROH_APNS_KEY_ID`, `AROH_APNS_TEAM_ID` and `AROH_APNS_TOPIC` (all four or none; `docs/deploy.md` section E shows how to get them). `AROH_APNS_TOPIC` is the bundle id of the app build you install.
5. Start the stack:

    ```sh
    docker compose -f selfhost/compose.yaml up -d --build
    ```

    The first start builds the engine in release mode (10 minutes or more). The `migrate` job waits for Auth and Storage, applies every file in `supabase/migrations/` that has not run yet (it records them in `aroh_admin.migrations`), and exits. On later starts it applies only new files.
6. Check it. Both must answer:

    ```sh
    curl -s http://localhost:8000/auth/v1/health
    curl -s http://localhost:8000/engine/healthz
    ```

7. Put TLS in front. The app accepts plain http only for `localhost` and `127.0.0.1`. The gateway listens on `127.0.0.1:8000`; any reverse proxy with certificates works. With Caddy, a `Caddyfile` of `aroh.example.com { reverse_proxy 127.0.0.1:8000 }` is enough. `HTTP_BIND` and `HTTP_PORT` in `selfhost/.env` move the gateway.
8. Update: `git pull`, then step 5 again. Back up the `pg` and `storage` volumes before each update.

What the stack does not run: `pg_cron`, `pg_net` and Vault are not in the image, so the migrations skip the cron and wake blocks. In worker mode the engine runs its job loop and its push loop in process, so nothing needs them.

The address the app uses for Supabase is `PUBLIC_URL`, and for the engine `PUBLIC_URL` followed by `/engine`.

## B. Supabase cloud and Vercel

`docs/deploy.md` is the step-by-step runbook for this path; follow it from section A to I. In short:

1. Create a Supabase project. Turn on the `vector`, `pg_cron` and `pg_net` extensions (deploy.md section B).
2. Apply the migrations with the Supabase CLI, which reads `supabase/config.toml` in this repo (deploy.md section C):

    ```sh
    supabase login
    supabase link --project-ref <ref>
    supabase db push
    ```

    `supabase db push` applies every file in `supabase/migrations/` in name order and records them, so the next run applies only new ones. The settings in `supabase/config.toml` other than the migrations apply to `supabase start` (local development) only; set Auth options for the cloud project in its dashboard.
3. Configure Auth and Realtime (deploy.md section D): your account, the sign-up policy, the `run_events` publication, and `AROH_JWKS_URL`.
4. Deploy the engine to Vercel with root directory `crates/aroh-vercel` and the environment variables of deploy.md section F.
5. Store the engine URL and a service token in Vault so `pg_cron` and the jobs trigger can wake the engine (deploy.md section G).
6. Run the smoke check (deploy.md section I).

## C. Point the app at your server

In the app, open **Settings**:

1. **Account**: **Project URL** is your Supabase URL (`PUBLIC_URL` for path A, `https://<ref>.supabase.co` for path B), and **Anon key** is `ANON_KEY` from `selfhost/.env` (path A) or the project's publishable key (path B).
2. Create an account or sign in.
3. **Server and budget**: **Server URL** is `PUBLIC_URL` followed by `/engine` (path A) or the Vercel domain (path B).
4. **Model provider**: enter your Claude key. The engine stores it encrypted under `AROH_MASTER_KEY`; the device keeps no copy.

To ship your own app build with your server as the default, set `AROH_DEFAULT_SUPABASE_URL`, `AROH_DEFAULT_SUPABASE_ANON_KEY` and `AROH_DEFAULT_SERVER_URL` in the environment of the build (for TestFlight, in `apps/aroh/.env.release`; see `apps/aroh/.env.release.example`). Your own bundle id needs a `tauri.conf` override and a regenerated Xcode project.

## Variables

`.env.example` at the repo root lists every variable Aroh reads, without values.

| Variable | Used by | Meaning |
|---|---|---|
| `PUBLIC_URL` | compose | Public address of the gateway, no trailing slash |
| `POSTGRES_PASSWORD` | compose | Password of the `postgres`, `authenticator` and `supabase_auth_admin` roles |
| `JWT_SECRET` | compose | HS256 secret shared by Auth, PostgREST, Realtime, Storage and the engine |
| `ANON_KEY`, `SERVICE_ROLE_KEY` | compose | JWTs signed with `JWT_SECRET`; the anon key goes into the app, the service key never leaves the server |
| `REALTIME_DB_ENC_KEY`, `REALTIME_SECRET_KEY_BASE` | compose | Realtime's own keys: 16 characters, and 64 or more |
| `HTTP_BIND`, `HTTP_PORT` | compose | Gateway address, default `127.0.0.1` and `8000` |
| `DISABLE_SIGNUP`, `MAILER_AUTOCONFIRM`, `SMTP_*` | compose | Auth sign-up policy and mail |
| `DATABASE_URL` | engine | Postgres URL; on Supabase cloud the session pooler on port 5432 |
| `AROH_MASTER_KEY` | engine | 32 random bytes, base64; encrypts the stored provider keys |
| `AROH_JWT_SECRET`, `AROH_JWKS_URL` | engine | How the engine verifies user tokens: the HS256 secret, the project's JWKS URL, or both |
| `AROH_JWT_AUDIENCE` | engine | Expected `aud` claim, default `authenticated` |
| `AROH_BIND`, `PORT` | engine | Listen address, default `0.0.0.0:8080` |
| `AROH_APNS_KEY`, `AROH_APNS_KEY_ID`, `AROH_APNS_TEAM_ID`, `AROH_APNS_TOPIC` | engine | Push through APNs; all four or none |
| `AROH_SEARCH_TOML` | engine | Path to a web search provider file inside the engine's filesystem; mount it as a volume |
| `AROH_ALLOW_CUSTOM_BASE_URL` | engine | `1` lets accounts point a provider at a custom base URL; off by default |
| `AROH_DEFAULT_SUPABASE_URL`, `AROH_DEFAULT_SUPABASE_ANON_KEY`, `AROH_DEFAULT_SERVER_URL` | app build | The server a build connects to until the user sets another; unset in a source build |
| `AROH_TEAM_ID` | iOS release | Apple team that signs a TestFlight build |
| `AROH_BUILD_NUMBER`, `AROH_ASC_KEY_PATH`, `AROH_ASC_KEY_ID`, `AROH_ASC_ISSUER_ID`, `AROH_UNSIGNED`, `AROH_IOS_OUT` | iOS release | Build number, App Store Connect API key (all three or none), unsigned dry run, output folder |
