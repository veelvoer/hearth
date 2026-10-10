# FAQ

## Something is not working

**The app says it cannot reach my server.** Check the address (it must start with `https://`). On the server run `hearth-server status`. Make sure ports 80 and 443 are open at your hosting company.

**My phone does not find my computer.** Both must be on the same Wi-Fi, and Hearth must be open on the computer. Some guest or office Wi-Fi networks block devices from seeing each other; use *Enter the address myself* then.

**The code is wrong or ran out.** Codes work for 10 minutes. Run `hearth-server code` on the server for a new one.

**A chat does not show on my other device.** Open **Settings → Sync** on the desktop app and press *Sync now*. Both lines should say OK. A chat that lives outside your `projects` folder stays on its own computer; other devices see it marked *only on …*.

**My files are not syncing.** Files must be inside your `projects` folder (in your home folder). Settings → Sync shows the last problem. Big generated folders like `node_modules` are skipped on purpose.

**I see a file called `something.conflict-…`.** You changed the same file on two devices. Hearth kept the newest as the real file and saved the other one with that name. Compare them and delete the one you do not need.

**I deleted a file by accident.** Look in the `.hearth-trash` folder inside your projects folder.

**It says some files "look deleted" and nothing was deleted.** That is the safety brake. Check that the right folder is connected. If you really deleted many files, delete them again in smaller steps.

**Claude Code was not found.** Install it (claude.com/claude-code), open a terminal, type `claude` once and sign in. Then restart Hearth.

## Installing the phone app

**Android says "App not installed" (or "something went wrong").** Almost always an older Hearth is already on the phone that was installed another way (a test build). Android refuses to replace an app with one signed by a different key. Fix: open Settings → Apps → Hearth → **Uninstall**, then install `Hearth-….apk` again. The first time Android also shows "Install anyway" (because Hearth is not in the Play Store) and may ask to allow your browser to install apps: allow it.

If it still fails: the file may be incomplete. Download it again, and make sure your phone has at least 100 MB free.

## Several computers

Connect each computer to the same server (the tutorial or *Settings → Computers and servers*). In a chat, **Options → Run on** (and when you start a new chat) you pick: *Auto*, the *Server*, or one of your computers by name (● online, ○ off). A computer that is off starts the work when you switch it on. In any chat you can also tell Claude: "set this up on my desktop PC". Project files are kept the same on all of them.

To show the code and link for another computer again, type `hearth-server code` on the server (`hearth-<name>-server code` if the server holds several Hearths).

## What is "Usage and cost"?

The dashboard counts the tokens in your Claude Code chats and prices them at Anthropic's published API prices. A Pro or Max plan is not billed per token, so read it as "what this work would cost on the API". Numbers come from the chats on that computer (the server has everything that was synced).

## Does Hearth work with Claude Cowork?

Not directly. Cowork lives inside the Claude desktop app and has no public interface that other apps can drive. What Hearth gives you instead is Claude Code with the same chats on every device, and plain-language *Talk* chats. If Anthropic opens Cowork up (for example through an API or remote connector), Hearth can follow.

## Several people, one server

Two people can run Hearth on the same server: the installer notices an existing Hearth and sets up a separate copy (its own address, key and service, named `hearth-<name>`). It never overwrites the first one. Each person uses their own commands, for example `hearth-server code` or `hearth-<name>-server code`.

## Updates

**How do I update?** Open Hearth → *Settings → Updates → Check for updates* (on the phone: *Settings → Updates*). Hearth looks at the newest release on GitHub. If there is one, press **Update**: the app downloads it and installs it. On the phone, Android asks you once to allow Hearth to install updates, then shows an *Update* button. If you are connected to a server, the same screen can update the server too.

**The phone says the update cannot be installed.** Android only installs an update when it is signed by the same key as the installed app. If you installed a test build, remove the app once and install the one from the GitHub release.

## Using your computer as the server

You do not need a rented server. In the tutorial choose **On this computer**. Your phone connects to the address and code that the desktop app shows (*Settings → Connect your phone*). Away from home, install **Tailscale** on both devices (free): the desktop app then shows a second address to use. Opening a port on your router also works, but it is not encrypted, so Tailscale is the safer choice.

## General

**What does Hearth cost?** Nothing. You pay for your server and your Claude plan.

**Does Hearth see my code or chats?** Only your own server and computers do. There is no Hearth cloud and no tracking.

**Which Claude plan do I need?** Whatever lets you use Claude Code. Hearth shows your limits when Claude Code reports them.

**Can several people use one server?** The server has one secret key, so it is meant for one person (and their devices). Use a server per person.

**Can I run it without a server?** Yes: use the apps with just your computer. Your phone then connects over your Wi-Fi.

**Where do I report bugs or ask for features?** Open an issue on GitHub.

**Is this affiliated with Anthropic?** No. It is an independent project. "Claude" is a trademark of Anthropic.
