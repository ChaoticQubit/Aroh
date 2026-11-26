# Aroh privacy policy (draft)

Draft for the hosted Aroh service, 2026-11-23. The owner reviews it, fills in the contact address and the effective date, confirms the Children and Changes sections (neither is backed by anything in the app yet), adds Sign in with Apple to the table if it ships, and publishes it before the App Store release. A self-hosted Aroh server is run by whoever hosts it; this policy covers only the hosted service.

## The short version

- Aroh plans your goals on a calendar. To do that it keeps your goals, plans and logs on your phone and, when you have an account, on our server.
- When an agent plans or replans, the goal data it needs goes to the AI provider you picked (Anthropic's Claude by default), using your own API key. Agents only run after you allow this in the app.
- We do not sell your data, show ads or track you across apps.
- You can delete your account in the app. It is disabled at once and deleted for good after 15 days, unless you sign in again before then.

## What is stored, and where

| Data | Where | Why |
|---|---|---|
| Email address and password (the password only as a hash) | Our Supabase project (Auth) | To sign you in |
| Goals, plans, calendar actions, trackers and their entries, categories, notes on preferences, places you name, proposals, budgets and usage | On your phone (SQLite) and, when signed in, our Supabase database | To show and sync your plan across devices |
| Files you attach to a goal, and the text the app reads from them | Our Supabase storage and database | So agents can use them when planning |
| Diary entries and day cards, encrypted on your phone with a diary key kept in iCloud Keychain | On your phone and, encrypted, our database; our server holds a copy of the diary key sealed with a server key | To sync your diary and let the engine structure your days |
| Busy times from calendars you connect (title, start, end) | On your phone and our database | So plans avoid your busy times |
| Apple Health values that a tracker imports (body mass, workouts, steps, dietary energy), only when you connect Health | On your phone and, as tracker entries, our database | To fill your trackers |
| Places you save (name, address, coordinates, radius) | On your phone and our database | Place-based reminders and planning |
| Visits detected near your saved places | On your phone only | Place-based nudges |
| Your AI provider and search API keys | Our server, encrypted at rest with a server key; never sent back to the app, which only sees the last 4 characters | To call the provider for you |
| Agent runs: inputs, answers, steps and costs | Our database | To show runs, continue them and keep you within your budget |
| Push notification token of your device | Our database | To send plan reminders and change requests |
| Your consent to send data to the AI provider, with its time | Our database | Agents need it to run |
| Your sign-in session | Your phone's Keychain | To keep you signed in |
| The network address of a sign-up | Our database, removed at the next sign-up after a day | To limit sign-ups per network |

Our server runs on Supabase (database, sign-in, file storage) and Vercel (the planning engine). Both process the data above on our behalf and keep their own request logs, which can include your network address.

## What goes to the AI provider

Agents run on our server and call the AI provider you chose with your own API key. Claude by Anthropic is the default; you can pick another provider in Settings. Nothing is sent until you allow it in the app, and you can withdraw that permission at any time, which stops all agent runs.

Depending on the task, an agent run sends:

- the goal's title, context and deadline, your time zone and today's date;
- your calendar actions and busy times for the planning window;
- preferences and profile notes you confirmed, and the names of your places;
- the text of files attached to the goal, and links you added;
- your answers to the agent's questions;
- for a free-text log, the text you typed and your tracker definitions;
- to structure a diary day, that day's words, opened in our server's memory only and never stored or logged unencrypted, with your tracker definitions;
- to read an attached image your phone could not read well, that image;
- with "Let agents read my diary" on, the summary, mood, energy and notes of your last 7 diary days;
- for a replan triggered by a tracker rule, the tracker's name and values in that window (which can include imported Health values such as body mass).

Agents can search the web and read web pages. With Claude, searches and page reads go through Anthropic's own web tools; if you set up another search service (Brave Search or a SearXNG server), the search terms go to that service.

The provider handles that data under its own terms and privacy policy. With Anthropic, see anthropic.com/legal.

## Push notifications

When you allow notifications, reminder and plan-change messages are sent through Apple's push service. A message can contain a goal title and a short summary of a proposed change.

## Limits on the hosted service

Each account can have 5 active goals, 10 agent runs you start yourself in any 24 hours (scheduled planning does not count), files up to 10 MB each, 20 stored files and 50 MB of files. New sign-ups can be paused when the service is near its capacity.

## How long we keep data

We keep your data while your account exists. Deleting a goal, file or entry in the app removes it from your phone and marks it deleted on the server; the deleted record stays until the account is deleted. Files you delete are removed from storage when your phone next syncs.

## Deleting your account

In the app: Settings, Account, Delete account.

1. Your account is disabled at once: your devices are signed out, no agent runs, and your data is no longer served to any device. The app also erases its data on that phone.
2. If you sign in again within 15 days, the deletion is cancelled and everything is back.
3. After 15 days, we delete your stored files, every record of your account (goals, plans, trackers, runs, keys, devices, consent, usage) and your sign-in.

Data the AI provider received during earlier agent runs is handled under the provider's own retention terms; we cannot delete it for you.

## Your choices

- Connecting calendars, Apple Health, location and notifications is optional; the app works without them.
- You can withdraw permission to send data to the AI provider in Settings.
- You can use Aroh without an account. Your data then stays on your phone and no agent runs.

## Children

Aroh is not meant for children under 13.

## Changes

We will announce changes to this policy in the app before they take effect.

## Contact

Questions or requests: (owner to add contact address).
