-- Public counters for the site: monthly listeners and monthly listening hours,
-- per genre and in total. Run once in the Supabase dashboard (SQL Editor -> paste -> Run).
-- Safe to run again. Needs stats.sql and genre.sql first.
--
-- "Monthly" = the last 30 days. A listener is a distinct user id (anonymous or registered)
-- that heard at least 10 seconds of a song; hours are the summed listening seconds / 3600.
-- The function only returns these aggregates, never rows, so the tables stay private.

create or replace function public.public_stats()
returns jsonb language sql stable security definer set search_path = public as $$
  with recent as (
    select genre, user_id, seconds
    from listens
    where last_at >= now() - interval '30 days' and seconds > 0
  ),
  per_genre as (
    select genre,
           count(distinct user_id) as listeners,
           round(sum(seconds) / 3600.0, 1) as hours
    from recent group by genre
  )
  select jsonb_build_object(
    'days', 30,
    'total', jsonb_build_object(
      'listeners', (select count(distinct user_id) from recent),
      'hours', coalesce((select round(sum(seconds) / 3600.0, 1) from recent), 0)
    ),
    'genres', coalesce((
      select jsonb_object_agg(genre, jsonb_build_object('listeners', listeners, 'hours', hours))
      from per_genre
    ), '{}'::jsonb)
  );
$$;

revoke execute on function public.public_stats() from public;
grant execute on function public.public_stats() to anon, authenticated, service_role;
