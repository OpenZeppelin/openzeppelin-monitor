//! Test helper utilities for creating files

use std::{
	os::unix::fs::PermissionsExt,
	path::{Path, PathBuf},
};

/// Creates a file named `name` in `dir` with the given permission bits and returns its path.
/// The file is only meant to be inspected, never executed.
pub(crate) fn create_file_with_mode(dir: &Path, name: &str, mode: u32) -> PathBuf {
	let file_path = dir.join(name);
	std::fs::write(&file_path, "").unwrap();
	std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(mode)).unwrap();
	file_path
}
