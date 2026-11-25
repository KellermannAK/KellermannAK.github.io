// Supabase connection: anonymous session for every visitor, optional e-mail account,
// feedback storage. Disabled (and the site works as before) while no anon key is configured.
const cfg = window.TECHNO_CONFIG || {};
const SDK = "https://cdn.jsdelivr.net/npm/@supabase/supabase-js@2.119.0/+esm";
let client = null;
let ready = null;
let token = null; // current access token, kept for requests that cannot await (page close)
const listeners = [];
/** Call `f` whenever the sign-in state changes (sign-in, sign-out, confirmed e-mail ...). */
export const onAuth = (f) => listeners.push(f);

export const enabled = !!(cfg.supabaseUrl && cfg.supabaseAnonKey);

/** The Supabase client with a session (anonymous if the visitor is not registered). */
export function db() {
  if (!enabled) return Promise.resolve(null);
  if (!ready) {
    ready = (async () => {
      const { createClient } = await import(SDK);
      client = createClient(cfg.supabaseUrl, cfg.supabaseAnonKey, {
        auth: { persistSession: true, autoRefreshToken: true, detectSessionInUrl: true },
      });
      client.auth.onAuthStateChange((_event, session) => {
        token = session ? session.access_token : null;
        listeners.forEach((f) => f(session));
      });
      const { data } = await client.auth.getSession();
      token = data.session ? data.session.access_token : null;
      if (!data.session) {
        const { error } = await client.auth.signInAnonymously();
        if (error) console.warn("anonymous sign-in failed:", error.message);
      }
      return client;
    })().catch((e) => {
      console.warn("database unavailable:", e);
      ready = null;
      return null;
    });
  }
  return ready;
}

export const accessToken = () => token;

/** Store one feedback entry. Returns true on success. */
export async function saveFeedback(e) {
  const c = await db();
  if (!c) return false;
  const { error } = await c.from("feedback").insert({
    rating: e.rating,
    comment: e.comment || null,
    where: e.where,
    context: e.ctx || {},
    engine: e.engine || null,
    profile: e.profile || null,
    build: e.build || null,
  });
  if (error) console.warn("feedback not stored:", error.message);
  return !error;
}

export async function account() {
  const c = await db();
  if (!c) return null;
  const { data } = await c.auth.getUser();
  const user = data.user;
  if (!user) return { signedIn: false, anonymous: true };
  const { data: profile } = await c.from("profiles").select("display_name").eq("id", user.id).maybeSingle();
  return {
    signedIn: true,
    anonymous: !!user.is_anonymous,
    email: user.email || user.new_email || null,
    pendingEmail: user.new_email || null,
    displayName: profile ? profile.display_name : null,
  };
}

const redirect = () => location.origin + location.pathname;

/**
 * Register with an e-mail address (no password: a confirmation link is sent).
 * An anonymous session is upgraded, so earlier feedback stays with the account.
 */
export async function register(email) {
  const c = await db();
  if (!c) return { error: "Datenbank nicht verfügbar" };
  const { data } = await c.auth.getUser();
  const res = data.user && data.user.is_anonymous
    ? await c.auth.updateUser({ email }, { emailRedirectTo: redirect() })
    : await c.auth.signInWithOtp({ email, options: { emailRedirectTo: redirect() } });
  return { error: res.error ? res.error.message : null };
}

/** Sign in on another device: sends a login link to an existing account. */
export async function signIn(email) {
  const c = await db();
  if (!c) return { error: "Datenbank nicht verfügbar" };
  const res = await c.auth.signInWithOtp({ email, options: { emailRedirectTo: redirect(), shouldCreateUser: false } });
  return { error: res.error ? res.error.message : null };
}

export async function setDisplayName(name) {
  const c = await db();
  if (!c) return { error: "Datenbank nicht verfügbar" };
  const { data } = await c.auth.getUser();
  const { error } = await c.from("profiles").upsert({ id: data.user.id, display_name: name.trim() || null });
  return { error: error ? error.message : null };
}

/** Sign out and continue anonymously. */
export async function signOut() {
  const c = await db();
  if (!c) return;
  await c.auth.signOut();
  await c.auth.signInAnonymously();
}

window.TechnoDB = { enabled, db, onAuth, accessToken, saveFeedback, account, register, signIn, setDisplayName, signOut };
