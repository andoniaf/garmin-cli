//! Workout data models for Garmin Connect API.
//!
//! Read-side models are permissive (all fields optional, unknown fields kept
//! via `extra`) because the workout JSON shape is only partially documented.

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

/// Sport type reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SportType {
    pub sport_type_id: i64,
    pub sport_type_key: String,
}
