// Public configuration of the site. Only PUBLIC values belong here: this file is served to
// every visitor. The Supabase "anon" key is designed to be public; access is controlled by
// Row Level Security in the database (see techno/supabase/schema.sql).
// Never put the database password or the service_role key here.
window.TECHNO_CONFIG = {
  supabaseUrl: "https://djycjopxdbkofrgrbwhx.supabase.co",
  // Public "publishable" key (new Supabase key format); enables accounts and feedback storage.
  supabaseAnonKey: "sb_publishable_r41irpktX40Nj6NN04offg_UbN7vllg",
};
