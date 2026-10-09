//! Hardcoded (id, key) pairs for the running-focused workout builder.
//!
//! Verified against a live `garmin workouts types` response (captured in
//! `tests/fixtures/workout_types.json`).

/// Sport type: running.
pub const SPORT_RUNNING: (i64, &str) = (1, "running");

/// Step types.
pub const STEP_WARMUP: (i64, &str) = (1, "warmup");
pub const STEP_COOLDOWN: (i64, &str) = (2, "cooldown");
pub const STEP_INTERVAL: (i64, &str) = (3, "interval");
pub const STEP_RECOVERY: (i64, &str) = (4, "recovery");
pub const STEP_REST: (i64, &str) = (5, "rest");
pub const STEP_REPEAT: (i64, &str) = (6, "repeat");
pub const STEP_OTHER: (i64, &str) = (7, "other");

/// End condition types.
pub const CONDITION_LAP_BUTTON: (i64, &str) = (1, "lap.button");
pub const CONDITION_TIME: (i64, &str) = (2, "time");
pub const CONDITION_DISTANCE: (i64, &str) = (3, "distance");
pub const CONDITION_ITERATIONS: (i64, &str) = (7, "iterations");

/// Target types.
pub const TARGET_NO_TARGET: (i64, &str) = (1, "no.target");
pub const TARGET_HEART_RATE_ZONE: (i64, &str) = (4, "heart.rate.zone");
pub const TARGET_PACE_ZONE: (i64, &str) = (6, "pace.zone");
