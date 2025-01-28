#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

# Base directories
BUILD_DIR="build/site"
RUST_DOCS_DIR="modules/ROOT/pages/rust_docs"

# Ensure we're running from `repo_root/docs`
if [ "$(basename "$PWD")" != "docs" ]; then
  echo "Error: You must run this script from the 'docs' directory."
  exit 1
fi

# Find the module and version directories
MODULE_DIR=$(find "$BUILD_DIR" -mindepth 1 -maxdepth 1 -type d | head -n 1)
if [ -z "$MODULE_DIR" ]; then
  echo "Error: No module directory found in '$BUILD_DIR'."
  exit 1
fi

VERSION_DIR=$(find "$MODULE_DIR" -mindepth 1 -maxdepth 1 -type d | head -n 1)
if [ -z "$VERSION_DIR" ]; then
  echo "Error: No version directory found in '$MODULE_DIR'."
  exit 1
fi

# Log the directories found
echo "Module directory: $MODULE_DIR"
echo "Version directory: $VERSION_DIR"

# Define the destination directory
DEST_DIR="$VERSION_DIR/rust_docs"

# Create the destination directory if it doesn't exist
mkdir -p "$DEST_DIR"

# Copy the rust_docs directory to the destination
echo "Copying '$RUST_DOCS_DIR' to '$DEST_DIR'..."


cp -r "$RUST_DOCS_DIR/"* "$DEST_DIR/"

# Success message
echo "Rust docs successfully copied to '$DEST_DIR'."
