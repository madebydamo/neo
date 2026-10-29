Letters, budgets, school projects: with Google Docs or Microsoft 365 they all
live on someone else's computer. [Nextcloud](service:nextcloud) keeps your files
on your own server, and [Collabora](service:collabora) adds an office suite to
it. You open Word, Excel and PowerPoint files right in the browser and several
people can edit the same file at the same time. Nobody needs to install Office.

## What you need

- About 25 minutes.
- A user name and a strong password for the Nextcloud administrator. Write
  them down in your password manager.

## 1. Turn on Nextcloud

Open [Nextcloud](service:nextcloud) and switch it on. Before you apply, press
**Generate** next to the [database password](option:nextcloud.dbPassword).

> [!warning]
> Generate the database password **before** the first start and never change
> it afterwards. Nextcloud stores it at its first start, and a different
> password later cuts Nextcloud off from all its data.

Also set [Phone number country](option:nextcloud.defaultPhoneRegion) to your
country (two letters, e.g. `CH`, `DE` or `US`). Apply the change.

The first start takes a few minutes while Nextcloud prepares its database.

## 2. Create the Nextcloud administrator

Open {{service.nextcloud.url}} and sign in with your homeserver login first.
The first time, Nextcloud then asks you to create its administrator account:
enter the user name and password you picked. The database is already set up,
so it asks for nothing else.

> [!tip]
> Nextcloud has its own accounts, separate from the homeserver login. As the
> administrator you add an account for each family member in Nextcloud's
> account management (in the menu behind your profile picture). Everyone then
> has their own files and can share them with the others.

> [!check] Nextcloud is ready
> After the first login you see your Nextcloud files.
> ```neo-check
> docker exec --user www-data nextcloud php occ status
> ```

## 3. Turn on Collabora

Collabora only works together with Nextcloud, and Neo refuses to apply it while
Nextcloud is off. Open [Collabora](service:collabora), switch it on and apply
the change.

Neo connects the two by itself: it installs the office app in Nextcloud and
points it at {{service.collabora.url}}. This can take a few minutes after the
apply, because it waits until Nextcloud is fully started.

> [!warning]
> Your browser loads the editor straight from {{service.collabora.url}}. Keep
> Collabora's [Reachable from](option:collabora.ingress) the same as
> Nextcloud's [Reachable from](option:nextcloud.ingress), otherwise documents
> open everywhere Nextcloud works but the editor stays blank.

## 4. Edit a document together

1. In Nextcloud, open **Files** and create a new document, spreadsheet or
   presentation with the **+ New** button, or upload a Word or Excel file.
2. Click the file. It opens in the editor in the same browser tab.
3. Share the file with another family member, or open it on a second device.
   You see each other's cursor and changes as you type.

> [!check] The editor opens
> Clicking a document opens the editor inside Nextcloud and you can type in it.
> ```neo-check
> systemctl is-active docker-nextcloud docker-collabora collabora-setup
> ```

