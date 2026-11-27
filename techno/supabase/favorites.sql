-- Favourite songs per user (heart button on the site).
-- Run once in the Supabase dashboard: SQL Editor -> New query -> paste -> Run.
-- Safe to run again. Needs schema.sql first.
--
-- A favourite stores what is needed to play the song again: genre, song seed and the key
-- root and BPM it was heard in (songs inside a mix are generated for the running tempo
-- and key), plus title / artist / key for the list. Every user sees and changes only their
-- own favourites (RLS); anonymous and registered users alike.

create table if not exists public.favorites (
  user_id uuid not null default auth.uid() references auth.users (id) on delete cascade,
  genre text not null check (genre in ('techno', 'house')),
  song_seed bigint not null check (song_seed >= 0),
  key_root smallint not null check (key_root between 0 and 11),
  bpm real not null check (bpm between 60 and 200),
  title text check (char_length(title) <= 120),
  artist text check (char_length(artist) <= 120),
  key text check (char_length(key) <= 40),
  build text check (char_length(build) <= 40),
  created_at timestamptz not null default now(),
  primary key (user_id, genre, song_seed)
);

alter table public.favorites enable row level security;

drop policy if exists "favorites: read own" on public.favorites;
create policy "favorites: read own" on public.favorites for select using (auth.uid() = user_id);
drop policy if exists "favorites: insert own" on public.favorites;
create policy "favorites: insert own" on public.favorites for insert with check (auth.uid() = user_id);
drop policy if exists "favorites: delete own" on public.favorites;
create policy "favorites: delete own" on public.favorites for delete using (auth.uid() = user_id);

-- At most 300 favourites per user (abuse guard).
create or replace function public.favorites_limit() returns trigger language plpgsql as $$
begin
  if (select count(*) from public.favorites where user_id = new.user_id) >= 300 then
    raise exception 'favorites limit reached';
  end if;
  return new;
end;
$$;
drop trigger if exists favorites_limit on public.favorites;
create trigger favorites_limit before insert on public.favorites
  for each row execute function public.favorites_limit();

revoke all on public.favorites from anon;
grant select, insert, delete on public.favorites to authenticated;
grant all on public.favorites to service_role;
