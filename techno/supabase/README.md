# Supabase setup (accounts and feedback)

The site talks to Supabase only with the public **anon** key; Row Level Security in the
database decides what it may do. The database password and the `service_role` key never go
into the site or the repository.

## One-time setup in the Supabase dashboard

1. **SQL Editor → New query**: paste `schema.sql`, click **Run** (safe to run again).
2. **Authentication → Sign In / Providers**: enable **Allow anonymous sign-ins**.
   Keep the **Email** provider enabled (magic links, no passwords needed).
3. **Authentication → URL Configuration**: Site URL `https://kellermannak.github.io`,
   add `https://kellermannak.github.io/**` to the redirect URLs.
4. **Project Settings → API**: copy the **Project URL** and the **anon / public** key and put
   the key into `site/app/config.js` (`supabaseAnonKey`). That key is meant to be public.

Note: Supabase's built-in e-mail service is only for testing (very low hourly limit, and it
may only deliver to members of the project's team). For real users configure your own SMTP
under **Authentication → Emails → SMTP Settings**.

## Usage statistics (second SQL file)

Run `stats.sql` the same way as `schema.sql` (SQL Editor -> paste -> Run). It adds page views,
time on the page and listening time per song:

- `visits`: one row per page view (random visit id, anonymous or registered user, seconds on the
  page and seconds listening in 10 s steps, device class, browser family, engine version, build,
  browser speed test result, economy mode).
- `listens`: one row per song heard in a visit (live set or mix seed, song seed, title, artist,
  seconds heard in 10 s steps).

The tables have no policies at all: the public key cannot read or write them. The site writes
only through the function `track()`, which validates and rounds everything and only touches the
caller's own rows. Values are cumulative per visit, so retries never double count. No IP
address is stored in these tables. Visitors can switch the statistics off in the account panel (circle at the top right);
the site also stays off if the browser sends Do Not Track / Global Privacy Control.

Summary for analysis: `scripts/fetch_stats.py` (needs the service_role / secret key).

## What is stored

- `profiles`: one row per user (anonymous or registered), optional display name.
- `feedback`: rating (up / down / idea), comment, position on the site, context (seeds, bar,
  section, key), engine version, profile and build. Users can insert feedback and read only
  their own; at most 60 entries per user and hour.

## Reading the feedback

In the dashboard (Table Editor → feedback), or for analysis:

```sh
SUPABASE_SERVICE_KEY=... scripts/fetch_feedback.py | target/release/techno analyze --input -
```
