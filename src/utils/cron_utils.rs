//! Utility functions for working with cron schedules and time intervals
//!
//! This module provides helper functions for parsing and analyzing cron expressions,

use chrono::Utc;
use cron::Schedule;

/// Calculates the time interval between two consecutive occurrences of a cron schedule
///
/// This function takes a cron expression and determines how many milliseconds will elapse
/// between two consecutive runs of the schedule.
///
/// # Arguments
///
/// * `cron_schedule` - A string slice containing a valid cron expression (e.g., "0 0 * * *")
///
/// # Returns
///
/// * `Some(i64)` - The number of milliseconds between consecutive schedule runs
/// * `None` - If the cron expression is invalid or if two consecutive occurrences cannot be
///   determined
pub fn get_cron_interval_ms(cron_schedule: &str) -> Option<i64> {
	// Parse the cron schedule
	let schedule = match cron_schedule.parse::<Schedule>() {
		Ok(schedule) => schedule,
		Err(_) => return None, // Return None if the cron string is invalid
	};

	// Get the current time
	let now = Utc::now();

	// Get the next two occurrences of the schedule
	let mut occurrences = schedule.after(&now).take(2);

	if let (Some(first), Some(second)) = (occurrences.next(), occurrences.next()) {
		// Calculate the interval in milliseconds
		let interval_ms = (second - first).num_milliseconds();
		Some(interval_ms)
	} else {
		None // Return None if we cannot find two occurrences
	}
}
