-- Techno Generator: users and listening feedback.
-- Run once in the Supabase dashboard: SQL Editor -> New query -> paste -> Run.
-- Safe to run again (idempotent).
--
-- Security model: the website only ever uses the public "anon" key. Row Level Security
-- (RLS) decides what that key may do:
--   * everyone (also anonymous sign-ins) may INSERT feedback for themselves;
--   * users may read and edit only their own profile and read only their own feedback;
--   * reading all feedback needs the secret service_role key (dashboard / analysis script).

-- ---------------------------------------------------------------- profiles
create table if not exists public.profiles (
  id uuid primary key references auth.users (id) on delete cascade,
  display_name text check (display_name is null or char_length(display_name) between 1 and 40),
  created_at timestamptz not null default now()
);

alter table public.profiles enable row level security;

drop policy if exists "profiles: read own" on public.profiles;
create policy "profiles: read own" on public.profiles
  for select using (auth.uid() = id);

drop policy if exists "profiles: insert own" on public.profiles;
create policy "profiles: insert own" on public.profiles
  for insert with check (auth.uid() = id);

drop policy if exists "profiles: update own" on public.profiles;
create policy "profiles: update own" on public.profiles
  for update using (auth.uid() = id) with check (auth.uid() = id);

-- A profile row for every new user (anonymous or registered).
create or replace function public.handle_new_user()
returns trigger language plpgsql security definer set search_path = public as $$
begin
  insert into public.profiles (id) values (new.id) on conflict (id) do nothing;
  return new;
end;
$$;

drop trigger if exists on_auth_user_created on auth.users;
create trigger on_auth_user_created
  after insert on auth.users
  for each row execute function public.handle_new_user();

-- ---------------------------------------------------------------- feedback
create table if not exists public.feedback (
  id bigint generated always as identity primary key,
  created_at timestamptz not null default now(),
  -- Set automatically from the session; null only for clients without any session.
  user_id uuid default auth.uid() references auth.users (id) on delete set null,
  rating text not null check (rating in ('up', 'down', 'idea')),
  comment text check (comment is null or char_length(comment) <= 1000),
  -- Human-readable position ("Live-Set 4242 @ 12:14 · Song 3 ...") as shown on the site.
  "where" text not null check (char_length("where") <= 400),
  -- Machine-readable context: set/mix seed, song seed, bar, section, key, ...
  context jsonb not null default '{}'::jsonb check (pg_column_size(context) <= 4000),
  engine text check (engine is null or char_length(engine) <= 20),
  profile text check (profile is null or char_length(profile) <= 60),
  build text check (build is null or char_length(build) <= 40)
);

create index if not exists feedback_created_at on public.feedback (created_at desc);
create index if not exists feedback_user on public.feedback (user_id);

alter table public.feedback enable row level security;

drop policy if exists "feedback: insert own" on public.feedback;
create policy "feedback: insert own" on public.feedback
  for insert to anon, authenticated
  with check (user_id is null or user_id = auth.uid());

drop policy if exists "feedback: read own" on public.feedback;
create policy "feedback: read own" on public.feedback
  for select to authenticated using (user_id = auth.uid());

-- Simple abuse guard: at most 60 feedback rows per user per hour.
create or replace function public.feedback_rate_limit()
returns trigger language plpgsql security definer set search_path = public as $$
begin
  if new.user_id is not null and (
    select count(*) from public.feedback
    where user_id = new.user_id and created_at > now() - interval '1 hour'
  ) >= 60 then
    raise exception 'rate limit: too much feedback in one hour';
  end if;
  return new;
end;
$$;

drop trigger if exists feedback_rate_limit on public.feedback;
create trigger feedback_rate_limit
  before insert on public.feedback
  for each row execute function public.feedback_rate_limit();

-- Number of feedback entries of the current user (for the account card).
create or replace function public.my_feedback_count()
returns bigint language sql stable security invoker as $$
  select count(*) from public.feedback where user_id = auth.uid();
$$;
