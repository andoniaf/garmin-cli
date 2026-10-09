//! Workout commands for garmin-cli: list, get, types, create, schedule,
//! delete, download of running workouts built from compact step notation.

use std::io::{self, Write};
use std::path::Path;

use chrono::NaiveDate;

use crate::client::{GarminClient, OAuth2Token};
use crate::config::CredentialStore;
use crate::error::{GarminError, Result};
use crate::models::WorkoutSummary;
use crate::workout::{builder, notation};

use super::auth::refresh_token;

const WORKOUT_PATH: &str = "/workout-service/workout";

/// List workouts.
pub async fn list(limit: u32, start: u32, profile: Option<String>) -> Result<()> {
    let store = CredentialStore::new(profile)?;
    let (_, oauth2) = refresh_token(&store).await?;

    let client = GarminClient::new();
    let path = format!(
        "/workout-service/workouts?start={}&limit={}&myWorkoutsOnly=true",
        start, limit
    );

    let workouts: Vec<WorkoutSummary> = client.get_json(&oauth2, &path).await?;

    if workouts.is_empty() {
        println!("No workouts found.");
        return Ok(());
    }

    println!(
        "{:<12} {:<32} {:<10} {:>10} {:>10} {:<12}",
        "ID", "Name", "Sport", "Duration", "Distance", "Updated"
    );
    println!("{}", "-".repeat(90));

    for workout in &workouts {
        let distance = workout
            .distance_km()
            .map(|d| format!("{:.2} km", d))
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<12} {:<32} {:<10} {:>10} {:>10} {:<12}",
            workout
                .workout_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            truncate(&workout.display_name(), 32),
            workout.sport_key(),
            workout.duration_formatted(),
            distance,
            workout.updated_date_short(),
        );
    }

    println!("\nShowing {} workouts", workouts.len());

    Ok(())
}

/// Get full workout details as raw JSON.
pub async fn get(id: u64, profile: Option<String>) -> Result<()> {
    let store = CredentialStore::new(profile)?;
    let (_, oauth2) = refresh_token(&store).await?;

    let client = GarminClient::new();
    let path = format!("{}/{}", WORKOUT_PATH, id);

    let value: serde_json::Value = client.get_json(&oauth2, &path).await?;
    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}

/// Print the live workout type enumerations (step/condition/target/sport types).
pub async fn types(profile: Option<String>) -> Result<()> {
    let store = CredentialStore::new(profile)?;
    let (_, oauth2) = refresh_token(&store).await?;

    let client = GarminClient::new();
    let value: serde_json::Value = client
        .get_json(&oauth2, "/workout-service/workout/types")
        .await?;
    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}

/// Create a workout from compact step notation or a raw JSON file, optionally
/// scheduling it to a date afterward.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    name: Option<String>,
    steps: Option<String>,
    file: Option<std::path::PathBuf>,
    description: Option<String>,
    schedule: Option<String>,
    dry_run: bool,
    profile: Option<String>,
) -> Result<()> {
    if let Some(date) = &schedule {
        validate_date(date)?;
    }

    let body = build_create_body(
        name.as_deref(),
        steps.as_deref(),
        file.as_deref(),
        description.as_deref(),
    )?;

    if dry_run {
        println!("{}", serde_json::to_string_pretty(&body)?);
        return Ok(());
    }

    let store = CredentialStore::new(profile)?;
    let (_, oauth2) = refresh_token(&store).await?;
    let client = GarminClient::new();

    let result = create_workout(&client, &oauth2, &body).await?;
    let id = result
        .get("workoutId")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| GarminError::invalid_response("Create response missing workoutId"))?;
    let created_name = result
        .get("workoutName")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    println!("Created workout {}: {}", id, created_name);

    if let Some(date) = schedule {
        match schedule_workout(&client, &oauth2, id, &date).await {
            Ok(resp) => {
                println!("Scheduled for {}", date);
                if let Some(schedule_id) = resp
                    .as_ref()
                    .and_then(|v| v.get("workoutScheduleId"))
                    .and_then(|v| v.as_u64())
                {
                    println!("schedule id {}", schedule_id);
                }
            }
            Err(e) => {
                eprintln!("Created workout {} but scheduling failed: {}", id, e);
                eprintln!("retry with: garmin workouts schedule {} {}", id, date);
                return Err(GarminError::Other(format!(
                    "workout {} created but scheduling failed",
                    id
                )));
            }
        }
    }

    Ok(())
}

/// Schedule an existing workout to a calendar date.
pub async fn schedule(id: u64, date: String, profile: Option<String>) -> Result<()> {
    validate_date(&date)?;

    let store = CredentialStore::new(profile)?;
    let (_, oauth2) = refresh_token(&store).await?;
    let client = GarminClient::new();

    let resp = schedule_workout(&client, &oauth2, id, &date).await?;
    println!("Scheduled for {}", date);
    if let Some(schedule_id) = resp
        .as_ref()
        .and_then(|v| v.get("workoutScheduleId"))
        .and_then(|v| v.as_u64())
    {
        println!("schedule id {}", schedule_id);
    }

    Ok(())
}

/// Delete a workout, prompting for confirmation unless `yes` is set.
pub async fn delete(id: u64, yes: bool, profile: Option<String>) -> Result<()> {
    if !yes {
        print!("Delete workout {}? [y/N] ", id);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let store = CredentialStore::new(profile)?;
    let (_, oauth2) = refresh_token(&store).await?;
    let client = GarminClient::new();

    let path = format!("{}/{}", WORKOUT_PATH, id);
    client.delete(&oauth2, &path).await?;
    println!("Deleted workout {}", id);

    Ok(())
}

/// Download a workout as a FIT file.
pub async fn download(id: u64, output: Option<String>, profile: Option<String>) -> Result<()> {
    let store = CredentialStore::new(profile)?;
    let (_, oauth2) = refresh_token(&store).await?;
    let client = GarminClient::new();

    let path = format!("/workout-service/workout/FIT/{}", id);
    let bytes = client.download(&oauth2, &path).await?;

    let output_path = output.unwrap_or_else(|| format!("workout_{}.fit", id));
    tokio::fs::write(&output_path, &bytes)
        .await
        .map_err(|e| GarminError::invalid_response(format!("Failed to write file: {}", e)))?;

    println!("Saved to: {}", output_path);
    println!("Size: {} bytes", bytes.len());

    Ok(())
}

/// POST a `WorkoutCreate` body and return the created workout as JSON.
/// Client-injected so it can be exercised against a wiremock server directly.
pub async fn create_workout(
    client: &GarminClient,
    token: &OAuth2Token,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    client.post_json(token, WORKOUT_PATH, body).await
}

/// POST a schedule request for a workout. Client-injected so it can be
/// exercised against a wiremock server directly; the response shape is
/// undocumented, so it's returned as an optional raw value.
pub async fn schedule_workout(
    client: &GarminClient,
    token: &OAuth2Token,
    id: u64,
    date: &str,
) -> Result<Option<serde_json::Value>> {
    let path = format!("/workout-service/schedule/{}", id);
    let body = serde_json::json!({ "date": date });
    client.post_json_optional(token, &path, &body).await
}

/// Build the JSON body for a create request, either from compact step
/// notation or from a raw `--file`. Pure (no network); validated before any
/// network call is made.
fn build_create_body(
    name: Option<&str>,
    steps: Option<&str>,
    file: Option<&Path>,
    description: Option<&str>,
) -> Result<serde_json::Value> {
    if let Some(file_path) = file {
        let contents = std::fs::read_to_string(file_path).map_err(|e| {
            GarminError::invalid_param(format!("Failed to read {}: {}", file_path.display(), e))
        })?;
        let mut value: serde_json::Value = serde_json::from_str(&contents).map_err(|e| {
            GarminError::invalid_param(format!("Invalid JSON in {}: {}", file_path.display(), e))
        })?;
        let obj = value
            .as_object_mut()
            .ok_or_else(|| GarminError::invalid_param("--file must contain a JSON object"))?;

        if let Some(n) = name {
            obj.insert(
                "workoutName".to_string(),
                serde_json::Value::String(n.to_string()),
            );
        }
        if let Some(d) = description {
            obj.insert(
                "description".to_string(),
                serde_json::Value::String(d.to_string()),
            );
        }

        let workout_name = obj
            .get("workoutName")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                GarminError::invalid_param(
                    "workout requires a name (--name or workoutName in --file)",
                )
            })?;
        validate_name(workout_name)?;

        Ok(value)
    } else {
        let name = name.ok_or_else(|| GarminError::invalid_param("--name is required"))?;
        validate_name(name)?;

        let steps = steps.ok_or_else(|| GarminError::invalid_param("--steps is required"))?;
        let items = notation::parse(steps)
            .map_err(|e| GarminError::InvalidParameter(e.to_display("--steps", steps)))?;

        let workout_create = builder::build_workout(name, description, &items);
        serde_json::to_value(&workout_create).map_err(GarminError::Json)
    }
}

fn validate_name(name: &str) -> Result<()> {
    let len = name.chars().count();
    if !(1..=32).contains(&len) {
        return Err(GarminError::invalid_param(format!(
            "workout name must be 1-32 characters (got {})",
            len
        )));
    }
    Ok(())
}

fn validate_date(date: &str) -> Result<()> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| GarminError::InvalidDateFormat(date.to_string()))
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_token() -> OAuth2Token {
        OAuth2Token {
            scope: "test".to_string(),
            jti: "jti".to_string(),
            token_type: "Bearer".to_string(),
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expires_in: 3600,
            expires_at: chrono::Utc::now().timestamp() + 3600,
            refresh_token_expires_in: 86400,
            refresh_token_expires_at: chrono::Utc::now().timestamp() + 86400,
        }
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a very long workout name", 10), "a very ...");
    }

    #[test]
    fn test_validate_name() {
        assert!(validate_name("Tempo").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name(&"x".repeat(33)).is_err());
        assert!(validate_name(&"x".repeat(32)).is_ok());
    }

    #[test]
    fn test_validate_date() {
        assert!(validate_date("2026-10-02").is_ok());
        assert!(validate_date("not-a-date").is_err());
        assert!(validate_date("2026/10/02").is_err());
    }

    #[test]
    fn test_build_create_body_from_steps() {
        let body =
            build_create_body(Some("Tempo"), Some("wu 10m @z2; cd 10m @z1"), None, None).unwrap();
        assert_eq!(body["workoutName"], "Tempo");
        assert_eq!(body["sportType"]["sportTypeKey"], "running");
        assert_eq!(
            body["workoutSegments"][0]["workoutSteps"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn test_build_create_body_rejects_bad_steps() {
        let err = build_create_body(Some("Tempo"), Some("wu"), None, None).unwrap_err();
        assert!(matches!(err, GarminError::InvalidParameter(_)));
        assert!(err.to_string().contains("column"));
    }

    #[test]
    fn test_build_create_body_from_file_overrides_name_and_description() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("workout.json");
        std::fs::write(
            &file_path,
            r#"{"workoutName":"Old Name","sportType":{"sportTypeId":1,"sportTypeKey":"running"},"workoutSegments":[]}"#,
        )
        .unwrap();

        let body =
            build_create_body(Some("New Name"), None, Some(&file_path), Some("desc")).unwrap();
        assert_eq!(body["workoutName"], "New Name");
        assert_eq!(body["description"], "desc");
    }

    #[test]
    fn test_build_create_body_from_file_without_name_uses_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("workout.json");
        std::fs::write(
            &file_path,
            r#"{"workoutName":"From File","sportType":{"sportTypeId":1,"sportTypeKey":"running"},"workoutSegments":[]}"#,
        )
        .unwrap();

        let body = build_create_body(None, None, Some(&file_path), None).unwrap();
        assert_eq!(body["workoutName"], "From File");
    }

    #[tokio::test]
    async fn test_create_workout_sends_body_and_returns_response() {
        let server = MockServer::start().await;
        let request_body = serde_json::json!({"workoutName": "Tempo"});
        let response_body = serde_json::json!({"workoutId": 555, "workoutName": "Tempo"});

        Mock::given(method("POST"))
            .and(path("/workout-service/workout"))
            .and(body_json(&request_body))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = GarminClient::new_with_base_url(&server.uri());
        let result = create_workout(&client, &test_token(), &request_body)
            .await
            .unwrap();
        assert_eq!(result["workoutId"], 555);
    }

    #[tokio::test]
    async fn test_schedule_workout_with_json_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/workout-service/schedule/42"))
            .and(body_json(serde_json::json!({"date": "2026-10-02"})))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"workoutScheduleId": 7})),
            )
            .mount(&server)
            .await;

        let client = GarminClient::new_with_base_url(&server.uri());
        let result = schedule_workout(&client, &test_token(), 42, "2026-10-02")
            .await
            .unwrap();
        assert_eq!(result.unwrap()["workoutScheduleId"], 7);
    }

    #[tokio::test]
    async fn test_schedule_workout_with_empty_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/workout-service/schedule/42"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = GarminClient::new_with_base_url(&server.uri());
        let result = schedule_workout(&client, &test_token(), 42, "2026-10-02")
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
