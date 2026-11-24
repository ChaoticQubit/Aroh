# Production deploy runbook

This runbook puts the Aroh server engine on Vercel Hobby in step mode, connected to your Supabase project, with push through APNs. Do the steps in order. Each step is one action.

Rules for the whole runbook:

- Never paste a secret into a file in this repo, a commit, an issue or a chat. Secrets go into the Vercel environment, Supabase Vault or your password manager.
- Wherever this runbook says "save in your password manager", save the value under the name it gives.
- `<ref>` is your Supabase project ref, the part before `.supabase.co` in the Project URL.
- `<engine>` is the Vercel production domain, for example `https://<project>.vercel.app`, with no trailing slash.

How it fits together: the app talks to Supabase (sign-in, sync, Realtime) and to the engine (`server_url`). In Supabase, `pg_cron` calls `<engine>/v1/tick` and `<engine>/v1/push/tick` every minute. A trigger on `jobs` calls `<engine>/v1/hooks/enqueue` as soon as a run is queued. Each call is one Vercel request, and each request is capped at 300 s.

## A. Supabase: check what is already there

1. Open https://supabase.com/dashboard and select your project.
2. Open **SQL Editor**, click **New query**, paste this, and click **Run**:

   ```sql
   select
     to_regclass('public.goals') is not null as any_aroh_tables,
     to_regclass('public.categories') is not null as m20261121_rebuild,
     to_regclass('public.user_keys') is not null as m20261122_server_engine,
     to_regclass('public.devices') is not null as m20261123130000_push_devices,
     to_regclass('public.action_events') is not null as m20261123150000_triggers_learning,
     exists (select 1 from information_schema.columns where table_schema = 'public' and column_name = 'day_kind') as m20261124_day_kind,
     exists (select 1 from pg_trigger where tgname = 'jobs_wake') as m20261125_step_mode_wake;
   ```

3. Read the result:
   - All `false`: the project is fresh. Apply every migration in section C.
   - `any_aroh_tables` is `true` and `m20261121_rebuild` is `false`: the project has the old MVP tables. Migration `20261121000000_rebuild.sql` **drops** `goals`, `actions`, `plans`, `replans`, `tracker_entries`, `plugins` and `settings`. If you want that data, open **Table Editor**, select each of those tables, and use **Export** > **Export table as CSV** before section C.
   - Some columns `true`: apply only the migrations whose column is `false`, in order.

## B. Supabase: extensions

4. Open **Database** > **Extensions**, search `vector`, and turn it on (schema `extensions`).
5. On the same page, search `pg_cron` and turn it on.
6. On the same page, search `pg_net` and turn it on.

## C. Supabase: migrations

The migrations are the files in `supabase/migrations/`, applied once each in name order. The first is `20261120000000_init.sql`; new releases add more. The Supabase CLI (https://supabase.com/docs/guides/local-development/cli/getting-started) applies them and records which ran, using `supabase/config.toml` in this repo.

7. In Terminal, at the repo root, run `supabase login` and finish the sign-in in the browser.
8. Run `supabase link --project-ref <ref>`, where `<ref>` is the part before `.supabase.co` in your project URL. Enter the database password when asked.
9. Run `supabase db push --dry-run`. On a new project it lists every file in `supabase/migrations/`. On a project where some files were applied by hand in the SQL Editor, mark each of those as applied first, with its 14-digit prefix: `supabase migration repair --status applied 20261120000000`, and so on.
10. Run `supabase db push` and confirm. Every file must apply; stop at the first error and report it.
11. Without the CLI: open **SQL Editor** and run each file of `supabase/migrations/` in name order, one file per query, all of it, once. Skip `20261120000000_init.sql` on a project that already has the Aroh tables. Every run must end with "Success".
12. In the SQL Editor, run `select id, public from storage.buckets where id = 'goal-files';`. It must return one row with `public` false. If it returns none, run `supabase/migrations/20261127020000_goal_files_bucket.sql` again: it creates the attachments bucket only when Storage is set up.
13. Run `select count(*) from supabase_migrations.schema_migrations;` (CLI route only). It must equal the number of files in `supabase/migrations/`.
14. Run step 2 again. Every column must now be `true`.
15. Run this query and check that it lists `aroh-tick` and `aroh-push`, both `* * * * *`:

    ```sql
    select jobname, schedule, active from cron.job order by jobname;
    ```

    If a job is missing because a migration ran before pg_cron was on, copy only the last `do $$ ... $$;` block of the migration that schedules it (`20261123150000_triggers_learning.sql` for `aroh-tick`, `20261125000000_step_mode_wake.sql` for `aroh-push`), and run that block alone.

The jobs fail every minute until section G stores the Vault secrets. That is expected and harmless.

Later, if you install the Supabase CLI, these files were applied by hand. Mark them as applied with `supabase migration repair --status applied <version>` before you run `supabase db push`.

## D. Supabase: Auth and Realtime

16. Open **Authentication** > **Users**, click **Add user** > **Create new user**, enter your email and a strong password, tick **Auto Confirm User**, and click **Create user**. Save the password in your password manager as "Aroh account".
17. Open **Authentication** > **Sign In / Providers**, and turn off **Allow new users to sign up**. Vercel Hobby is for non-commercial use, and the engine keeps API keys for every account, so only your account should exist. Click **Save**.
18. Open **Database** > **Publications**, click `supabase_realtime`, and check that `run_events` is on. If it is off, turn it on. `20261121000000_rebuild.sql` adds it.
19. Open **Project Settings** > **JWT Keys**. Look at the current signing key:
    - **ECC (P-256)** or **RSA**: continue with step 20.
    - **Legacy HS256 (Shared Secret)**: click **Migrate JWT secret**, then **Rotate keys**, so new user tokens are signed with an asymmetric key. The app signs in again by itself when its token expires (at most one hour).
20. Save `https://<ref>.supabase.co/auth/v1/.well-known/jwks.json` as `AROH_JWKS_URL`. Open it in a browser and check that it returns a `keys` list with at least one key.

## E. Apple: APNs key and capabilities

21. Open https://developer.apple.com/account and click **Membership details**. Save the **Team ID** (10 characters) as `AROH_APNS_TEAM_ID`.
22. Open **Certificates, Identifiers & Profiles** > **Identifiers** and click the App ID `io.aroh.app`. If it does not exist, click **+**, choose **App IDs** > **App**, and set the Bundle ID to explicit `io.aroh.app`.
23. Under **Capabilities**, tick **Push Notifications** and **HealthKit**, then click **Save** (or **Continue** > **Register** for a new App ID).
24. Open **Keys**, click **+**, name the key "Aroh APNs", tick **Apple Push Notifications service (APNs)**, click **Configure**, choose environment **Sandbox & Production**, then click **Save** > **Continue** > **Register**.
25. Click **Download**. Apple lets you download the `.p8` file only once. Move it into your password manager, not into the repo.
26. Save the **Key ID** (10 characters) shown on the key page as `AROH_APNS_KEY_ID`.

The topic is the bundle id, `io.aroh.app`. Debug builds register as `sandbox` and release builds as `production`. Every device reports its own environment, so one engine serves both.

### Sign in with Apple

The app signs in natively (Authentication Services) and hands Apple's identity token to Supabase, so no Services ID, OAuth key or 6-monthly secret rotation is needed. A Services ID and key are only for a web sign-in, which Aroh does not have.

- On the App ID `io.aroh.app` (step 22), under **Capabilities**, tick **Sign In with Apple** (leave the server-to-server notification endpoint empty) and click **Save**. Xcode signing then picks up the `com.apple.developer.applesignin` entitlement the app already declares.
- In Supabase, open **Authentication** > **Sign In / Providers** > **Apple**, turn it on, and add `io.aroh.app` under **Client IDs**. Leave the OAuth secret fields empty. Click **Save**.
- Sign in with Apple creates an account on first use, so it follows the sign-up setting of step 17.
- Check on a real iPhone: onboarding, **Continue with Apple**. The simulator needs an Apple ID signed in under its Settings.

## F. Vercel: project and environment

27. Push the code to a GitHub repository you own (a fork of Aroh works). Pick the branch Vercel deploys from; this runbook calls it `prod`.
28. Open https://vercel.com/new, click **Import** next to your repository, and grant the Vercel GitHub app access to that repo if asked. After import, open **Settings** > **Environments** > **Production** and set the tracked branch to `prod`, then **Deployments** > **Create Deployment** with ref `prod`.
29. On the configure screen, click **Edit** next to **Root Directory**, choose `crates/aroh-vercel`, and click **Continue**. Leave **Include files outside the root directory in the Build Step** on (the default): the build needs the whole workspace.
30. Set **Framework Preset** to **Other**. Leave the Build and Output settings empty.
31. Generate the master key in Terminal with `openssl rand -base64 32`. Save it in your password manager as `AROH_MASTER_KEY`. If you lose or change it, every stored provider key becomes unreadable, and you must enter the keys again in the app.
32. Generate the engine JWT secret in Terminal with `openssl rand -base64 48`. Save it in your password manager as `AROH_JWT_SECRET`. Only the engine and the mint step in section G use it.
33. In Supabase, click **Connect**, choose **Session pooler**, and copy the URI (`postgresql://postgres.<ref>:[YOUR-PASSWORD]@<pooler-host>:5432/postgres`). Replace `[YOUR-PASSWORD]` with the database password, and percent-encode any `@`, `:`, `/`, `#`, `?` or `%` in it (`@` becomes `%40`). Then add `?sslmode=require` at the end. If you do not know the password, reset it under **Database** > **Settings** > **Reset database password**. Save the URI as `DATABASE_URL`. Use the session pooler, not the transaction pooler: the engine uses prepared statements, and the direct connection is IPv6 only.
34. On the Vercel configure screen, open **Environment Variables** and add each row below, for **Production**, with **Sensitive** on for every secret:

    | Name | Value | Comes from |
    |---|---|---|
    | `DATABASE_URL` | session pooler URI | step 33 |
    | `AROH_MASTER_KEY` | base64, 32 bytes | step 31 |
    | `AROH_JWT_SECRET` | base64, 48 bytes | step 32 |
    | `AROH_JWKS_URL` | `https://<ref>.supabase.co/auth/v1/.well-known/jwks.json` | step 20 |
    | `AROH_APNS_KEY` | full text of the `.p8` file, `-----BEGIN PRIVATE KEY-----` to `-----END PRIVATE KEY-----` | step 25 |
    | `AROH_APNS_KEY_ID` | 10 characters | step 26 |
    | `AROH_APNS_TEAM_ID` | 10 characters | step 21 |
    | `AROH_APNS_TOPIC` | `io.aroh.app` | bundle id |

    Do not set `AROH_ALLOW_CUSTOM_BASE_URL` (it lets the engine call any host with a user's key), `AROH_SEARCH_TOML` (a file path, and there is no file on Vercel), `AROH_BIND` or `PORT`. `AROH_JWT_AUDIENCE` defaults to `authenticated`, which is right for Supabase.
35. Click **Deploy**. The first build compiles the whole engine. A local release build took 10 minutes, and Vercel may take longer. It is not yet known whether Vercel caches the build between deploys.
36. When the build is green, open **Settings** > **Domains** and save the production domain (`https://<project>.vercel.app`) as `<engine>`. Use this domain everywhere, not a deployment URL: Standard Protection, the default, blocks deployment URLs and leaves production domains open.
37. Open **Settings** > **Functions**, set **Function Region** to the AWS region of your Supabase project (shown in Supabase under **Project Settings** > **General**, for example `us-east-1` = Washington D.C. `iad1`), and click **Save**.
38. Open **Deployments**, click the newest one, and click **Redeploy** so the region takes effect.
39. Check in a browser that `<engine>/healthz` returns `{"ok":true}`.

## G. Supabase: Vault secrets for pg_cron and the jobs trigger

40. Mint the service token in Terminal, from the repo:

    ```sh
    read -s AROH_JWT_SECRET && export AROH_JWT_SECRET
    python3 e2e/prod.py mint
    ```

    At the `read` prompt, paste the step 32 secret and press Enter. Nothing echoes. The script prints one token, valid for 365 days. Save it in your password manager as `AROH_SERVICE_JWT`, with its expiry date.
41. In the SQL Editor, run this with your values in place of the two placeholders. The SQL Editor keeps query text as a snippet, so delete this query afterwards from the snippet list on the left (**...** > **Delete query**).

    ```sql
    select vault.create_secret('<engine>', 'aroh_engine_url');
    select vault.create_secret('<the token from step 40>', 'aroh_service_jwt');
    ```

42. Wait two minutes. Then run this. Each row should show `status_code` 200:

    ```sql
    select id, status_code, left(content::text, 120) as body, error_msg, created
    from net._http_response order by created desc limit 6;
    ```

    A 401 means the token or `AROH_JWT_SECRET` do not match. A 404 or a timeout means `aroh_engine_url` is wrong. Fix it with `select vault.update_secret((select id from vault.secrets where name = 'aroh_engine_url'), '<engine>');`. Cron run history is in `select * from cron.job_run_details order by start_time desc limit 5;`.

To renew the token before it expires, repeat step 40, then run `select vault.update_secret((select id from vault.secrets where name = 'aroh_service_jwt'), '<new token>');`.

## H. App settings

43. In Supabase, open **Project Settings** > **API Keys** and copy the publishable key (`sb_publishable_...`), or the legacy `anon` key. Both are public keys, safe in the app.
44. In the app, open **Settings** > **Account**. Set **Project URL** to `https://<ref>.supabase.co` (setting `supabase_url`) and **Anon key** to the step 43 key (setting `supabase_anon_key`).
45. Sign in with the step 16 account.
46. In **Settings** > **Server and budget**, set **Server URL** to `<engine>` (setting `server_url`).
47. In **Settings** > **Model provider**, enter your provider key. The engine stores it encrypted under `AROH_MASTER_KEY`. The app keeps no copy.

## I. Smoke check

48. Run the smoke check from the repo. Type the secrets at the `read` prompts so they stay out of your shell history:

    ```sh
    export AROH_SERVER_URL='<engine>'
    export SUPABASE_URL='https://<ref>.supabase.co'
    export SUPABASE_ANON_KEY='<step 43 key>'
    export AROH_SMOKE_EMAIL='<step 16 email>'
    read -s AROH_SMOKE_PASSWORD && export AROH_SMOKE_PASSWORD
    read -s AROH_SERVICE_JWT && export AROH_SERVICE_JWT
    python3 e2e/prod.py smoke
    ```

    Expected output, with exit code 0:

    ```text
    ok   GET /healthz: 200 (want 200)
    ok   GET /v1/keys without a token: 401 (want 401)
    ok   GET /v1/keys as the owner: 200 (want 200)
    ok   POST /v1/tick as the owner: 403 (want 403)
    ok   POST /v1/tick as the service: 200 (want 200)
    ok   POST /v1/push/tick as the service: 200 (want 200)
    ```

49. The same check by hand with curl, without the script:

    ```sh
    curl -s "$AROH_SERVER_URL/healthz"
    curl -s -o /dev/null -w '%{http_code}\n' "$AROH_SERVER_URL/v1/keys"
    curl -s -o /dev/null -w '%{http_code}\n' -X POST -H "Authorization: Bearer $AROH_SERVICE_JWT" -H 'Content-Type: application/json' -d '{}' "$AROH_SERVER_URL/v1/tick"
    ```

    Expected: `{"ok":true}`, then `401`, then `200`.
50. In the app, start a goal. The first question should arrive within seconds, through the jobs trigger, and at the latest after one minute through `aroh-tick`.

## J. TestFlight: Aroh on your iPhone

Internal testing only (Q57): builds go to your team, with no App Review.

51. In Xcode, open **Settings** > **Accounts** and check that the Apple Account of your team is listed. If not, click **+** > **Apple Account** and sign in.
52. Open https://appstoreconnect.apple.com, click **Apps**, then **+** > **New App**. Set **Platforms** to iOS, **Name** to ArohPlan (done 2026-11-22; App Store names are unique; the home-screen name stays "Aroh", from `CFBundleName`, since the App Store name and the name under the icon may differ), **Primary Language** to English, **Bundle ID** to `io.aroh.app`, **SKU** to `aroh-ios`, **User Access** to Full Access. Click **Create**.
53. Copy `apps/aroh/.env.release.example` to `apps/aroh/.env.release` (untracked) and fill in `AROH_TEAM_ID` (your Apple team ID) and the server this build connects to by default: `AROH_DEFAULT_SUPABASE_URL` and `AROH_DEFAULT_SUPABASE_ANON_KEY` (your project's URL and publishable key, step 43) and `AROH_DEFAULT_SERVER_URL` (your engine URL). A build without these starts with no server and asks for one on first launch.
54. In the new app, open the **TestFlight** tab. Click **+** next to **Internal Testing**, name the group `Owner`, tick **Enable automatic distribution**, click **Create**. Click **Invite Testers**, tick yourself, click **Add**.
55. The leader runs the build and upload from the repo root, with your real HOME so Xcode sees your account and signing keychain:

    ```sh
    sh apps/aroh/ios-testflight.sh
    ```

    It builds the frontend and the Rust library, archives with your team and automatic signing, stamps build number UTC `yyyymmddHHMM`, and uploads. Expected last line: `uploaded build <number> of io.aroh.app to App Store Connect`. If macOS asks whether `codesign` may use a key, click **Always Allow**. Without a signed-in account, set an App Store Connect API key instead (never put the `.p8` in the repo): `AROH_ASC_KEY_PATH`, `AROH_ASC_KEY_ID` and `AROH_ASC_ISSUER_ID`, all three.
56. Wait for the email that the build finished processing (usually minutes). The build then shows under **TestFlight** > **iOS Builds**. Export compliance is already answered in the app (`ITSAppUsesNonExemptEncryption` false), so no questionnaire appears.
57. On the iPhone, install **TestFlight** from the App Store and sign in with the same Apple Account. Open the invitation email and tap **View in TestFlight**, or open TestFlight directly, then tap **Accept** and **Install**. Open Aroh from the home screen.
58. For a new build, repeat step 55. With automatic distribution on, TestFlight offers the update on the phone.

## K. Public accounts: limits, sign-ups, deletion (Q75, Q76)

Not done yet. Do this when the release that carries `20261202000000_public_accounts.sql` goes out, in this order. Try steps 58 to 64 on a scratch Supabase project first: steps 62 and 65 rely on Supabase behaviour the repo tests only against stubs.

58. Apply `supabase/migrations/20261202000000_public_accounts.sql`, then `supabase/migrations/20261202010000_public_accounts_storage.sql`, then `supabase/migrations/20261202020000_ai_consent_backfill.sql`, in the SQL Editor as in section C, after every earlier migration. Do this before the engine with this change reaches production: the engine reads `account_deletions` on every signed-in request, so on the old schema every route answers 500.
59. Agents now refuse to run until the account allows sending data to the AI provider. The backfill migration records that consent once for every account that already has a goal of its own, so existing users, the owner included, keep working. New accounts allow it in the app.

60. In Vercel, add `AROH_SUPABASE_URL` (`https://<ref>.supabase.co`) and `AROH_SUPABASE_SECRET_KEY` (Supabase **Project Settings** > **API Keys** > **Secret keys** > **Add new secret key**, named `aroh-engine`), **Sensitive** on, then redeploy. The engine uses the key only to remove the files of deleted accounts. Without it, an account with files is never purged and every tick reports it.
61. Open **Authentication** > **Sign In / Providers** > **Email** and turn on **Confirm email**. The built-in email sender sends 2 emails an hour for the whole project, so set up custom SMTP under **Authentication** > **Emails** > **SMTP Settings** before opening sign-ups.
62. Open **Authentication** > **Hooks**, add a **Before User Created** hook, type **Postgres**, schema `public`, function `aroh_before_user_created`, and enable it. It refuses sign-ups while they are paused or the project nears the free tier, and allows 5 sign-ups per network address an hour.
63. Open **Authentication** > **Rate Limits** and check **Sign-ups and sign-ins** per IP address. Lower it if sign-up abuse shows up.
64. Open **Authentication** > **Sign In / Providers** and turn on **Allow new users to sign up** (it was turned off in step 17).
65. Check on the scratch project: a new sign-up gets a confirmation email; `DELETE /v1/account` answers 202; signing in again within 15 days cancels it (the `auth.users.last_sign_in_at` trigger); a tick after `purge_after` removes the user from **Authentication** > **Users** and their files from **Storage**.

Read capacity and pause sign-ups in the SQL Editor:

```sql
select * from public.aroh_capacity();
update public.signup_settings set paused = true, updated_at = now();
update public.signup_settings set paused = false, max_users = 40000, max_db_bytes = 419430400, max_storage_bytes = 838860800, signups_per_ip_hour = 5, updated_at = now();
```

The thresholds start at 80 % of the Supabase free tier (50,000 monthly active users, 500 MB database, 1 GB file storage). Sign-ups close by themselves when any one is reached.

## If something fails

| Symptom | Check |
|---|---|
| Build fails in `cargo` | **Deployments** > the build > **Build Logs**. Paste the first error into a ticket. |
| `/healthz` is 500, or the function crashes at start | **Logs** in Vercel. `... is not set` names a missing variable. `master key must be 32 bytes` means step 31 output was cut. |
| App shows 401 from the engine | `AROH_JWKS_URL` is wrong, or the project still signs with the legacy secret (step 19). |
| Ticks are 401 in `net._http_response` | The Vault token was minted with a different `AROH_JWT_SECRET` than the one in Vercel. |
| No push | `AROH_APNS_*` missing (Vercel logs then show no `push on` line at start), or the device never registered: open the app once after sign-in. |
| Database errors about prepared statements | `DATABASE_URL` uses port 6543 (transaction pooler). Use the session pooler on 5432 (step 33). |
| Step 55 fails with no account or no signing certificate for the team | Steps 51 and 53, and run it from your own terminal, outside any agent sandbox. |
| Step 55 archive fails with "no devices registered" or no development profile | Register the iPhone on the team once: connect it, open Xcode > **Window** > **Devices and Simulators**, or add its UDID at https://developer.apple.com/account/resources/devices. Run step 55 again. |
| Step 55 upload fails with no app record for `io.aroh.app` | Step 52 is missing or used another bundle id. |
| Step 55 upload rejects the build number as used | Run again a minute later, or set `AROH_BUILD_NUMBER` to a higher number. |
| No push on the TestFlight build | TestFlight builds register as `production` (section E). Check that the engine has the APNs key for Sandbox & Production (step 24). |
