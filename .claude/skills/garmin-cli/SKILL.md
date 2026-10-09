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

- **No raw-bpm targets.** Only HR *zones* 1-5 (`@z1`-`@z5`) are supported — there's no way to target "keep HR below 145 bpm" directly. Resolve a stated bpm range/ceiling to a zone number instead (see "Mapping a bpm range to a zone" below); only fall back to a `[note]` if no zone reasonably fits.
- **Prefer giving every step an explicit target.** This user's stated default: no "sin objetivo" / target-less steps when a zone or pace range can be set instead — including short interval/recovery steps inside a repeat group, which are easy to forget. Treat this as the default for any running workout unless the user says otherwise.
- **No `workouts update` command.** Fixing a live workout's steps/targets means `delete` + recreate with the corrected `--steps`. If the old workout was scheduled, the schedule entry is tied to its workout id and is lost on delete — re-apply `--schedule YYYY-MM-DD` (or `workouts schedule`) on the new one.
- **Workout name is capped at 1-32 characters.** Keep it tight; put the full description in `--description` or step notes instead.
- **Always `--dry-run` first.** It prints the exact Garmin JSON payload with no network call — review it before creating anything live.
- **`workouts types`** shows the live account's actual step/condition/target/sport type IDs — check it if a create ever behaves unexpectedly.
- **`workouts delete <id> --yes`** skips the interactive confirmation — needed for non-interactive use; without `--yes` it prompts `[y/N]`.
- **`workouts create --schedule YYYY-MM-DD`** schedules in the same call; or do it after the fact with `workouts schedule <id> <date>`.
- **UTF-8 round-trips correctly** — Spanish/accented text in names, descriptions, and step notes comes back unmodified from Garmin.

### Mapping a bpm range to a zone

HR zone boundaries are per-account, user-editable in the Garmin app, and *change* — don't reuse a boundary you fetched earlier in the conversation if the user mentions editing their zones. There's no CLI subcommand for this yet; fetch live boundaries directly:

```bash
TOKEN=$(python3 -c "import json; print(json.load(open('<profile_dir>/oauth2_token.json'))['access_token'])")
curl -s "https://connectapi.garmin.com/biometric-service/heartRateZones/" \
  -H "Authorization: Bearer $TOKEN" -H "User-Agent: GCM-iOS-5.7.2.1" | python3 -m json.tool
```

(Profile dir defaults to `~/.local/share/garmin/<profile>/` on Linux; `garmin auth status` confirms the active profile.) The response gives `zone1Floor`..`zone5Floor` and `maxHeartRateUsed`; zone N spans `zoneNFloor` to `zone(N+1)Floor - 1` (zone 5 runs to max). Pick the zone whose range best contains the stated bpm range — an exact floor match on the low end is a strong signal; if the user's ceiling falls strictly inside a zone (not at a boundary), say so explicitly rather than silently picking a side, and note the discrepancy in the step's `[note]`.

### Worked example 1: HR-zone repeat workout

```bash
garmin workouts create --name "CLI Sample: Z2/3xZ4" \
  --steps "wu 10m @z2; 3x(int 4m @z4, rec 3m @z3); cd 10m @z1"
```

10 min warmup @Z2, then 3×(4 min @Z4 interval, 3 min @Z3 recovery), then 10 min cooldown @Z1.

### Worked example 2: HR-zone distance step, plus pace-targeted pickups (every step has a target)

```bash
garmin workouts create \
  --name "Suave 6.5km + 4x20s" \
  --description "Recuperación tras Monza (~40' total). Aceleraciones y recuperaciones por ritmo." \
  --steps "other 6.5km @z2 [Suave, dentro de Z2. Ritmo de referencia 5:45/km o más lento. Ayer el pulso subió a ~150 al final: si hoy te acercas al techo de Z2, frena aunque parezca lentísimo -- así se recupera la pierna tras Monza. ~35-40 min.]; 4x(int 20s @3:10-3:35/km [Rápido pero suelto, sin esprintar], rec 1m @6:30-10:00/km [Andando o trotando entre cada una])"
```

This evolved from an earlier target-less version (first drafted with no `@z2` on the run and no pace on the pickups/recoveries) once the user clarified they want an explicit objective on *every* step so they don't have to watch the screen: a 6.5 km easy run capped at Z2 (130-149 bpm, chosen because the user's stated 145 bpm ceiling sits almost exactly at the top of their Z2 — see "Mapping a bpm range to a zone"), then 4×(20s pickup at 3:10-3:35/km, 1 min recovery at 6:30-10:00/km), each step still carrying its own `[note]` for on-watch context.

### Worked example 3: zones referenced by number only, no free/target-less steps

```bash
garmin workouts create \
  --name "Carrera a la oficina" \
  --description "Camino al trabajo: 4km a FC Z2 (incluye calentamiento), 1km a ritmo 5:15-5:25/km, enfriamiento Z1 hasta la puerta." \
  --steps "other 4km @z2 [El primer km hace de calentamiento. Dentro de Z2, tu tope de suave son 145, aunque el reloj no avise hasta 149.]; other 1km @5:15-5:25/km [Ritmo porque el pulso de muñeca llega tarde; deberías acabar en Z3 (150-160), tu zona de media. Con mochila, quédate en la parte lenta del rango.]; cd lap @z1 [Trote o andando hasta la puerta de la oficina.]" \
  --schedule 2026-10-08
```

A commute run collapsed to 3 steps, each with a zone or pace target (no separate warmup/cooldown-without-objective steps): 4 km @Z2 (first km doubles as the warmup — called out in the `[note]`, not a separate step), 1 km @5:15-5:25/km, cooldown-by-lap-button @Z1. This replaced a 4-step draft that had used a target-less `wu` and a target-less `cd` — once the user's zones were confirmed live and the "always a target" preference was stated, it collapsed to this shape.

## Safety

Creating, scheduling, and deleting workouts are **live writes to the user's real Garmin account**. Confirm with the user before doing them, unless they've already clearly asked for it in this conversation (e.g. "create this workout," "test it live," "send this to my Garmin"). The same goes for any other side-effecting command (activity note edits, weight-add, uploads).
