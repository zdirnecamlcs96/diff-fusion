# Contributing to diff-fusion

Thank you for your interest in contributing to diff-fusion! This document outlines our coding principles, architectural guidelines, and development practices.

> The Rust crate lives in `src/` — file paths and `cargo` commands in this
> document are relative to that directory. The TypeScript SDK is `sdk/typescript/`,
> the Go SDK `sdk/golang/`; the cross-language contract lives in `spec/`.

## 🎯 Project Mission

**Core Focus**: A unified JSON transformation and conflict detection library that solves the n(n-1)/2 integration problem using Common Intermediate Format (CIF).

**What We Are**:

- A pure Rust library with clean, focused logic
- Schema-driven JSON transformation engine
- Smart conflict detection system
- Hub-and-spoke integration pattern implementation

**What We Are NOT**:

- A multi-language FFI framework (that's a separate concern)
- A REST API server
- A UI/frontend tool
- A database synchronization tool

## 🏗️ Architectural Principles

### Clean Architecture

We follow Clean Architecture principles with clear separation of concerns:

```
┌─────────────────────────────────────────┐
│           CLI Layer (main.rs)            │  ← Entry points
├─────────────────────────────────────────┤
│         API Layer (lib.rs)               │  ← Public API
├─────────────────────────────────────────┤
│   Core Logic (transform.rs, compare.rs) │  ← Business logic
├─────────────────────────────────────────┤
│      Data Structures (serde_json)        │  ← Data layer
└─────────────────────────────────────────┘
```

**Layer Rules**:

- **Inner layers** don't know about outer layers
- **Dependencies point inward** (Dependency Inversion Principle)
- **Core logic** has no external dependencies beyond serde_json
- **API layer** exposes stable interfaces
- **CLI layer** handles user interaction only

### SOLID Principles

#### 1. Single Responsibility Principle (SRP)

Each module has ONE reason to change:

- `transform.rs` - JSON transformation logic only
- `compare.rs` - Conflict detection logic only
- `cli.rs` - Command-line argument parsing only
- `main.rs` - CLI orchestration only
- `lib.rs` - Public API surface only

**Example** ✅:

```rust
// Good: transform.rs focuses on transformation
pub fn transform_to_cif(
    source: &Value,
    schema: &Value,
    format_id: &str,
) -> Result<Value, Box<dyn Error>>
```

**Anti-pattern** ❌:

```rust
// Bad: mixing transformation with I/O
pub fn transform_and_save_to_file(
    source: &Value,
    schema: &Value,
    output_path: &str,
) -> Result<(), Box<dyn Error>>
```

#### 2. Open/Closed Principle (OCP)

Open for extension, closed for modification:

- **Schema-driven design** allows new formats without code changes
- **Type system** prevents breaking changes
- **Result types** make errors explicit

**Example** ✅:

```rust
// Add new format by updating schema.json, not code
{
  "transformations": {
    "new_format": {  // ← Extension point
      "field": {"source_path": "path", "type": "string"}
    }
  }
}
```

#### 3. Liskov Substitution Principle (LSP)

Subtypes must be substitutable:

- **Trait implementations** honor contracts
- **Error types** are consistent
- **JSON values** are interchangeable

#### 4. Interface Segregation Principle (ISP)

Clients shouldn't depend on unused interfaces:

- **Minimal public API**: Only `transform_to_cif()` and `compare_json()`
- **No fat traits**: Functions over traits when possible
- **Clear separation**: String API vs Value API

**Example** ✅:

```rust
// Two focused functions instead of one bloated trait
pub fn transform_to_cif(source: &Value, ...) -> Result<Value, ...>
pub fn compare_json(a: &Value, b: &Value) -> Vec<(String, (Value, Value))>
```

#### 5. Dependency Inversion Principle (DIP)

Depend on abstractions, not concretions:

- **Core logic** depends on `serde_json::Value` (abstraction)
- **No database coupling**: We work with JSON, not SQL
- **No HTTP coupling**: We process data, not requests

## 📝 Code Style Guidelines

### Rust Best Practices

1. **Use the Standard Library**

   ```rust
   // Good: Use std types
   use std::collections::HashMap;

   // Avoid: Custom implementations when std works
   ```

2. **Prefer Owned Types in Public APIs**

   ```rust
   // Good: Clear ownership
   pub fn transform_to_cif(source: &Value, ...) -> Result<Value, String>

   // Avoid: Complex lifetimes in public API
   pub fn transform<'a>(source: &'a Value) -> Result<&'a Value, String>
   ```

3. **Use Result for Errors**

   ```rust
   // Good: Explicit error handling
   pub fn transform(...) -> Result<Value, Box<dyn Error>>

   // Bad: Panicking in library code
   pub fn transform(...) -> Value // panics on error ❌
   ```

4. **Document Public APIs**

   ```rust
   /// Transform JSON to Common Intermediate Format (CIF)
   ///
   /// # Arguments
   /// * `source` - JSON value to transform
   /// * `schema` - Schema definition with transformation rules
   /// * `format_id` - Format identifier (e.g., "format_a")
   ///
   /// # Returns
   /// Transformed JSON in CIF format
   ///
   /// # Errors
   /// Returns error if format_id not found or required field missing
   pub fn transform_to_cif(...)
   ```

### Module Organization

```
src/
├── lib.rs          // Public API, re-exports, types
├── main.rs         // CLI entry point
├── cli.rs          // CLI argument parsing
├── transform.rs    // Transformation logic
└── compare.rs      // Comparison logic
```

**Rules**:

- Keep files under 200 lines when possible
- One primary responsibility per file
- Internal helpers stay private
- Public functions go at the top

### Naming Conventions

```rust
// Functions: verb_noun in snake_case
pub fn transform_to_cif(...)
pub fn compare_json(...)
pub fn extract_value_by_path(...)

// Types: PascalCase
pub struct ConflictReport { ... }
pub struct Conflict { ... }

// Constants: SCREAMING_SNAKE_CASE
const MAX_DEPTH: usize = 100;

// Modules: snake_case
pub mod transform;
pub mod compare;
```

## 🧪 Testing Guidelines

### Test Coverage Requirements

Every public function must have:

1. **Happy path test** - Normal operation
2. **Error case tests** - Invalid inputs
3. **Edge case tests** - Boundary conditions

### Test Structure

```rust
#[test]
fn test_transform_to_cif_basic() {
    // Arrange
    let source = json!({"name": "Widget"});
    let schema = json!({...});

    // Act
    let result = transform_to_cif(&source, &schema, "format_a");

    // Assert
    assert!(result.is_ok());
    let cif = result.unwrap();
    assert_eq!(cif["product_name"], "Widget");
}
```

### Test Location

- **Unit tests**: In `tests/` directory
- **Integration tests**: In `tests/` directory
- **Example code**: In `examples/` directory

## 🚫 Anti-Patterns to Avoid

### 1. God Objects

```rust
// Bad: One struct doing everything
struct DiffFusion {
    pub fn transform(...) { }
    pub fn compare(...) { }
    pub fn save_to_db(...) { }  // ❌
    pub fn send_email(...) { }  // ❌
}

// Good: Focused functions
pub fn transform_to_cif(...) { }
pub fn compare_json(...) { }
```

### 2. Hidden Dependencies

```rust
// Bad: Global state
static mut CONFIG: Option<Config> = None;

// Good: Explicit parameters
pub fn transform(source: &Value, schema: &Value, ...) { }
```

### 3. Mixing Concerns

```rust
// Bad: Business logic + I/O
pub fn transform_and_print(source: &Value) {
    let result = transform(source);
    println!("{}", result);  // ❌
}

// Good: Separate concerns
pub fn transform(source: &Value) -> Result<Value, Error> { }
// Caller handles printing
```

### 4. Premature Optimization

```rust
// Bad: Complex optimization without profiling
pub fn compare_json_fast(...) {
    // 100 lines of unsafe pointer arithmetic ❌
}

// Good: Clear, correct code first
pub fn compare_json(...) {
    // Simple, readable implementation
}
```

## 🔄 Development Workflow

### Before Submitting

1. **Run tests**: `cargo test` — must be **0 failed, 0 ignored**. An ignored
   test is dead code; either fix it or delete it. `rust,ignore` doctests
   count too — make them runnable (use `# ` to hide setup lines) or drop
   the fence.
2. **Check formatting**: `cargo fmt`
3. **Run linter**: `cargo clippy --all-targets` — must be **0 warnings**.
   If a warning is a false positive, silence it locally with
   `#[allow(clippy::lint_name)]` and a one-line WHY comment.
4. **Build release**: `cargo build --release`
5. **Update docs**: Document any API changes

### Commit Messages

Follow conventional commits:

```
feat: add nested path support in transformations
fix: handle numeric equality (5 == 5.0)
docs: update README with schema examples
refactor: split transform logic into modules
test: add edge cases for type conversion
```

### Pull Request Guidelines

1. **Focus on one thing**: One feature/fix per PR
2. **Write tests**: All new code must be tested
3. **Update docs**: Keep README and doc comments current
4. **Follow style**: Run `cargo fmt` and `cargo clippy`
5. **Link issues**: Reference related issue numbers

## 🎓 Learning Resources

### Rust

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

### Architecture

- [Clean Architecture (Robert C. Martin)](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html)
- [SOLID Principles](https://en.wikipedia.org/wiki/SOLID)
- [Domain-Driven Design](https://martinfowler.com/bliki/DomainDrivenDesign.html)

## 📋 Checklist for New Features

- [ ] Does it align with core mission? (CIF transformation & conflict detection)
- [ ] Does it follow SOLID principles?
- [ ] Is it in the right layer? (Core logic vs API vs CLI)
- [ ] Does it have comprehensive tests?
- [ ] Is the public API documented?
- [ ] Does it handle errors properly?
- [ ] Is it backwards compatible?
- [ ] Does `cargo clippy --all-targets` pass with **0 warnings**?
- [ ] Does `cargo fmt` pass?
- [ ] Does `cargo test` pass with **0 failed, 0 ignored**?

## 💡 Design Decisions

### Why Schema-Driven?

- **Open/Closed Principle**: Add formats without code changes
- **Separation of Concerns**: Data definition separate from logic
- **Testability**: Easy to test with different schemas

### Why Pure Functions?

- **Predictability**: Same input = same output
- **Testability**: No hidden state to mock
- **Composability**: Easy to combine operations

### Why No Database Coupling?

- **Single Responsibility**: We transform data, not store it
- **Reusability**: Works with any storage backend
- **Simplicity**: Fewer dependencies, easier to test

## 🤝 Questions?

If you have questions about these guidelines or need clarification on design decisions, please:

1. Check existing issues and discussions
2. Read the [ARCHITECTURE.md](./ARCHITECTURE.md) document
3. Open a discussion on GitHub
4. Ask in pull request comments

Thank you for contributing to diff-fusion! 🚀
