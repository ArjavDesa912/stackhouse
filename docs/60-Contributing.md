# 60 - Contributing

## 🤝 Contributing to Stackhouse

Thank you for your interest in contributing!

### Getting Started

See [`CONTRIBUTING.md`](../CONTRIBUTING.md) at the repo root for the canonical, up-to-date contribution guide (dev setup, code style, tests, PR process). The steps below are a quick summary.

```bash
# Fork the repository, then:
git clone https://github.com/YOUR_USERNAME/stackhouse.git
cd stackhouse

# Add upstream remote
git remote add upstream https://github.com/ArjavDesa912/stackhouse.git

# Create feature branch
git checkout -b feature/your-feature-name
```

### Development Setup

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build Stackhouse
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

### Code Style

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Run checks
cargo check --all-features
```

### Making Changes

1. **Create a branch** for each feature
2. **Write tests** for new functionality
3. **Update documentation** if needed
4. **Commit with clear messages**

### Pull Request Process

```bash
# Push to your fork
git push origin feature/your-feature-name

# Create Pull Request on GitHub
# Include:
# - Description of changes
# - Related issues
# - Testing done
```

### Contribution Areas

We welcome contributions in:

- 🐛 Bug fixes
- ✨ New features
- 📚 Documentation
- 🧪 Tests
- ⚡ Performance improvements
- 🌐 Internationalization

### Code Review Process

- All PRs require review
- At least one approval needed
- CI must pass
- Tests must pass

### Getting Help

- 🐙 [GitHub Issues](https://github.com/ArjavDesa912/stackhouse/issues) — bugs and feature requests
- 💬 [GitHub Discussions](https://github.com/ArjavDesa912/stackhouse/discussions) — questions and ideas
- 📖 Docs: See [Documentation Index](./DOCS_INDEX.md)

---

**Ready to contribute?** Start coding! 🚀
