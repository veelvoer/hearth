# Release notes

One file per version, named `X.Y.Z.md`, written for the people who use Hearth: what is new or fixed, in plain words.

`node scripts/set-version.js X.Y.Z` makes a stub for the new version. Replace the TODO line, then tag. The release workflow refuses a tag whose notes are missing or still say TODO, and publishes the file as the text of the GitHub release. The apps show the same text when an update is ready.
