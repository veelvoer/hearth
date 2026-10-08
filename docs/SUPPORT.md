# The support desk

People write to you from **Settings → Support** in the app. Everything runs on your own server and your own Gmail mailbox. There is no third party.

## What people see

1. They sign in with their email address. Hearth emails a 6-digit code (it works with any real address, Gmail included; no password).
2. They pick *Bug*, *Question*, *Idea* or *Other*, write a title and a message, and press **Send**.
3. They get a confirmation email with their request number and what happens next.
4. When you answer, they get your answer in a Hearth-styled email, and can read it (and reply) in the app too.

## What you see

Each request arrives in your support mailbox as a Hearth-styled email: who wrote, the type, the app version and system, and the message. **Just reply to that email.** The desk catches your reply and sends it on with the same layout. Quoted text is cut away automatically.

## Set it up (about 5 minutes)

You need the support mailbox (for example `hearth.support1@gmail.com`) and a Hearth server.

1. In that Google account turn on **2-Step Verification**, then create an **App password** (myaccount.google.com → Security → App passwords). Google shows 16 letters. Copy them.
2. On the server, run:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/support/install.sh | sudo bash
   ```

   It asks for the mailbox and the app password (typed text is hidden) and starts the service `hearth-support`. The password stays in `/etc/hearth-support.env`, readable only by root.
3. Open Hearth → Settings → Support and send yourself a test request.

Change the password or mailbox later: run the same line again.

## Good to know

- **Who may answer?** The mailbox itself, plus any extra addresses you give the installer. Replies must come with a passing SPF/DKIM check, so nobody can fake an answer.
- **Limits:** 5 codes per hour per email, 6 requests per day per person, messages up to 5000 characters.
- **Data:** requests and signed-in sessions are kept in `/var/lib/hearth-support/support.json` (readable by the service user only). To delete someone's data, remove their tickets from that file and restart the service.
- **Sign in with Google?** Not offered: it needs a Google Cloud OAuth project that only you can create. The email code works with Gmail addresses already.
- **Another address:** the apps talk to `https://<your-server>/support`. Forks can change `SUPPORT_URL` in `desktop/main.js` and `app/.../Support.kt`.
- **Test:** `node support/test/support.test.js`.
