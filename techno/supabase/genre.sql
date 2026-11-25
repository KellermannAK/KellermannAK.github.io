-- Genres (techno, house): store the genre with every feedback and every listened song.
-- Run once in the Supabase dashboard: SQL Editor -> New query -> paste -> Run.
-- Safe to run again (idempotent). Needs schema.sql and stats.sql to have been run first.
--
-- The site already sends the genre (feedback: context.genre, listens: "genre" per song);
-- before this script runs it is simply kept in the feedback context / ignored for listens.

-- ---------------------------------------------------------------- feedback
-- Derived from the context, so the site needs no change and old rows read as techno.
alter table public.feedback
  add column if not exists genre text generated always as (coalesce(context->>'genre', 'techno')) stored;
create index if not exists feedback_genre on public.feedback (genre, created_at desc);

-- ---------------------------------------------------------------- listens
alter table public.listens
  add column if not exists genre text not null default 'techno';
do $$
begin
  if not exists (select 1 from pg_constraint where conname = 'listens_genre_check') then
    alter table public.listens add constraint listens_genre_check check (genre in ('techno', 'house'));
  end if;
end $$;

-- The same set seed means different songs in different genres: the genre is part of the key.
do $$
declare
  pk text;
  cols text;
begin
  select c.conname, pg_get_constraintdef(c.oid) into pk, cols
  from pg_constraint c where c.conrelid = 'public.listens'::regclass and c.contype = 'p';
  if pk is not null and position('genre' in cols) = 0 then
    execute format('alter table public.listens drop constraint %I', pk);
    alter table public.listens add primary key (visit_id, genre, source, source_seed, song_seed);
  end if;
end $$;
create index if not exists listens_genre on public.listens (genre);

-- ---------------------------------------------------------------- track()
-- Same as in stats.sql, plus the genre per listened song.
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
  g text;
begin
  if uid is null or p_visit is null then
    return;
  end if;
  p_listens := coalesce(p_listens, '[]'::jsonb);
  if jsonb_typeof(p_listens) <> 'array' or jsonb_array_length(p_listens) > 60 then
    raise exception 'invalid listens';
  end if;

  if exists (select 1 from visits where id = p_visit and user_id <> uid) then
    return;
  end if;

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
    g := coalesce(l->>'genre', 'techno');
    if g not in ('techno', 'house') then
      g := 'techno';
    end if;
    insert into listens (visit_id, user_id, genre, source, source_seed, song_seed, song_index, title, artist, seconds)
    values (
      p_visit, uid, g, l->>'source', (l->>'source_seed')::bigint, (l->>'song_seed')::bigint,
      coalesce((l->>'song_index')::int, 0), left(l->>'title', 120), left(l->>'artist', 120), s
    )
    on conflict (visit_id, genre, source, source_seed, song_seed) do update set
      seconds = greatest(listens.seconds, excluded.seconds),
      last_at = now();
  end loop;

  update visits
  set listen_seconds = least((select coalesce(sum(seconds), 0) from listens where visit_id = p_visit), 86400)
  where id = p_visit and user_id = uid;
end;
$$;

revoke execute on function public.track(uuid, int, jsonb, text, text, text, text, real, boolean) from public, anon;
grant execute on function public.track(uuid, int, jsonb, text, text, text, text, real, boolean) to authenticated, service_role;
