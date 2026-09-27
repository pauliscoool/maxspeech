-- Owner admin: let pauldimov5@gmail.com list and update every profile / settings row.
-- Apply in the MaxSpeech Supabase SQL editor if the Usage tab only shows your own account.

alter table public.profiles enable row level security;
alter table public.user_settings enable row level security;

drop policy if exists owner_read_all_profiles on public.profiles;
create policy owner_read_all_profiles on public.profiles
  for select using (
    auth.uid() = id
    or lower(coalesce(auth.jwt() ->> 'email', '')) = 'pauldimov5@gmail.com'
  );

drop policy if exists owner_update_all_profiles on public.profiles;
create policy owner_update_all_profiles on public.profiles
  for update using (
    auth.uid() = id
    or lower(coalesce(auth.jwt() ->> 'email', '')) = 'pauldimov5@gmail.com'
  );

drop policy if exists owner_read_all_settings on public.user_settings;
create policy owner_read_all_settings on public.user_settings
  for select using (
    auth.uid() = user_id
    or lower(coalesce(auth.jwt() ->> 'email', '')) = 'pauldimov5@gmail.com'
  );

drop policy if exists owner_write_all_settings on public.user_settings;
create policy owner_write_all_settings on public.user_settings
  for all using (
    auth.uid() = user_id
    or lower(coalesce(auth.jwt() ->> 'email', '')) = 'pauldimov5@gmail.com'
  )
  with check (
    auth.uid() = user_id
    or lower(coalesce(auth.jwt() ->> 'email', '')) = 'pauldimov5@gmail.com'
  );
