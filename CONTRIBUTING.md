# Contributing to API Gateway

Thank you for your interest in contributing to the API Gateway project! This document provides guidelines and instructions for contributing.

## Code of Conduct

Be respectful, inclusive, and constructive in all interactions.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/gateway.git
   cd gateway
   ```
3. Add the upstream repository:
   ```bash
   git remote add upstream https://github.com/therealutkarshpriyadarshi/gateway.git
   ```
4. Create a new branch for your feature:
   ```bash
   git checkout -b feature/my-awesome-feature
   ```

## Development Setup

### Prerequisites

- Rust 1.70 or later (install via [rustup](https://rustup.rs))
- Git

### Building the Project

```bash
# Build in debug mode
cargo build

# Build in release mode
cargo build --release

# Run the gateway
cargo run
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests with coverage
cargo tarpaulin --out Html
```

## Development Workflow

1. **Make your changes**
   - Write clear, readable code
   - Follow Rust best practices
   - Add comments for complex logic

2. **Add tests**
   - Unit tests in the same file as the code
   - Integration tests in `tests/` directory
   - Aim for good test coverage

3. **Run tests and lints**
   ```bash
   # Run tests
   cargo test

   # Check formatting
   cargo fmt --all -- --check

   # Run clippy
   cargo clippy --all-targets --all-features -- -D warnings
   ```

4. **Format your code**
   ```bash
   cargo fmt
   ```

5. **Commit your changes**
   ```bash
   git add .
   git commit -m "Add feature: description of feature"
   ```

6. **Push to your fork**
   ```bash
   git push origin feature/my-awesome-feature
   ```

7. **Create a Pull Request**
   - Go to GitHub and create a PR from your fork
   - Provide a clear description of the changes
   - Reference any related issues

## Coding Standards

### Rust Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting (enforced in CI)
- Use `clippy` for linting (enforced in CI)
- Write idiomatic Rust code

### Code Organization

```rust
// Imports first
use std::collections::HashMap;
use crate::error::Result;

// Constants
const MAX_RETRIES: u32 = 3;

// Type definitions
pub struct MyStruct {
    field: String,
}

// Implementations
impl MyStruct {
    pub fn new(field: String) -> Self {
        Self { field }
    }
}

// Tests at the end
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // test code
    }
}
```

### Documentation

- Add doc comments for public APIs:
  ```rust
  /// This function does something important
  ///
  /// # Arguments
  ///
  /// * `param` - Description of parameter
  ///
  /// # Returns
  ///
  /// Description of return value
  pub fn my_function(param: String) -> Result<()> {
      // implementation
  }
  ```

- Update README.md if adding new features
- Add examples in `examples/` if appropriate

### Error Handling

- Use the `Result` type for operations that can fail
- Create specific error types in `error/mod.rs`
- Provide helpful error messages

### Testing

- Write unit tests for individual functions
- Write integration tests for end-to-end flows
- Use descriptive test names: `test_router_matches_path_parameters`
- Test error cases, not just happy paths

## Pull Request Guidelines

### PR Title

Use clear, descriptive titles:
- `feat: Add rate limiting support`
- `fix: Correct path parameter matching`
- `docs: Update configuration examples`
- `test: Add integration tests for proxy handler`
- `refactor: Simplify router implementation`

### PR Description

Include:
- What changes were made
- Why the changes were made
- Any breaking changes
- How to test the changes
- Related issue numbers (if applicable)

Example:
```markdown
## Description
Adds support for wildcard path matching in the router.

## Changes
- Updated router to support `*` wildcards
- Added tests for wildcard matching
- Updated documentation

## Testing
- Unit tests added
- Manual testing with example configs

Closes #123
```

### Review Process

1. Automated CI checks must pass
2. At least one maintainer review required
3. Address review feedback
4. Maintainer will merge when approved

## Project Structure

```
gateway/
├── src/
│   ├── config/      # Configuration loading
│   ├── error/       # Error types
│   ├── router/      # Routing logic
│   ├── proxy/       # Proxy handler
│   └── lib.rs       # Library entry
├── tests/           # Integration tests
├── examples/        # Example configs
└── .github/         # CI/CD workflows
```

## Areas to Contribute

### Current Phase (Phase 1)
- Documentation improvements
- Additional tests
- Bug fixes
- Performance optimizations

### Future Phases
- Authentication & Authorization (Phase 2)
- Rate Limiting (Phase 3)
- Circuit Breaking (Phase 4)
- Load Balancing (Phase 5)
- Observability (Phase 6)

See [ROADMAP.md](ROADMAP.md) for detailed plans.

## Questions?

- Open an issue for bugs or feature requests
- Start a discussion for questions or ideas
- Reach out to maintainers

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

Thank you for contributing to API Gateway!
