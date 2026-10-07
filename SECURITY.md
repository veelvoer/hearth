# Security

## Reporting a problem

Please do not open a public issue for a security problem. Use GitHub's **Report a vulnerability** button (Security tab of this repository), or contact the maintainer through their GitHub profile. You will get a reply as soon as possible.

## How Hearth protects you

- The server accepts requests only with a secret key and is meant to run behind HTTPS (the installer sets this up).
- Pairing uses a 6-digit code that works for 10 minutes and locks for a minute after 5 wrong tries.
- Hearth never asks for, stores or sends your Claude password or login token.
- The server confines Claude to the projects folder and does not allow "Full auto" unless started with `--allow-bypass`.
- The Android app stores the keys encrypted with the Android Keystore.

## Things to know

- Anyone who has the server's key can use Claude on your server. Keep pairing links private.
- Chats and files are copied between your own devices over HTTPS. They are not encrypted on the server's disk.
