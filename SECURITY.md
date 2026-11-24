# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: open this repository's **Security** tab and click **Report a vulnerability**. Do not open a public issue, pull request or discussion for a security problem.

Include what is affected (app, engine, sync, Supabase policies, self-host stack), the steps to reproduce, and the impact you see. Do not include real user data or live keys; if a key was exposed, say so and we rotate it.

We acknowledge each report and agree on a disclosure date with you once a fix is ready. We credit reporters in the advisory unless you ask us not to.

## Supported versions

Only the latest release and the `master` branch receive security fixes.

## Scope

In scope:

- The engine (`crates/aroh-server`, `crates/aroh-vercel`): authentication, per-account isolation, the encrypted provider key store, tool calls and outbound fetches.
- Sync and storage (`crates/aroh-sync`, `supabase/migrations`): row level security, the storage bucket policies, Realtime.
- The app (`apps/aroh`, `crates/aroh-app`): local data, the keychain, the settings that point the app at a server.
- The self-host stack (`selfhost/`) as shipped.

Out of scope: findings that need a rooted or jailbroken device, denial of service by volume against a self-hosted server you run, and issues in third-party services (Supabase, Vercel, Apple, model providers) that are not caused by Aroh's configuration.
