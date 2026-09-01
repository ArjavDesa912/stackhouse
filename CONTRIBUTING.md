# Contributing to Stackhouse

Thanks for your interest in contributing — this is a single-maintainer project today, so clear, well-scoped PRs are the fastest way to get something merged.

## Getting Started

```bash
# Fork the repo on GitHub, then:
git clone https://github.com/YOUR_USERNAME/stackhouse.git
cd stackhouse
git remote add upstream https://github.com/ArjavDesa912/stackhouse.git
git checkout -b feature/your-feature-name
```

## Development Setup

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Postgres (via Docker)
docker compose up -d postgres

# Build and run the server
cp .env.example .env
cargo run --release -- serve

# UI (optional, only needed for frontend work)
cd ui && npm install && npm run dev
```

## Code Style

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo check --all-features
```

For the UI: `npm run lint` (from `ui/`).

## Tests

```bash
cargo test                    # Rust integration + unit tests
STACKHOUSE_TEST_URL=postgres://postgres:password@localhost:5432/stackhouse_test cargo test
```

New functionality should come with tests. Bug fixes should include a regression test where practical.

## Making Changes

1. One focused change per branch/PR — smaller PRs review faster.
2. Update the relevant doc under `docs/` if behavior changes.
3. Run `cargo fmt`, `cargo clippy`, and `cargo test` before opening the PR.
4. Write clear commit messages describing *why*, not just *what*.

## Pull Request Process

Open a PR against `main` and fill in the PR template. CI (build, clippy, fmt check, tests) must pass. At least one maintainer review is required before merge.

## Where to Contribute

- **Docs** (`docs/`) — corrections and clarifications are always welcome, no issue required.
- **Tests** (`tests/`) — coverage gaps are good first issues.
- **Bug fixes** — see issues labeled `good first issue` / `help wanted`.

For anything larger (new subsystem, breaking API change), please open an issue first to discuss the approach before writing code.

## Getting Help

Open a [GitHub Issue](https://github.com/ArjavDesa912/stackhouse/issues) or start a [Discussion](https://github.com/ArjavDesa912/stackhouse/discussions).
