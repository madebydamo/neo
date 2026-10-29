A disk can die, and a fire or a burglary takes the whole server. With
[Backup](service:backup), your homeserver copies its data every night to another
computer you trust, for example a friend's server or a small box at your
parents' place. If this server breaks, your photos, documents and settings are
safe elsewhere.

## What you need

- A friend with a Linux server (or NAS) that is switched on at night and can be
  reached from your server over SSH, the usual remote login for servers.
- Enough free space on their side for your data.
- About 30 minutes, part of it on the friend's side.

## 1. Copy your server's key

Your server logs in to the friend's server with a key instead of a password.
Open **Settings** and find **Homeserver SSH public key** under
**Access & backup**. Copy the key, the line that starts with `ssh-ed25519`, and
send it to your friend.

> [!tip]
> This is the public half of the key and safe to send by message or mail. The
> private half never leaves your server.

## 2. What your friend prepares

Send your friend this list:

1. Create a user for the backup, for example `backup`. A separate user keeps
   your files apart from theirs.
2. Add the key from step 1 to that user's `~/.ssh/authorized_keys` file.
3. Make sure `rsync` is installed. The backup uses it on both sides.
4. If the server sits behind a router, forward its SSH port, so your server can
   reach it from outside.
5. Tell you the server's **name or IP address**, the **username**, and the SSH
   **port** if it is not 22.

## 3. Set up the backup

Open [Backup](service:backup), switch it on and fill in:

- [Backup server](option:backup.host): the name or IP address from your friend.
- [Username](option:backup.user): the user your friend created.
- [Extra SSH options](option:backup.extraOptions): add
  `-o StrictHostKeyChecking=accept-new`, so your server trusts the friend's
  server the first time it connects. If the port is not 22, add a second
  entry with the port, for example `-p 2222`.

Leave the [SSH key](option:backup.sshKey) as it is: it is the key from step 1.

The backup lands in a folder on your friend's server named after your
homeserver, in the backup user's home folder. Change
[Folder on backup server](option:backup.remotePath) only if your friend asks
for another place.

## 4. Pick the time and what to skip

- [Backup time](option:backup.schedule): midnight by default. Write `03:00:00`
  for 3 AM, or `Sun 02:00` for once a week on Sunday night.
- [Skip folders](option:backup.excludedDirs): folders you do not need twice.
  `DATA/Media` leaves out films and music if you can download them again, and a
  plain name such as `cache` skips every folder with that name.

By default the whole data folder is copied: all app data, your documents and
your media. [Folder to back up](option:backup.sourceDir) changes that.

Apply the change.

> [!warning]
> The backup is a copy of your server as it is tonight. A file you delete here
> also disappears from the backup at the next run, so it does not bring back
> something you deleted last week.

## 5. Run it once and check

You do not have to wait for the night: in the Backup pane, start the **backup**
unit once. The first run copies everything and can take hours; later runs only
copy what changed.

> [!check] The backup ran
> The backup unit finishes without an error, and your friend sees a folder named
> after your homeserver in the backup user's home folder.
> ```neo-check
> systemctl list-timers backup.timer --no-pager
> systemctl show -p Result -p ExecMainStatus backup.service   # Result=success
> ```

If it fails, the details of the last run are in `/var/log/backup.log` on your
server. "Permission denied" usually means the key from step 1 was not added to
the right user.
