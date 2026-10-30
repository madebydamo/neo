Your apps sit behind one shared login page, [Tinyauth](service:tinyauth).
Adding every family member there by hand gets tedious. With
[Authentik](service:authentik) people sign up themselves with their email
address and a password they choose. Nobody gets into your apps until you
approve them, by adding them to a group.

## How it fits together

- **Authentik** holds the accounts: sign up, email check, password reset.
- **Tinyauth** shows a **Sign in with Authentik** button and lets in only the
  members of the groups you allow.
- **Approving** someone means adding them to such a group in Authentik. A new
  account starts in no group, so it opens nothing.

## What you need

- Tinyauth switched on, with your apps opening at `https://<app>.{{domain}}`.
- A mail account that may send email (from your mail provider), so Authentik can
  send the confirmation email and password resets.
- About 45 minutes.

## 1. Turn on Authentik

Open [Authentik](service:authentik), press **Generate** for the
[secret key](option:authentik.secretKey) and the
[database password](option:authentik.dbPassword), and fill in
[admin email](option:authentik.bootstrapEmail). Under
[Email](option:authentik.email), enter your mail server, login and sender.
Apply the change.

Sign in at {{service.authentik.url}} as `akadmin`.

> [!check] Authentik is running
> {{service.authentik.url}} shows the Authentik login.
> ```neo-check
> systemctl is-active docker-authentik docker-authentik-worker docker-authentik-db
> ```

## 2. Allow sign ups

Authentik ships a ready-made sign-up flow that checks the email address:

1. In the Authentik admin interface, open **Customization → Blueprints** and
   apply **Example - Enrollment (with email verification)**.
2. Open **Flows and Stages → Stages**, edit
   **default-authentication-identification** and set its **Enrollment flow** to
   the new enrollment flow.

The Authentik login page now shows **Sign up**. New people enter their email,
confirm it with the link Authentik sends, and pick a password.

## 3. Create the family group

Under **Directory → Groups**, create a group, for example `family`. Everyone
you approve goes in here. Add your own account too.

## 4. Connect Tinyauth to Authentik

In Authentik, open **Applications → Applications → Create with provider**:

- **Provider:** OAuth2/OpenID Provider.
- **Redirect URI:** `{{service.tinyauth.url}}/api/oauth/callback/authentik`
- Copy the **Client ID** and **Client secret**.

Then add a provider named `authentik` under
[sign-in providers](option:tinyauth.oauth):

- **Provider type:** Generic OAuth / OIDC.
- **Button text:** for example `Family account`.
- **Client ID** and **Client secret** from Authentik.
- **Authorization URL:** `{{service.authentik.url}}/application/o/authorize/`
- **Token URL:** `{{service.authentik.url}}/application/o/token/`
- **User info URL:** `{{service.authentik.url}}/application/o/userinfo/`

Leave the scopes as they are: Authentik sends the groups with them.

Finally put `family` in [allowed groups](option:tinyauth.oauthGroups) and apply
the change.

> [!warning]
> Without an allowed group, **every** account in Authentik gets in, including
> people who just signed up. The group is your approval.

## 5. Approve new people

When someone signs up, open **Directory → Users** in Authentik, check that you
know them and add them to `family`. From their next sign-in, they can open your
apps. To take access away, remove them from the group or deactivate the
account.

> [!tip]
> To be told about new sign ups, add a **notification rule** in Authentik
> (**Events → Notification Rules**) for new users, sent to you by email.

To let a group open only some apps, use
[app access per group](option:tinyauth.groupAccess), for example a `friends`
group that only opens the photo albums.

> [!check] Approval works
> Sign up with a second email address in a private browser window and open one
> of your apps with **Family account**: you are turned away. Add the account to
> `family` in Authentik and sign in again: the app opens.
> ```neo-check
> systemctl is-active docker-tinyauth docker-authentik
> ```

## 6. Optional: skip the login page

When everybody uses Authentik, set
[skip login page](option:tinyauth.oauthAutoRedirect) to `authentik`. People then
go straight to the Authentik login. The users you added to Tinyauth with a
password keep working when the login page is shown.

> [!tip]
> Already using Microsoft 365 for the family? Tinyauth can show a
> **Sign in with Microsoft** button instead: add a provider of type
> Microsoft Entra ID under [sign-in providers](option:tinyauth.oauth).
