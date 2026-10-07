---
name: garmin-cli
description: Use the `garmin` CLI to interact with Garmin Connect — list/inspect activities and devices, read health and performance metrics, sync data locally, and author/push running workouts (including the compact step notation) to a real account. Use whenever a task involves Garmin Connect data or creating/scheduling/deleting a workout on a Garmin account.
---

# garmin-cli

A Rust CLI for the Garmin Connect API. Covers auth, activities, health metrics, devices, profile, local sync, and running workouts.

## Prerequisites

- Build it: `cargo build --release -p garmin-cli` (binary lands at `target/release/garmin`; debug builds work too, at `target/debug/garmin`). It is not assumed to be on `PATH` — use the full path to the binary unless you've confirmed otherwise.
- Authenticate once: `garmin auth login` (prompts for email/password, handles MFA). Check status any time with `garmin auth status`.
- Multiple Garmin accounts: pass `--profile <name>` on any command, or set `GARMIN_PROFILE=<name>`. Each profile has its own stored credentials.
- Every subcommand supports `--help` for the exhaustive flag list — this skill covers "how do I accomplish X," not every flag.

## Activities

```bash
garmin activities list [--limit N]
garmin activities get <id>
garmin activities note get|set|clear <id> [note text]
garmin activities download <id> --type fit|gpx|tcx|kml [--output path]
garmin activities upload <file.fit>
```

## Health & performance

```bash
garmin health summary|sleep|stress|body-battery|heart-rate [--date YYYY-MM-DD] [--days N]
garmin health vo2max|training-readiness|training-status|hrv|fitness-age [--date ...] [--days ...]
garmin health weight [--from ... --to ...]  /  weight-add <kg> [--unit kg|lbs]
garmin health lactate-threshold|race-predictions|endurance-score|hill-score|personal-records|performance-summary
garmin health spo2|respiration|intensity-minutes|blood-pressure|hydration
garmin health insights [--days N]   # sleep/stress correlations
```

Full flag list is in the README's "Health Commands" section — reach for that if a metric here needs a less common option.

## Devices & profile

```bash
garmin devices list
garmin devices get <device-id>
garmin devices history                 # from locally synced data
garmin profile show
garmin profile settings
```

## Local sync (Parquet)

```bash
garmin sync run [--from ... --to ...] [--activities|--health|--performance] [--backfill] [--dry-run]
garmin sync status
garmin sync reset   # failed tasks -> pending
garmin sync clear   # drop pending tasks
```

Syncs into time-partitioned Parquet files (see README's "Storage Architecture" / "Querying with DuckDB" sections) for offline analysis — reach for `sync` when the task is "analyze my training history," not "do one thing to one activity."

## Workouts

Creates running workouts from a compact step notation and pushes them to Garmin Connect, where they sync to the watch.

```bash
garmin workouts list [--limit N] [--start N]
garmin workouts get <id>
garmin workouts types                                  # live account's actual type IDs
garmin workouts create --name "<1-32 chars>" --steps "<notation>" [--description "..."] [--schedule YYYY-MM-DD] [--dry-run]
garmin workouts create --file workout.json [--name ...] [--description ...]   # raw JSON escape hatch
garmin workouts schedule <id> <YYYY-MM-DD>
garmin workouts delete <id> [--yes]                     # --yes skips the confirmation prompt
garmin workouts download <id> [--output path.fit]
```

### Step notation

```
workout  := item (';' | ',' separated)*
item     := repeat | step
repeat   := COUNT ('x'|'X'|'×') '(' step, ... ')'      -- 1-99 iterations, no nesting
step     := kind end [target] [note]

kind     := wu|warmup, cd|cooldown, int|interval, rec|recovery, rest, other

end      := 'lap'
          | duration: 90s, 10min, 10m, 1h30m, 4:00 (m:ss), 1:05:00 (h:mm:ss)  -- 1s to 6h
          | distance: 400mt, 0.4km, 1.5mi                                     -- up to 100km

target   := '@z1'..'@z5'                             -- heart-rate zone
          | '@4:30/km'                                -- single pace, ±5s/km band
          | '@4:30-4:40/km' or '@7:00-7:20/mi'        -- pace range, either order -- 2:00-15:00 pace

note     := '[free text]'                             -- becomes the step's on-watch description
```

`400m` is rejected with a guidance message (`400m means 400 minutes; for meters write 400mt or 0.4km`) — bare `m` after a number ≥100 is almost always a typo for meters. Parse errors point at the exact column with a caret, e.g. `invalid --steps at column 17: expected duration, distance or 'lap'`.

### Confirmed from live smoke-testing against a real account

These are facts, not caveats to hedge on:

- **No raw-bpm targets.** Only HR *zones* 1-5 (`@z1`-`@z5`) are supported — there's no way to target "keep HR below 145 bpm" directly. If a user gives you a bpm ceiling or similar nuanced guidance, put it in that step's `[note]` (it becomes the step's description, shown on the watch) rather than inventing a zone that isn't what they asked for.
- **Workout name is capped at 1-32 characters.** Keep it tight; put the full description in `--description` or step notes instead.
- **Always `--dry-run` first.** It prints the exact Garmin JSON payload with no network call — review it before creating anything live.
- **`workouts types`** shows the live account's actual step/condition/target/sport type IDs — check it if a create ever behaves unexpectedly.
- **`workouts delete <id> --yes`** skips the interactive confirmation — needed for non-interactive use; without `--yes` it prompts `[y/N]`.
- **`workouts create --schedule YYYY-MM-DD`** schedules in the same call; or do it after the fact with `workouts schedule <id> <date>`.
- **UTF-8 round-trips correctly** — Spanish/accented text in names, descriptions, and step notes comes back unmodified from Garmin.

### Worked example 1: HR-zone repeat workout

```bash
garmin workouts create --name "CLI Sample: Z2/3xZ4" \
  --steps "wu 10m @z2; 3x(int 4m @z4, rec 3m @z3); cd 10m @z1"
```

10 min warmup @Z2, then 3×(4 min @Z4 interval, 3 min @Z3 recovery), then 10 min cooldown @Z1.

### Worked example 2: distance step with a free-text HR/pace note, plus short pickups

```bash
garmin workouts create \
  --name "Suave 6.5km + 4x20s" \
  --description "Recuperación tras Monza (~40' total)." \
  --steps "other 6.5km [Suave. Ritmo 5:45/km o más lento, pulso por debajo de 145 (ayer subió a ~150 al final: si hoy sube de 145, frena aunque parezca lentísimo -- así se recupera la pierna tras Monza). ~35-40 min.]; 4x(int 20s [Rápido pero suelto, sin esprintar], rec 1m [Andando o trotando entre cada una])"
```

A 6.5 km easy run (`other` kind, no HR-zone target — the bpm ceiling and pace guidance live in the `[note]`), followed by 4×(20s fast-but-relaxed pickup, 1 min walk/jog recovery), each with its own `[note]`.

## Safety

Creating, scheduling, and deleting workouts are **live writes to the user's real Garmin account**. Confirm with the user before doing them, unless they've already clearly asked for it in this conversation (e.g. "create this workout," "test it live," "send this to my Garmin"). The same goes for any other side-effecting command (activity note edits, weight-add, uploads).
