//! Workout data models for Garmin Connect API.
//!
//! Read-side models are permissive (all fields optional, unknown fields kept
//! via `extra`) because the workout JSON shape is only partially documented.
//! Create-side models are strict and serialize-only: they describe exactly
//! the payload the builder produces.

use serde::{Deserialize, Serialize};

/// Abbreviated workout as returned by the workout list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkoutSummary {
    #[serde(default)]
    pub workout_id: Option<u64>,

    #[serde(default)]
    pub workout_name: Option<String>,

    #[serde(default)]
    pub sport_type: Option<SportType>,

    #[serde(default)]
    pub estimated_duration_in_secs: Option<f64>,

    #[serde(default)]
    pub estimated_distance_in_meters: Option<f64>,

    /// `updateDate`, not `updatedDate` as the OpenAPI spec names it — verified
    /// against a live Garmin Connect account (2026-10-02).
    #[serde(default, rename = "updateDate")]
    pub update_date: Option<String>,

    /// Catch-all for fields not modeled above.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl WorkoutSummary {
    pub fn display_name(&self) -> String {
        self.workout_name
            .clone()
            .unwrap_or_else(|| "Unnamed Workout".to_string())
    }

    pub fn sport_key(&self) -> String {
        self.sport_type
            .as_ref()
            .map(|s| s.sport_type_key.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Duration formatted as HH:MM:SS or MM:SS.
    pub fn duration_formatted(&self) -> String {
        match self.estimated_duration_in_secs {
            Some(secs) => {
                let total_secs = secs as u64;
                let hours = total_secs / 3600;
                let minutes = (total_secs % 3600) / 60;
                let seconds = total_secs % 60;
                if hours > 0 {
                    format!("{}:{:02}:{:02}", hours, minutes, seconds)
                } else {
                    format!("{}:{:02}", minutes, seconds)
                }
            }
            None => "-".to_string(),
        }
    }

    pub fn distance_km(&self) -> Option<f64> {
        self.estimated_distance_in_meters.map(|d| d / 1000.0)
    }

    /// Date portion of `updateDate`.
    pub fn updated_date_short(&self) -> String {
        self.update_date
            .as_ref()
            .map(|s| s.split(['T', ' ']).next().unwrap_or(s).to_string())
            .unwrap_or_else(|| "-".to_string())
    }
}

/// Full workout details, as returned by the workout-by-id endpoint. Only the
/// id is modeled; commands print the raw JSON so unknown fields never break.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workout {
    pub workout_id: u64,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Sport type reference, shared by read and create payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SportType {
    pub sport_type_id: i64,
    pub sport_type_key: String,
}

impl SportType {
    pub fn running() -> Self {
        let (id, key) = crate::workout::ids::SPORT_RUNNING;
        Self {
            sport_type_id: id,
            sport_type_key: key.to_string(),
        }
    }
}

/// Step type reference (warmup, cooldown, interval, recovery, rest, repeat, other).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StepType {
    pub step_type_id: i64,
    pub step_type_key: String,
}

/// End-condition type reference (time, distance, lap.button, iterations, ...).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConditionType {
    pub condition_type_id: i64,
    pub condition_type_key: String,
}

/// Target type reference (no.target, heart.rate.zone, pace.zone, ...).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TargetType {
    pub workout_target_type_id: i64,
    pub workout_target_type_key: String,
}

/// Distance unit for `preferredEndConditionUnit`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DistanceUnit {
    pub unit_key: String,
}

/// A single leaf step within a workout segment or repeat group.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableStep {
    pub step_id: Option<u64>,
    pub step_order: i64,
    /// 1-based index of the enclosing repeat group for steps inside one,
    /// `None` for top-level steps.
    pub child_step_id: Option<i64>,
    pub step_type: StepType,
    pub description: Option<String>,
    pub step_audio_note: Option<String>,
    pub end_condition: ConditionType,
    pub end_condition_value: Option<f64>,
    pub end_condition_compare: Option<String>,
    pub end_condition_zone: Option<i64>,
    pub preferred_end_condition_unit: Option<DistanceUnit>,
    pub target_type: TargetType,
    pub target_value_one: Option<f64>,
    pub target_value_two: Option<f64>,
    pub target_value_unit: Option<String>,
    pub zone_number: Option<i64>,
    pub secondary_target_type: Option<TargetType>,
    pub secondary_target_value_one: Option<f64>,
    pub secondary_target_value_two: Option<f64>,
    pub secondary_target_value_unit: Option<String>,
    pub secondary_zone_number: Option<i64>,
}

/// A group of steps repeated `number_of_iterations` times.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepeatGroup {
    pub step_id: Option<u64>,
    pub step_order: i64,
    /// 1-based index of this group among the workout's repeat groups.
    pub child_step_id: Option<i64>,
    pub step_type: StepType,
    pub number_of_iterations: i64,
    pub smart_repeat: bool,
    pub end_condition: ConditionType,
    pub end_condition_value: f64,
    pub preferred_end_condition_unit: Option<String>,
    pub skip_last_rest_step: bool,
    /// Always `ExecutableStepDTO` entries; nested repeat groups are not
    /// allowed, but this is typed as `WorkoutStep` (not `ExecutableStep`) so
    /// each entry still serializes its `type` discriminator.
    pub workout_steps: Vec<WorkoutStep>,
}

/// A workout step: either a leaf step or a repeat group. Internally tagged
/// on `type`, matching Garmin's `ExecutableStepDTO` / `RepeatGroupDTO` shapes.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum WorkoutStep {
    #[serde(rename = "ExecutableStepDTO")]
    Executable(ExecutableStep),
    #[serde(rename = "RepeatGroupDTO")]
    Repeat(RepeatGroup),
}

/// A segment of a workout (single-sport MVP: exactly one running segment).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkoutSegment {
    pub segment_order: i64,
    pub sport_type: SportType,
    pub workout_steps: Vec<WorkoutStep>,
}

/// Payload sent to `POST /workout-service/workout`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkoutCreate {
    pub workout_name: String,
    pub description: Option<String>,
    pub sport_type: SportType,
    pub workout_segments: Vec<WorkoutSegment>,
}
