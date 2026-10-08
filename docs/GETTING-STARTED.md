# Getting started

This guide is written so that anyone can follow it. Take your time. Every step says what you should see.

You will do three things:

1. Put **Hearth on a server** (a computer that is always on).
2. Put the **Hearth app on your computer**.
3. Put the **Hearth app on your phone** (optional).

## Before you start

You need:

- **Claude Code and a Claude account.** Hearth uses Claude Code. Make sure you can use it on your own computer first (see [claude.com/claude-code](https://claude.com/claude-code)).
- **A server.** A server is just a computer in a data center that never switches off. Any small Linux server is enough (1 CPU, 1 GB memory). Most hosting companies sell one for around €4 a month. Choose **Debian** or **Ubuntu** when it asks which system you want.
- **Your computer** (Windows or Linux).
- Optionally, an **Android phone**.

> No server yet? You can still try Hearth: skip the server step in the tutorial. Everything then works while your computer is on, and your phone can join over your home Wi-Fi.

## Step 1: set up the server

1. Log in to your server. Your hosting company shows you how. It is usually a line like `ssh root@123.45.67.89` typed in a terminal (on Windows: PowerShell).
2. Copy this line, paste it, and press **Enter**:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/install.sh | bash
   ```

3. Answer the questions. **If you are not sure, just press Enter.** You will see numbers, like `1` and `2`. Type a number and press Enter.
   - *Where should Hearth live?* **Directly on this server** is the easy choice. Choose Docker only if you like Docker.
   - *Which address do you want?* **Get a free address for me.** Hearth makes a safe web address with a padlock (HTTPS) for you. Ports 80 and 443 must be open: if your hosting company has a "firewall" setting, allow them.
   - *What should your server be called?* Anything you like.
4. Wait a few minutes. At one point Claude opens so you can **sign in to your Claude account**: open the link it shows, sign in, paste the code back, then type `/exit`.
5. At the end you see a box like this:

   ```
   Your server is ready!
   Address:  https://1-2-3-4.sslip.io
   Code:     123 456
   ```

   Keep this window open. You need the address and the code next.

## Step 2: the app on your computer

1. Go to the [latest release](https://github.com/veelvoer/hearth/releases/latest) and download the file for your computer:
   - **Windows:** `Hearth-…-win-x64.exe`. Double-click it.
   - **Linux:** `Hearth-…-linux-x64.AppImage`. Make it runnable (`chmod +x` or right-click → Properties → Allow executing) and double-click it. Or install the `.deb`.
2. Open Hearth. A **tutorial** starts. You can skip it any time and replay it later (Settings → Help).
3. At **First, set up your server** type the **address** and the **6-digit code** from step 1, then press **Connect**. You should see a green ✓.
4. Hearth checks that **Claude Code** is installed on this computer. If not, it tells you what to do.

You now have one **projects folder**: a folder named `projects` inside your home folder. Put your projects there. Hearth keeps it the same on your computer and your server.

## Step 3: the app on your phone (Android)

1. Download `Hearth-….apk` from the [latest release](https://github.com/veelvoer/hearth/releases/latest) on your phone and open it. Android may ask permission to install apps from your browser. Allow it.
2. Put the phone on the **same Wi-Fi as your computer** and open Hearth on both.
3. On the phone, follow the tutorial. At **Connect to your computer** you see your computer in a list. Tap **Connect**.
4. On your computer a card pops up: *"Your phone wants to connect"*. Press **Accept**. Done! The phone also gets the connection to your server, if you set one up. You can turn these cards off in Settings → Connect your phone.
5. Allow notifications when asked, so Hearth can tell you when Claude is done.

> **Not on the same Wi-Fi?** Tap *Enter the address myself* and type the address and a 6-digit code (`hearth-server code` on the server, or Settings → Connect your phone on the computer).

## Try it

- **On your phone:** tap *New session*, choose *Coding*, name a project and say what you want: "make a tiny website about cats".
- **On your computer:** if it was off, Hearth tells you: *"A new project was made on your server. Install it?"* Press **Install** to copy it to your computer, or **Later** and it asks again next time you open Hearth.
- Open the chat on your computer: it is the same chat.

## Everyday tips

| I want to… | Do this |
|---|---|
| See commands and skills | Type `/` in a message |
| Add skills, plugins or tools | Press **Add-ons** in a chat |
| See what Claude changed, save it or undo it | **Add-ons → Changes** |
| Tell Claude things it should always remember about a project | **Add-ons → Notes** |
| Be called when Claude is done | Chat → **Options** → *Phone call* |
| Check that everything is in sync | **Settings → Sync** |
| Get a new code for a new device | `hearth-server code` on the server |

Something not working? See [Troubleshooting](FAQ.md#something-is-not-working).
