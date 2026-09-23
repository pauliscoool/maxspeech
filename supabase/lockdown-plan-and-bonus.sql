-- Lock down self-service plan/quota edits.
--
-- Today, profiles/user_settings RLS (see owner-admin.sql) lets a signed-in
-- user update their OWN row for any column, including `plan_tier` and the
-- `usage_bonus` / `usage_bonus_week` / `admin_words_used` /
-- `admin_words_used_week` keys inside `user_settings.settings`. That means
-- any user can grant themselves a paid plan or extra quota by writing
-- directly to Supabase (bypassing the app's own client-side checks, which
-- only gate the *app's* UI — not the database).
--
-- This adds triggers that silently revert unauthorized changes to those
-- specific fields back to their previous value, while leaving every other
-- self-service edit (username, avatar, theme, hotkey, …) untouched. Only
-- the owner account (pauldimov5@gmail.com) or the service_role key (used by
-- a future billing webhook) may change them.
--
-- Apply in the MaxSpeech Supabase SQL editor. Safe to re-run.

create or replace function public.protect_plan_tier()
returns trigger as $$
begin
  if new.plan_tier is distinct from old.plan_tier then
    if not (
      auth.role() = 'service_role'
      or lower(coalesce(auth.jwt() ->> 'email', '')) = 'pauldimov5@gmail.com'
    ) then
      new.plan_tier := old.plan_tier;
    end if;
  end if;
  return new;
end;
$$ language plpgsql security definer set search_path = public;

drop trigger if exists protect_plan_tier_trigger on public.profiles;
create trigger protect_plan_tier_trigger
  before update on public.profiles
  for each row
  execute function public.protect_plan_tier();

create or replace function public.protect_usage_admin_keys()
returns trigger as $$
declare
  privileged boolean;
  protected_keys text[] := array[
    'usage_bonus', 'usage_bonus_week',
    'admin_words_used', 'admin_words_used_week'
  ];
  k text;
begin
  privileged := (
    auth.role() = 'service_role'
    or lower(coalesce(auth.jwt() ->> 'email', '')) = 'pauldimov5@gmail.com'
  );
  if not privileged then
    if tg_op = 'INSERT' then
      foreach k in array protected_keys loop
        new.settings := new.settings - k;
      end loop;
    else
      foreach k in array protected_keys loop
        if (new.settings -> k) is distinct from (old.settings -> k) then
          if (old.settings ? k) then
            new.settings := jsonb_set(new.settings, array[k], old.settings -> k);
          else
            new.settings := new.settings - k;
          end if;
        end if;
      end loop;
    end if;
  end if;
  return new;
end;
$$ language plpgsql security definer set search_path = public;

drop trigger if exists protect_usage_admin_keys_insert_trigger on public.user_settings;
create trigger protect_usage_admin_keys_insert_trigger
  before insert on public.user_settings
  for each row
  execute function public.protect_usage_admin_keys();

drop trigger if exists protect_usage_admin_keys_update_trigger on public.user_settings;
create trigger protect_usage_admin_keys_update_trigger
  before update on public.user_settings
  for each row
  execute function public.protect_usage_admin_keys();
