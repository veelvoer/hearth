# FAQ

## Something is not working

**The app says it cannot reach my server.** Check the address (it must start with `https://`). On the server run `hearth-server status`. Make sure ports 80 and 443 are open at your hosting company.

**The code is wrong or ran out.** Codes work for 10 minutes. Run `hearth-server code` on the server for a new one.

**A chat does not show on my other device.** Open **Settings → Sync** on the desktop app and press *Sync now*. Both lines should say OK. A chat that lives outside your `projects` folder stays on its own computer; other devices see it marked *only on …*.

**My files are not syncing.** Files must be inside your `projects` folder (in your home folder). Settings → Sync shows the last problem. Big generated folders like `node_modules` are skipped on purpose.

**I see a file called `something.conflict-…`.** You changed the same file on two devices. Hearth kept the newest as the real file and saved the other one with that name. Compare them and delete the one you do not need.

**I deleted a file by accident.** Look in the `.hearth-trash` folder inside your projects folder.

**It says some files "look deleted" and nothing was deleted.** That is the safety brake. Check that the right folder is connected. If you really deleted many files, delete them again in smaller steps.

**Claude Code was not found.** Install it (claude.com/claude-code), open a terminal, type `claude` once and sign in. Then restart Hearth.

## General

**What does Hearth cost?** Nothing. You pay for your server and your Claude plan.

**Does Hearth see my code or chats?** Only your own server and computers do. There is no Hearth cloud and no tracking.

**Which Claude plan do I need?** Whatever lets you use Claude Code. Hearth shows your limits when Claude Code reports them.

**Can several people use one server?** The server has one secret key, so it is meant for one person (and their devices). Use a server per person.

**Can I run it without a server?** Yes: use the apps with just your computer. Your phone then connects over your Wi-Fi.

**Where do I report bugs or ask for features?** Open an issue on GitHub.

**Is this affiliated with Anthropic?** No. It is an independent project. "Claude" is a trademark of Anthropic.
