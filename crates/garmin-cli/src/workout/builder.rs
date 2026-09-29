//! Builds a Garmin `WorkoutCreate` payload from a parsed notation AST.
//!
//! Pure, no I/O. `stepOrder` assignment uses a single global counter across
//! the whole workout (including steps nested in repeat groups), rather than
//! per-parent numbering — see the risk note in the implementation plan. This
//! is isolated to `next_order` so it's easy to flip if a live smoke test
//! shows Garmin expects per-parent order instead.

use crate::models::workout::{
    ConditionType, DistanceUnit, ExecutableStep, RepeatGroup, SportType, StepType, TargetType,
    WorkoutCreate, WorkoutSegment, WorkoutStep,
};
use crate::workout::ids;
use crate::workout::notation::{DistUnit, End, Item, Step, StepKind, Target};

/// Build a single-segment running `WorkoutCreate` from parsed items.
pub fn build_workout(name: &str, description: Option<&str>, items: &[Item]) -> WorkoutCreate {
    let mut order = 0i64;
    let workout_steps = items
        .iter()
        .map(|item| build_item(item, &mut order))
        .collect();

    WorkoutCreate {
        workout_name: name.to_string(),
        description: description.map(|d| d.to_string()),
        sport_type: SportType::running(),
        workout_segments: vec![WorkoutSegment {
            segment_order: 1,
            sport_type: SportType::running(),
            workout_steps,
        }],
    }
}

fn next_order(counter: &mut i64) -> i64 {
    *counter += 1;
    *counter
}

fn build_item(item: &Item, counter: &mut i64) -> WorkoutStep {
    match item {
        Item::Step(step) => WorkoutStep::Executable(build_executable_step(step, counter, None)),
        Item::Repeat { count, steps, .. } => {
            let step_order = next_order(counter);
            let workout_steps = steps
                .iter()
                .map(|s| WorkoutStep::Executable(build_executable_step(s, counter, Some(1))))
                .collect();
            let (repeat_id, repeat_key) = ids::STEP_REPEAT;
            let (iterations_id, iterations_key) = ids::CONDITION_ITERATIONS;
            WorkoutStep::Repeat(RepeatGroup {
                step_id: None,
                step_order,
                child_step_id: None,
                step_type: StepType {
                    step_type_id: repeat_id,
                    step_type_key: repeat_key.to_string(),
                },
                number_of_iterations: *count as i64,
                smart_repeat: false,
                end_condition: ConditionType {
                    condition_type_id: iterations_id,
                    condition_type_key: iterations_key.to_string(),
                },
                end_condition_value: *count as f64,
                preferred_end_condition_unit: None,
                skip_last_rest_step: false,
                workout_steps,
            })
        }
    }
}

fn build_executable_step(
    step: &Step,
    counter: &mut i64,
    child_step_id: Option<i64>,
) -> ExecutableStep {
    let step_order = next_order(counter);
    let (step_type_id, step_type_key) = step_type_ids(step.kind);
    let (end_condition, end_condition_value, preferred_end_condition_unit) = end_fields(&step.end);
    let (target_type, target_value_one, target_value_two, zone_number) =
        target_fields(&step.target);

    ExecutableStep {
        step_id: None,
        step_order,
        child_step_id,
        step_type: StepType {
            step_type_id,
            step_type_key: step_type_key.to_string(),
        },
        description: step.note.clone(),
        step_audio_note: None,
        end_condition,
        end_condition_value,
        end_condition_compare: None,
        end_condition_zone: None,
        preferred_end_condition_unit,
        target_type,
        target_value_one,
        target_value_two,
        target_value_unit: None,
        zone_number,
        secondary_target_type: None,
        secondary_target_value_one: None,
        secondary_target_value_two: None,
        secondary_target_value_unit: None,
        secondary_zone_number: None,
    }
}

fn step_type_ids(kind: StepKind) -> (i64, &'static str) {
    match kind {
        StepKind::Warmup => ids::STEP_WARMUP,
        StepKind::Cooldown => ids::STEP_COOLDOWN,
        StepKind::Interval => ids::STEP_INTERVAL,
        StepKind::Recovery => ids::STEP_RECOVERY,
        StepKind::Rest => ids::STEP_REST,
        StepKind::Other => ids::STEP_OTHER,
    }
}

fn end_fields(end: &End) -> (ConditionType, Option<f64>, Option<DistanceUnit>) {
    match end {
        End::Lap => {
            let (id, key) = ids::CONDITION_LAP_BUTTON;
            (condition(id, key), None, None)
        }
        End::Time(secs) => {
            let (id, key) = ids::CONDITION_TIME;
            (condition(id, key), Some(*secs as f64), None)
        }
        End::Distance { meters, unit } => {
            let (id, key) = ids::CONDITION_DISTANCE;
            let unit_key = match unit {
                DistUnit::Kilometer => "kilometer",
                DistUnit::Mile => "mile",
                DistUnit::Meter => "meter",
            };
            (
                condition(id, key),
                Some(*meters),
                Some(DistanceUnit {
                    unit_key: unit_key.to_string(),
                }),
            )
        }
    }
}

fn condition(id: i64, key: &str) -> ConditionType {
    ConditionType {
        condition_type_id: id,
        condition_type_key: key.to_string(),
    }
}

fn target_fields(target: &Target) -> (TargetType, Option<f64>, Option<f64>, Option<i64>) {
    match target {
        Target::None => {
            let (id, key) = ids::TARGET_NO_TARGET;
            (target_type(id, key), None, None, None)
        }
        Target::HrZone(zone) => {
            let (id, key) = ids::TARGET_HEART_RATE_ZONE;
            (target_type(id, key), None, None, Some(*zone as i64))
        }
        Target::Pace { fast_mps, slow_mps } => {
            let (id, key) = ids::TARGET_PACE_ZONE;
            (target_type(id, key), Some(*fast_mps), Some(*slow_mps), None)
        }
    }
}

fn target_type(id: i64, key: &str) -> TargetType {
    TargetType {
        workout_target_type_id: id,
        workout_target_type_key: key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workout::notation::parse;
    use serde_json::json;

    #[test]
    fn builds_example_one() {
        let items = parse("wu 10m @z2; 3x(int 4m @z4, rec 3m @z3); cd 10m @z1").unwrap();
        let workout = build_workout("10' Z2 / 3x(4' Z4, 3' Z3)", None, &items);
        let value = serde_json::to_value(&workout).unwrap();

        assert_eq!(
            value,
            json!({
                "workoutName": "10' Z2 / 3x(4' Z4, 3' Z3)",
                "description": null,
                "sportType": {"sportTypeId": 1, "sportTypeKey": "running"},
                "workoutSegments": [{
                    "segmentOrder": 1,
                    "sportType": {"sportTypeId": 1, "sportTypeKey": "running"},
                    "workoutSteps": [
                        {
                            "type": "ExecutableStepDTO",
                            "stepId": null,
                            "stepOrder": 1,
                            "childStepId": null,
                            "stepType": {"stepTypeId": 1, "stepTypeKey": "warmup"},
                            "description": null,
                            "stepAudioNote": null,
                            "endCondition": {"conditionTypeId": 2, "conditionTypeKey": "time"},
                            "endConditionValue": 600.0,
                            "endConditionCompare": null,
                            "endConditionZone": null,
                            "preferredEndConditionUnit": null,
                            "targetType": {"workoutTargetTypeId": 4, "workoutTargetTypeKey": "heart.rate.zone"},
                            "targetValueOne": null,
                            "targetValueTwo": null,
                            "targetValueUnit": null,
                            "zoneNumber": 2,
                            "secondaryTargetType": null,
                            "secondaryTargetValueOne": null,
                            "secondaryTargetValueTwo": null,
                            "secondaryTargetValueUnit": null,
                            "secondaryZoneNumber": null
                        },
                        {
                            "type": "RepeatGroupDTO",
                            "stepId": null,
                            "stepOrder": 2,
                            "childStepId": null,
                            "stepType": {"stepTypeId": 6, "stepTypeKey": "repeat"},
                            "numberOfIterations": 3,
                            "smartRepeat": false,
                            "endCondition": {"conditionTypeId": 7, "conditionTypeKey": "iterations"},
                            "endConditionValue": 3.0,
                            "preferredEndConditionUnit": null,
                            "skipLastRestStep": false,
                            "workoutSteps": [
                                {
                                    "type": "ExecutableStepDTO",
                                    "stepId": null,
                                    "stepOrder": 3,
                                    "childStepId": 1,
                                    "stepType": {"stepTypeId": 3, "stepTypeKey": "interval"},
                                    "description": null,
                                    "stepAudioNote": null,
                                    "endCondition": {"conditionTypeId": 2, "conditionTypeKey": "time"},
                                    "endConditionValue": 240.0,
                                    "endConditionCompare": null,
                                    "endConditionZone": null,
                                    "preferredEndConditionUnit": null,
                                    "targetType": {"workoutTargetTypeId": 4, "workoutTargetTypeKey": "heart.rate.zone"},
                                    "targetValueOne": null,
                                    "targetValueTwo": null,
                                    "targetValueUnit": null,
                                    "zoneNumber": 4,
                                    "secondaryTargetType": null,
                                    "secondaryTargetValueOne": null,
                                    "secondaryTargetValueTwo": null,
                                    "secondaryTargetValueUnit": null,
                                    "secondaryZoneNumber": null
                                },
                                {
                                    "type": "ExecutableStepDTO",
                                    "stepId": null,
                                    "stepOrder": 4,
                                    "childStepId": 1,
                                    "stepType": {"stepTypeId": 4, "stepTypeKey": "recovery"},
                                    "description": null,
                                    "stepAudioNote": null,
                                    "endCondition": {"conditionTypeId": 2, "conditionTypeKey": "time"},
                                    "endConditionValue": 180.0,
                                    "endConditionCompare": null,
                                    "endConditionZone": null,
                                    "preferredEndConditionUnit": null,
                                    "targetType": {"workoutTargetTypeId": 4, "workoutTargetTypeKey": "heart.rate.zone"},
                                    "targetValueOne": null,
                                    "targetValueTwo": null,
                                    "targetValueUnit": null,
                                    "zoneNumber": 3,
                                    "secondaryTargetType": null,
                                    "secondaryTargetValueOne": null,
                                    "secondaryTargetValueTwo": null,
                                    "secondaryTargetValueUnit": null,
                                    "secondaryZoneNumber": null
                                }
                            ]
                        },
                        {
                            "type": "ExecutableStepDTO",
                            "stepId": null,
                            "stepOrder": 5,
                            "childStepId": null,
                            "stepType": {"stepTypeId": 2, "stepTypeKey": "cooldown"},
                            "description": null,
                            "stepAudioNote": null,
                            "endCondition": {"conditionTypeId": 2, "conditionTypeKey": "time"},
                            "endConditionValue": 600.0,
                            "endConditionCompare": null,
                            "endConditionZone": null,
                            "preferredEndConditionUnit": null,
                            "targetType": {"workoutTargetTypeId": 4, "workoutTargetTypeKey": "heart.rate.zone"},
                            "targetValueOne": null,
                            "targetValueTwo": null,
                            "targetValueUnit": null,
                            "zoneNumber": 1,
                            "secondaryTargetType": null,
                            "secondaryTargetValueOne": null,
                            "secondaryTargetValueTwo": null,
                            "secondaryTargetValueUnit": null,
                            "secondaryZoneNumber": null
                        }
                    ]
                }]
            })
        );
    }

    #[test]
    fn builds_example_two_with_pace_and_description() {
        let items = parse("wu 2km; int 5km @4:30-4:40/km; cd 1km").unwrap();
        let workout = build_workout("Tempo", Some("Marathon block week 3"), &items);
        let value = serde_json::to_value(&workout).unwrap();

        assert_eq!(value["workoutName"], "Tempo");
        assert_eq!(value["description"], "Marathon block week 3");

        let steps = &value["workoutSegments"][0]["workoutSteps"];
        assert_eq!(steps[0]["stepOrder"], 1);
        assert_eq!(steps[0]["endCondition"]["conditionTypeKey"], "distance");
        assert_eq!(steps[0]["endConditionValue"], 2000.0);
        assert_eq!(
            steps[0]["preferredEndConditionUnit"],
            json!({"unitKey": "kilometer"})
        );
        assert_eq!(steps[0]["targetType"]["workoutTargetTypeKey"], "no.target");

        assert_eq!(steps[1]["stepOrder"], 2);
        assert_eq!(steps[1]["targetType"]["workoutTargetTypeKey"], "pace.zone");
        let fast = steps[1]["targetValueOne"].as_f64().unwrap();
        let slow = steps[1]["targetValueTwo"].as_f64().unwrap();
        assert!((fast - 1000.0 / 270.0).abs() < 1e-9);
        assert!((slow - 1000.0 / 280.0).abs() < 1e-9);
        assert!(fast > slow);

        assert_eq!(steps[2]["stepOrder"], 3);
    }

    #[test]
    fn two_repeat_groups_share_a_global_step_order_counter() {
        let items = parse("2x(int 1m); 2x(int 1m)").unwrap();
        let workout = build_workout("Two Blocks", None, &items);
        let value = serde_json::to_value(&workout).unwrap();
        let steps = &value["workoutSegments"][0]["workoutSteps"];

        assert_eq!(steps[0]["stepOrder"], 1);
        assert_eq!(steps[0]["workoutSteps"][0]["stepOrder"], 2);
        assert_eq!(steps[1]["stepOrder"], 3);
        assert_eq!(steps[1]["workoutSteps"][0]["stepOrder"], 4);
    }

    #[test]
    fn lap_step_nulls_end_condition_value_and_unit() {
        let items = parse("int lap").unwrap();
        let workout = build_workout("Lap Test", None, &items);
        let value = serde_json::to_value(&workout).unwrap();
        let step = &value["workoutSegments"][0]["workoutSteps"][0];

        assert_eq!(step["endCondition"]["conditionTypeKey"], "lap.button");
        assert!(step["endConditionValue"].is_null());
        assert!(step["preferredEndConditionUnit"].is_null());
    }
}
