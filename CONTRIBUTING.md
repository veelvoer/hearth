# Contributing

Thank you! Hearth should stay **simple for beginners**. When you change something, ask: could a 10 year old follow this?

## Set up

```bash
git clone https://github.com/veelvoer/hearth && cd hearth
cd desktop && npm install
npm start                  # run the desktop app
npm run test:sync          # starts two relays and checks chat + file sync (about 90 seconds)
npm run test:layout        # opens every screen at three window sizes and reports clipped or overflowing things
```

Phone app: `./gradlew :app:assembleDebug` (JDK 17 and the Android SDK).

## Pull requests

- Keep changes small and say what you tested.
- Words shown to people should be plain and friendly. No jargon without explaining it.
- New screens must pass `npm run test:layout`.
- Anything that deletes or overwrites a user's files needs a test in `desktop/test/sync.test.js`.

## Ideas that would help a lot

macOS build · iPhone app · phone version of the Add-ons panel · more languages · Flathub / AUR / winget packages.
