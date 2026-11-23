alter table public.goal_files
  add column extracted_text text check (char_length(extracted_text) between 1 and 50000);
