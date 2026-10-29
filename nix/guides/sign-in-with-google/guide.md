Your apps sit behind one shared login page, [Tinyauth](service:tinyauth). Instead
of giving everyone a new password, you can add a **Sign in with Microsoft** or
Google button to it. People sign in with the account they already have, and you
decide which of them may open which apps.

## What you need

- Tinyauth switched on, with your apps opening at `https://<app>.{{domain}}`.
- **For Microsoft:** a Microsoft Entra ID directory (the one behind Microsoft 365
  work or school accounts) where you may register apps and grant admin consent.
- **For Google:** [Authentik](service:authentik) on this server. Google goes
  through Authentik, see step 3.

## 1. Pick a name for the provider

Each sign-in provider gets a short name of lowercase letters and digits, for
example `microsoft` or `authentik`. The name is part of the **redirect URL**, the
address the provider sends people back to after they sign in:

```
{{service.tinyauth.url}}/api/oauth/callback/<name>
```

For a provider named `microsoft` that is
`{{service.tinyauth.url}}/api/oauth/callback/microsoft`.

## 2. Sign in with Microsoft

In the Microsoft Entra admin center, register a new app for your homeserver:

- **Redirect URL** (platform type Web): the address from step 1.
- **Client secret:** create one and copy its **value** right away. The secret
  ID is not what Tinyauth needs.
- **API permissions** (Microsoft Graph, delegated): `openid`, `profile`, `email`,
  `User.Read` and `GroupMember.Read.All`. Grant admin consent.
- Note the **Application (client) ID** and the **Directory (tenant) ID** from the
  app's overview.

Then open [sign-in providers](option:tinyauth.oauth) in Tinyauth, add a provider
named `microsoft` and fill in:

- **Provider type:** Microsoft Entra ID.
- **Client ID**, **Client secret** and **Tenant ID** from the app registration.

> [!tip]
> Without `GroupMember.Read.All` and admin consent, turn off **Use group names**
> for the provider and use the groups' object IDs in step 4 instead of their
> names.

## 3. Sign in with Google

> [!warning]
> Do not add Google directly as a provider named `google`. Tinyauth then lets
> **every** Google account in, and the allowed groups from step 4 do not apply.
> Under any other name Google sends no groups, so nobody gets in.

Google signs people in through [Authentik](service:authentik) instead, which
adds the groups Tinyauth needs:

1. Switch on Authentik and sign in at {{service.authentik.url}}.
2. Add Google as a login source in Authentik, following Authentik's
   documentation for Google. Google needs a client ID and secret, which you
   create in the Google Cloud console with the redirect address Authentik's
   documentation gives.
3. In Authentik, create an OAuth2 / OpenID provider and application for
   Tinyauth, with the redirect URL from step 1 for the name `authentik`. Copy
   its client ID and client secret.
4. Put the people who may use your apps in a group in Authentik.

Then add a provider named `authentik` under
[sign-in providers](option:tinyauth.oauth):

- **Provider type:** Generic OAuth / OIDC.
- **Button text:** for example `Google`.
- **Client ID** and **Client secret** from Authentik.
- **Authorization URL:** `{{service.authentik.url}}/application/o/authorize/`
- **Token URL:** `{{service.authentik.url}}/application/o/token/`
- **User info URL:** `{{service.authentik.url}}/application/o/userinfo/`

Leave the scopes as they are: Authentik sends the groups with them.

## 4. Choose who may sign in

People who sign in with a provider only get in when they belong to one of the
[allowed groups](option:tinyauth.oauthGroups). Add the group names from Entra or
Authentik there. Everyone in none of them is turned away.

To let a group open only some apps, use
[app access per group](option:tinyauth.groupAccess). A group without a rule
opens every app.

Apply the change.

> [!tip]
> The users you added to Tinyauth with a password keep working next to the new
> button. Group changes at the provider count from the person's next sign-in.

> [!check] The new button works
> Open one of your apps in a private browser window. The login page shows the
> new button. Sign in with an account from an allowed group: the app opens.
> With an account outside the groups, it does not.
> ```neo-check
> systemctl is-active docker-tinyauth
> systemctl is-active docker-tinyauth-entra   # only with Microsoft Entra ID
> ```

## 5. Optional: skip the login page

When everybody uses the same provider, set
[skip login page](option:tinyauth.oauthAutoRedirect) to its name. People then go
straight to Microsoft or Authentik.
