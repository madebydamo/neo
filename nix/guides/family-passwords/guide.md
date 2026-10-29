A password manager remembers a different strong password for every website and
fills it in for you. [Vaultwarden](service:vaultwarden) runs one on your own
server and works with the free **Bitwarden** apps and browser extensions, so
every phone and computer in the family can use it. The Wi-Fi password, the
streaming account and the insurance login can be shared with the family, while
everything else stays private to each person. See [Passwords](privacy:passwords).

## What you need

- The **Bitwarden** app on each phone, and the Bitwarden extension in each
  browser you use.
- About 30 minutes.

## 1. Turn on Vaultwarden

Open [Vaultwarden](service:vaultwarden) and switch it on. Press **Generate** for
the [admin page password](option:vaultwarden.adminToken) and keep it somewhere
safe. Apply the change.

> [!check] Vaultwarden is running
> {{service.vaultwarden.url}} opens the Bitwarden web vault after your
> homeserver login.
> ```neo-check
> systemctl is-active docker-vaultwarden
> ```

## 2. Create your account

At {{service.vaultwarden.url}}, create an account with your email address and a
**master password**. The master password unlocks all your other passwords.
Choose a long one that you can remember, for example four random words.

> [!warning]
> Nobody can reset a forgotten master password, not even the admin page.
> Without it the passwords in that vault are lost. Write it down and keep the
> paper somewhere safe at home.

Each family member creates their own account the same way, with their own
master password.

## 3. Connect the apps

The Bitwarden apps connect to Bitwarden's own servers unless you tell them
otherwise. On the login screen of the app or browser extension, choose the
**self-hosted** server option and enter `{{service.vaultwarden.url}}`. Then sign
in with your email and master password.

> [!tip]
> The apps do not need your homeserver login. They talk to Vaultwarden
> directly, so passwords also sync when you are away from home.

> [!check] Passwords sync
> Save a new login in the browser extension. A moment later it appears in the
> app on your phone.

## 4. Share passwords with the family

In the web vault at {{service.vaultwarden.url}}, create an **organization**,
for example "Family". Invite each family member with the email address of their
Vaultwarden account, then confirm them once they have joined.

Inside the organization, create **collections** such as "Home" or "Streaming".
Move a password into a collection, and everyone with access to that collection
sees it. Passwords outside the organization stay private.

## 5. Move your old passwords

Most browsers and password managers can export your saved passwords to a file.
In the web vault, import that file into your vault. Delete the export file
afterwards: it contains every password in plain text.

## The admin page

At `{{service.vaultwarden.url}}/admin` you see all accounts and can, for
example, delete an account or invite users. It asks for the admin page
password from step 1. If you leave that password empty, the admin page is
switched off.

> [!warning]
> This server now holds the keys to your whole digital life. Keep a backup of
> the server's data in another place, and do not clear Vaultwarden's data: that
> deletes every vault.
