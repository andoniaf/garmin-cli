//! Workout commands for garmin-cli: list, get, types, delete and download.

use std::io::{self, Write};

use crate::client::GarminClient;
use crate::config::CredentialStore;
use crate::error::{GarminError, Result};
use crate::models::WorkoutSummary;

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

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a very long workout name", 10), "a very ...");
    }
}
