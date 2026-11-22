# Backlog

Open and closed findings and leftovers. Closed items stay, marked with the commit or verdict that closed it.

| # | Source | Where | Finding | Severity | State |
|---|---|---|---|---|---|
| B1 | PR #1 CodeRabbit | `apps/aroh/src-tauri/src/lib.rs:63` | Zone change shifts only stored actions; a server-held occurrence pulled after the shift keeps its old-zone instant, so two live occurrences of one slot can exist | high | open, pod 27 |
| B2 | PR #1 CodeRabbit | `crates/aroh-domain/src/travel.rs:117` | A series whose start falls in the destination zone's DST gap fails validation and aborts the whole device zone change | high | open, pod 27 |
| B3 | PR #1 review (P2) | `crates/aroh-app/src/travel.rs:48` | Deleted future occurrences are not carried into the new zone, so a deleted occurrence can come back | medium | open |
| B4 | PR #1 review (P2) | `crates/aroh-domain/src/travel.rs:83` | Re-keying actions on a zone change leaves open proposals pointing at the old action id | medium | open |
| B5 | PR #1 review (P2) | `crates/aroh-domain/src/travel.rs:125` | Shifted occurrence keys do not use recurrence DST gap semantics | medium | open |
| B6 | PR #1 CodeRabbit | `apps/aroh/ios-release.sh:27` | `AROH_ASC_KEY_PATH` is not resolved to an absolute path before `cd`, so a relative key path breaks upload | low | open |
| B7 | PR #1 CodeRabbit | `apps/aroh/src-tauri/src/commands/engine.rs:88` | Concurrent `send_zone` calls can deliver an older zone after a newer one | medium | open |
| B8 | PR #1 CodeRabbit | `apps/aroh/src/routes/inbox/+page.svelte:62` | Dismissed cards are not retained for the combined Inbox card limit | low | open |
| B9 | PR #1 CodeRabbit | `crates/aroh-server/src/triggers.rs:738` | `scan_reviews` can overwrite a `set_timezone` change with a value from the old zone | medium | open |
