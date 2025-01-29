#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

# Base directories
NAME=$(yq '.name' antora.yml)
VERSION=$(yq '.version' antora.yml)
BUILD_DIR="build/site"
RUST_DOCS_DIR="modules/ROOT/pages/rust_docs"

# Ensure we're running from `repo_root/docs`
if [ "$(basename "$PWD")" != "docs" ]; then
  echo "Error: You must run this script from the 'docs' directory."
  exit 1
fi

# Find the target directory
TARGET_DIR="$BUILD_DIR/$NAME/$VERSION"
if [ ! -d "$TARGET_DIR" ]; then
  echo "Error: Target directory '$TARGET_DIR' not found."
  exit 1
fi

# Log the directories found
echo "Target directory: $TARGET_DIR"

# Define the destination directory
DEST_DIR="$TARGET_DIR/rust_docs"

# Create the destination directory if it doesn't exist
mkdir -p "$DEST_DIR"

# Check if the source directory exists and is not empty
if [ -d "$RUST_DOCS_DIR" ] && [ "$(ls -A "$RUST_DOCS_DIR")" ]; then
  echo "Copying '$RUST_DOCS_DIR' to '$DEST_DIR'..."
  cp -r "$RUST_DOCS_DIR/"* "$DEST_DIR/"
  echo "Rust docs successfully copied to '$DEST_DIR'."
else
  echo "Source directory '$RUST_DOCS_DIR' does not exist or is empty."
fi
