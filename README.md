# Requirements Manager

A professional requirements management system with both CLI and GUI interfaces, written in Rust.

## Project Structure

This is a Cargo workspace with three crates:

- **requirements-core**: Shared library containing all core functionality (models, storage, business logic)
- **requirements-cli**: Command-line interface (`req` binary)
- **requirements-gui**: Graphical interface using egui (`req-gui` binary)

## Building

```bash
# Build everything
cargo build --workspace

# Build release versions
cargo build --workspace --release
```

## Binaries

After building, you'll find two binaries in `target/debug/` (or `target/release/`):

- `req`: CLI tool
- `req-gui`: GUI application

## CLI Usage

```bash
# List requirements
./target/debug/req list

# Add a requirement
./target/debug/req add --title "New Feature" --description "Description" --priority High

# Show requirement details
./target/debug/req show SPEC-001

# Add relationship
./target/debug/req rel add --from SPEC-001 --to SPEC-002 --type parent -b

# See all commands
./target/debug/req --help
```

## GUI Usage

```bash
# Launch the GUI
./target/debug/req-gui
```

The GUI provides:
- Requirements list with filtering
- Detail view for each requirement
- Relationship visualization
- Search functionality

## Features

- **Dual Interface**: Both CLI and GUI use the same core library
- **SPEC-ID System**: Human-friendly IDs (SPEC-001, SPEC-002, etc.) alongside UUIDs
- **Relationships**: Define parent/child, verifies, references, and custom relationships
- **Flexible Storage**: YAML-based, human-readable format
- **Multi-Project Support**: Manage multiple requirement sets via registry
- **Feature Organization**: Group requirements by numbered features

## Development

```bash
# Run tests
cargo test --workspace

# Run CLI directly (development)
cargo run -p requirements-cli -- list

# Run GUI directly (development)
cargo run -p requirements-gui
```

## Architecture

- Pure Rust implementation
- Shared core library ensures CLI and GUI stay in sync
- egui for modern, cross-platform GUI
- Workspace structure for clean separation of concerns
