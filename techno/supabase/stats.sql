-- Techno Generator: usage statistics (page views, time on page, listening time per song).
-- Run once in the Supabase dashboard: SQL Editor -> New query -> paste -> Run.
-- Safe to run again (idempotent). Needs schema.sql to have been run first.
--
-- What is stored (no IP address, no e-mail, nothing from the page content):
--   visits  one row per page view: random visit id, user id (anonymous or registered),
--           start, last activity, seconds on the page, seconds listening (both 10 s steps),
--           device class, browser family, engine version, build, browser speed test result.
--   listens one row per song heard in a visit: live set / mix seed, song seed, title, artist,
--           seconds heard (10 s steps).
--
-- Security model: the tables have RLS enabled and NO policies, so the public key cannot
-- read or write them directly. The site writes only through track() below, which validates
-- and rounds everything and only ever touches the caller's own rows. Reading everything needs
-- the secret service_role key (dashboard, scripts/fetch_stats.py).

-- ---------------------------------------------------------------- tables
create table if not exists public.visits (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  started_at timestamptz not null default now(),
  last_seen_at timestamptz not null default now(),
  page_seconds int not null default 0 check (page_seconds between 0 and 86400),
  listen_seconds int not null default 0 check (listen_seconds between 0 and 86400),
  device text check (device in ('mobile', 'tablet', 'desktop')),
  browser text check (char_length(browser) <= 40),
  engine text check (char_length(engine) <= 20),
  build text check (char_length(build) <= 40),
  wasm_speed real check (wasm_speed is null or wasm_speed between 0 and 100000),
  eco boolean
);

create index if not exists visits_user on public.visits (user_id);
create index if not exists visits_started on public.visits (started_at desc);

create table if not exists public.listens (
  visit_id uuid not null references public.visits (id) on delete cascade,
  user_id uuid not null references auth.users (id) on delete cascade,
  source text not null check (source in ('live', 'mix')),
  -- Seed of the live set or of the mix the song was heard in.
  source_seed bigint not null,
  song_seed bigint not null,
  song_index int not null default 0 check (song_index between 0 and 1000),
  title text check (char_length(title) <= 120),
  artist text check (char_length(artist) <= 120),
  seconds int not null default 0 check (seconds between 0 and 86400),
  first_at timestamptz not null default now(),
  last_at timestamptz not null default now(),
  primary key (visit_id, source, source_seed, song_seed)
);

create index if not exists listens_user on public.listens (user_id);
create index if not exists listens_song on public.listens (song_seed);

alter table public.visits enable row level security;
alter table public.listens enable row level security;

-- ---------------------------------------------------------------- track()
-- Called by the site every ~30 s and when the tab is hidden or closed. Values are CUMULATIVE
-- per visit (seconds so far), so repeated or retried calls never double count.
create or replace function public.track(
  p_visit uuid,
  p_page_seconds int,
  p_listens jsonb default '[]'::jsonb,
  p_device text default null,
  p_browser text default null,
  p_engine text default null,
  p_build text default null,
  p_wasm_speed real default null,
  p_eco boolean default null
) returns void language plpgsql security definer set search_path = public as $$
declare
  uid uuid := auth.uid();
  page_s int := (least(greatest(coalesce(p_page_seconds, 0), 0), 86400) / 10) * 10;
  l jsonb;
  s int;
begin
  if uid is null or p_visit is null then
    return;
  end if;
  p_listens := coalesce(p_listens, '[]'::jsonb);
  if jsonb_typeof(p_listens) <> 'array' or jsonb_array_length(p_listens) > 60 then
    raise exception 'invalid listens';
  end if;

  -- A visit id belongs to the user who created it.
  if exists (select 1 from visits where id = p_visit and user_id <> uid) then
    return;
  end if;

  -- Abuse guard: at most 60 new visits per user and hour.
  if not exists (select 1 from visits where id = p_visit) and (
    select count(*) from visits where user_id = uid and started_at > now() - interval '1 hour'
  ) >= 60 then
    raise exception 'rate limit: too many visits in one hour';
  end if;

  insert into visits (id, user_id, page_seconds, device, browser, engine, build, wasm_speed, eco)
  values (p_visit, uid, page_s, p_device, left(p_browser, 40), left(p_engine, 20), left(p_build, 40), p_wasm_speed, p_eco)
  on conflict (id) do update set
    last_seen_at = now(),
    page_seconds = greatest(visits.page_seconds, excluded.page_seconds),
    device = coalesce(excluded.device, visits.device),
    browser = coalesce(excluded.browser, visits.browser),
    engine = coalesce(excluded.engine, visits.engine),
    build = coalesce(excluded.build, visits.build),
    wasm_speed = coalesce(excluded.wasm_speed, visits.wasm_speed),
    eco = coalesce(excluded.eco, visits.eco);

  for l in select * from jsonb_array_elements(p_listens) loop
    s := (least(greatest(coalesce((l->>'seconds')::int, 0), 0), 86400) / 10) * 10;
    insert into listens (visit_id, user_id, source, source_seed, song_seed, song_index, title, artist, seconds)
    values (
      p_visit, uid, l->>'source', (l->>'source_seed')::bigint, (l->>'song_seed')::bigint,
      coalesce((l->>'song_index')::int, 0), left(l->>'title', 120), left(l->>'artist', 120), s
    )
    on conflict (visit_id, source, source_seed, song_seed) do update set
      seconds = greatest(listens.seconds, excluded.seconds),
      last_at = now();
  end loop;

  update visits
  set listen_seconds = least((select coalesce(sum(seconds), 0) from listens where visit_id = p_visit), 86400)
  where id = p_visit and user_id = uid;
end;
$$;

-- Totals of the current user (shown on the account card).
create or replace function public.my_stats()
returns jsonb language sql stable security definer set search_path = public as $$
  select jsonb_build_object(
    'visits', (select count(*) from visits where user_id = auth.uid()),
    'page_seconds', (select coalesce(sum(page_seconds), 0) from visits where user_id = auth.uid()),
    'listen_seconds', (select coalesce(sum(listen_seconds), 0) from visits where user_id = auth.uid()),
    'songs', (select count(distinct song_seed) from listens where user_id = auth.uid())
  );
$$;

-- ---------------------------------------------------------------- grants
-- No direct table access for the public roles; only the two functions are callable.
revoke all on public.visits, public.listens from anon, authenticated;
grant all on public.visits, public.listens to service_role;
revoke execute on function public.track(uuid, int, jsonb, text, text, text, text, real, boolean) from public, anon;
revoke execute on function public.my_stats() from public, anon;
grant execute on function public.track(uuid, int, jsonb, text, text, text, text, real, boolean) to authenticated, service_role;
grant execute on function public.my_stats() to authenticated, service_role;
