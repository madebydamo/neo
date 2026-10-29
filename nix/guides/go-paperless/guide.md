Tax letters, insurance papers, payslips and warranties pile up, and the one you
need is never there. [Paperless](service:paperless) turns them into an archive
on your own server: scan or upload a document, and Paperless reads the text,
files it by sender and type, and finds it again when you search for any word in
it. Your paperwork says a lot about your health, money and family, so it stays
at home. See [Letters & documents](privacy:documents).

## What you need

- A scanner, or a scanner app on your phone that saves PDFs.
- About 30 minutes.

## 1. Turn on Paperless

Open [Paperless](service:paperless) and switch it on. Before the first start,
press **Generate** for both passwords:

- the [database password](option:paperless.dbPassword), which Paperless uses
  for its own database,
- the [secret key](option:paperless.secretKey), which Paperless needs to start
  at all.

You never type either of them in. Apply the change.

> [!warning]
> Keep both values once Paperless has started. A new database password cuts
> Paperless off from its documents, and a new secret key signs everyone out.

> [!check] Paperless is running
> {{service.paperless.url}} opens the Paperless sign-in page after your
> homeserver login.
> ```neo-check
> systemctl is-active docker-paperless docker-paperless-db docker-paperless-redis
> ```

## 2. Create your Paperless account

Paperless has its own users, separate from your homeserver login. The first one
is the administrator. If Paperless does not offer to create it on its page,
create it on the server, or let the assistant do it:

```
docker exec -it paperless createsuperuser
```

Sign in at {{service.paperless.url}}. In Paperless's settings you can then add
a user for each family member.

## 3. Add your first documents

Drag PDFs or photos of letters onto the Paperless page, or use its upload
button. On a phone, scan the letter with a scanner app and upload the PDF in
the browser.

After a minute Paperless has read the text. It understands German and English
documents.

> [!check] You find the letter again
> Upload a letter, wait a minute and search for a word that appears in it, such
> as the sender's name. The letter shows up in the results.

## 4. Let Paperless sort for you

Give your first documents a **correspondent** (who sent it, for example your
health insurance), a **document type** (invoice, contract, payslip) and
**tags**. Paperless learns from them and suggests the same for new documents.

> [!tip]
> Paperless can also collect documents from an email account, for example bills
> that arrive as attachments. Add the email account in Paperless's mail settings.

## 5. Keep the paper or not

Once a letter is in Paperless, you can usually throw the paper away. Keep
originals that must stay on paper, such as certificates, contracts you signed
and official documents.

> [!warning]
> The archive now holds your important papers. Keep a backup of the server's
> data in another place, and do not clear Paperless's data: that deletes all
> documents.
