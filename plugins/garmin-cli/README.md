# garmin-cli Claude Code plugin

Ships the `garmin-cli` skill, which teaches Claude Code how to drive the `garmin` CLI, including running workouts written in the compact step notation.

## Install

```bash
claude plugin marketplace add andoniaf/garmin-cli
claude plugin install garmin-cli@andoniaf --scope user
```

You can also run the same commands inside a session as `/plugin marketplace add …` and `/plugin install …`. The skill shows up as `/garmin-cli:garmin-cli`.

The skill needs the `garmin` binary from this fork:

```bash
cargo install --git https://github.com/andoniaf/garmin-cli --branch main garmin-cli --locked
garmin auth login
```

## Updates

The plugin has no `version` field on purpose, so each commit to `main` counts as a new release.

- **Automatic:** open `/plugin`, go to the **Marketplaces** tab, select `andoniaf` and choose **Enable auto-update**. It is off by default for third-party marketplaces.
- **Manual:** run `claude plugin marketplace update andoniaf && claude plugin update garmin-cli@andoniaf`.

## Develop

Edit `skills/garmin-cli/SKILL.md`, then run `claude plugin validate .` from the repo root. The only expected warning is the missing `version`.
