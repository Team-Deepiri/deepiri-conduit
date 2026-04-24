# CodeQL Setup for deepiri-conduit

This folder contains the CodeQL configuration for security scanning in this repository.

## What each file does

- `.github/workflows/codeql.yml`
  - Defines when scans run and how GitHub Actions executes CodeQL.
- `.github/codeql/codeql-config.yml`
  - Defines what folders to include and ignore during analysis.

## CodeQL workflow breakdown (`.github/workflows/codeql.yml`)

### `name: CodeQL`
The display name in the Actions tab.

### `on.pull_request.branches` and `on.push.branches`
```yaml
on:
  pull_request:
    branches: [main, dev]
  push:
    branches: [main, dev]
```
Runs scans when PRs target `main` or `dev`, and when commits are pushed to `main` or `dev`.

### `permissions`
```yaml
permissions:
  actions: read
  contents: read
  security-events: write
```
Uses least-privilege permissions. `security-events: write` is required so CodeQL can upload findings.

### Language setup (current)
```yaml
with:
  languages: rust
```
This workflow currently runs analysis for Rust.

### Checkout step
```yaml
with:
  fetch-depth: 0
```
- `fetch-depth: 0` keeps full git history (safe default for analysis and troubleshooting).

### Initialize CodeQL
```yaml
uses: github/codeql-action/init@v3
with:
  config-file: ./.github/codeql/codeql-config.yml
```
Starts the CodeQL engine and loads `.github/codeql/codeql-config.yml`.

### Autobuild
```yaml
uses: github/codeql-action/autobuild@v3
```
Builds the project automatically for analysis before running queries.

### Analyze
```yaml
uses: github/codeql-action/analyze@v3
```
Executes queries and uploads results to GitHub Security.

## Config breakdown (`.github/codeql/codeql-config.yml`)

### `name`
The config name should identify this repository clearly, for example:

```yaml
name: deepiri-conduit-codeql
```

### `paths`
For this repository, the primary Rust source and test code should be in scope:

```yaml
paths:
  - src
  - tests
```

### `paths-ignore`
Generated/build/runtime artifact paths should be excluded to reduce noise and runtime:

```yaml
paths-ignore:
  - '**/node_modules/**'
  - '**/dist/**'
  - '**/build/**'
  - '**/coverage/**'
  - '**/logs/**'
  - '**/*.min.js'
  - '**/target/**'
  - '**/.cargo/**'
  - '**/.git/**'
```

## Best practices

1. Keep trigger scope intentional.
   Use branch filters (`main`, `dev`) to control cost and noise.
2. Keep language list explicit.
   Only include languages with meaningful source code.
3. Keep `paths` focused when used.
   Include actively maintained production code first.
4. Exclude generated/vendor artifacts.
   Keep caches, dependencies, build outputs, logs, and minified files in `paths-ignore`.
5. Pin to stable major action versions.
   `@v3` is the current stable major for CodeQL actions.
6. Review alerts regularly.
   Triage high/critical findings first and suppress only with documented reasoning.

## Maintenance examples
Keeping this updated as code and language coverage evolve is important. Here are common maintenance changes.

### Keep language scope aligned with this repository
This workflow currently analyzes Rust only:

```yaml
with:
  languages: rust
```

Only change this value when this repository adds production code in another supported language.

### Include only specific top-level packages
Add explicit `paths` only for directories that exist in this checkout.

Example:

```yaml
paths:
  - src
  - tests
```

### Exclude another generated folder
Add a glob to `paths-ignore`, for example:

```yaml
- '**/generated/**'
```
