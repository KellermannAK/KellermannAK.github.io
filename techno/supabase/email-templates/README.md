# E-mail templates (English, app branding)

Supabase sends the account e-mails of the app. Paste these templates in the dashboard:
**Authentication → Emails → Templates** (older dashboards: Authentication → Email Templates).

| Template in Supabase | Subject | Body (paste the HTML) |
|---|---|---|
| Change Email Address | `Confirm your email – Techno Generator` | `change-email.html` |
| Confirm signup | `Welcome to Techno Generator – please confirm` | `confirm-signup.html` |
| Magic Link | `Your sign-in link – Techno Generator` | `magic-link.html` |

"Change Email Address" is the one a visitor gets when registering, because the anonymous
session is upgraded with an e-mail address. "Magic Link" is used for signing in on another
device.

The sender (name and address) can only be changed with your own SMTP server:
**Authentication → Emails → SMTP Settings**. Without it Supabase's test mail server is used,
which sends very few e-mails per hour and shows Supabase as the sender.
