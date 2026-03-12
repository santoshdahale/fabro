# Contributing to Fabro

Thanks for your interest in contributing to Fabro!

## Getting started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (for the web frontend)
- Git

### Build and test

```bash
# Build all Rust crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Check formatting and lint
cargo fmt --check --all
cargo clippy --workspace -- -D warnings
```

### Web frontend (fabro-web)

```bash
cd apps/fabro-web
bun install
bun run dev        # start dev server
bun test           # run tests
bun run typecheck  # type check
```

## Development workflow

1. Fork the repository and create a branch from `main`
2. Make your changes
3. Ensure `cargo test --workspace`, `cargo fmt --check --all`, and `cargo clippy --workspace -- -D warnings` pass
4. Open a pull request

## Reporting bugs

Open an issue on [GitHub Issues](https://github.com/brynary/arc/issues) with steps to reproduce the problem.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE.md).
