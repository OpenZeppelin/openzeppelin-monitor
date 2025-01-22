# Contributing to OpenZeppelin Monitor

Thank you for your interest in contributing to the OpenZeppelin Monitor project! This document provides guidelines to ensure your contributions are effectively integrated into the project.

There are many ways to contribute, regardless of your experience level. Whether you're new to Rust or a seasoned expert, your help is invaluable. Every contribution matters, no matter how small, and all efforts are greatly appreciated. This document is here to guide you through the process. Don’t feel overwhelmed—it’s meant to support and simplify your contribution journey.

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Getting Started](#getting-started)
3. [How to Contribute](#how-to-contribute)
   - [Bug Reports](#bug-reports)
   - [Feature Requests](#feature-requests)
   - [Pull Requests](#pull-requests)
   - [Reviewing Pull Requests](#reviewing-pull-requests)
4. [Development Workflow](#development-workflow)
5. [Coding Standards](#coding-standards)
6. [Testing](#testing)
7. [Issue Labels](#issue-labels)
8. [Security](#security)
9. [Communication Channels](#communication-channels)
10. [License](#license)

---

## Code of Conduct

This project adheres to the [OpenZeppelin Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold this code.

## Getting Started

1. **Fork the Repository**:
   - Navigate to the [repository](https://github.com/openzeppelin/openzeppelin-monitor).
   - Click the "Fork" button to create your own copy of the repository.
2. **Clone Your Fork**:
   ```bash
   git clone https://github.com/<your-username>/openzeppelin-monitor.git
   cd openzeppelin-monitor
   ```
3. **Set Upstream Remote**:
   ```bash
   git remote add upstream https://github.com/openzeppelin/openzeppelin-monitor.git
   ```

## How to Contribute

### Bug Reports

If you encounter a bug, please:

1. Search the [issue tracker](https://github.com/openzeppelin/openzeppelin-monitor/issues) to see if it has already been reported.
2. If not, create a new issue and include:
   - A descriptive title.
   - Steps to reproduce the bug with as much detail as possible.
   - Expected behavior and actual behavior.
   - Relevant logs, configuration files, code snippets, or screenshots.

To help us diagnose the issue efficiently, ensure that your code snippets are minimal and focus solely on reproducing the bug. Avoid sharing entire projects when a small example suffices! For more guidance, refer to this [guide on creating a minimal, complete, and verifiable example](https://stackoverflow.com/help/mcve).

### Feature Requests

If you have an idea for a new feature:

1. Check the [issue tracker](https://github.com/openzeppelin/openzeppelin-monitor/issues) to see if a similar feature has been suggested.
2. Open a new issue and describe:
   - The problem the feature solves.
   - How it would be used.
   - Potential implementation details.

### Pull Requests

We welcome pull requests for bug fixes, new features, or documentation improvements. Follow these steps:

1. **Open an Issue**:
   - Discuss your planned changes by opening an issue before starting work.
2. **Branch Naming**:
   - Use a descriptive branch name (e.g., `fix-block-processing` or `feature-slack-notifications`).
   ```bash
   git checkout -b <branch-name>
   ```
3. **Commit Changes**:
   - Write clear and concise commit messages.
   - Follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

Keeping your changes logically grouped within individual commits is a best practice. Multiple commits in a single pull request are fine as long as they are meaningful. If you have "checkpoint" commits that don’t represent logical changes, consider squashing them.

4. **Push Your Changes**:
   ```bash
   git push origin <branch-name>
   ```
5. **Open a Pull Request**:
   - Provide a descriptive title and summary of your changes.
   - Link any relevant issues.

### Reviewing Pull Requests

Any member of the OpenZeppelin community is welcome to review pull requests.

Reviewers have a responsibility to provide helpful, constructive feedback. Avoid blocking a pull request without a clear explanation, and work collaboratively with contributors to improve the submission. All feedback should align with the [Code of Conduct](CODE_OF_CONDUCT.md).

#### Focus on Incremental Improvement

- Prioritize significant changes:
  - Does the change align with project goals?
  - Does it improve the codebase, even incrementally?
  - Are there clear bugs or large-scale issues?

Small imperfections can be addressed in follow-up pull requests. Ensure contributors feel encouraged, even if their pull request isn’t merged immediately.

#### Nitpicks and Minor Suggestions

Label small suggestions as non-blocking (e.g., **Nit: Change `foo()` to `bar()`. Not blocking.**). Address minor nits post-merge if appropriate.

#### Handling Stale Pull Requests

If a pull request stalls, reach out to the contributor. With their consent, you may take over the work while giving credit in the commit metadata.

---

## Development Workflow

1. **Set Up Development Environment**:
   - Install dependencies:
     ```bash
     cargo build
     ```
   - Set up environment variables:
     ```bash
     cp .env.example .env
     ```
2. **Run Tests**:
   - Unit tests:
     ```bash
     cargo test
     ```
   - Integration tests:
     ```bash
     cargo test integration
     ```
3. **Follow Git Hooks**:
   - Make hooks executable:
     ```bash
     chmod +x .githooks/*
     ```
   - Configure hooks:
     ```bash
     git config core.hooksPath .githooks
     ```

---

## Coding Standards

- Use **Rust 2021 edition**.
- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- Format code with `rustfmt`:
  ```bash
  rustup component add rustfmt --toolchain nightly
  cargo +nightly fmt
  ```
- Lint code with `clippy`:
  ```bash
  cargo clippy --all-targets --all-features
  ```

---

## Testing

All contributions must pass existing tests and include new tests when applicable:

1. Write tests for new features or bug fixes.
2. Run the test suite:
   ```bash
   cargo test
   ```
3. Ensure no warnings or errors.

---

## Issue Labels

We use the following labels to categorize issues:

- **Area Labels (`A-`)**:
  - `A-architecture`, `A-block-processing`, `A-notifications`, etc.
- **Type Labels (`T-`)**:
  - `T-bug`, `T-feature`, `T-documentation`, etc.
- **Priority Labels (`P-`)**:
  - `P-high`, `P-medium`, `P-low`.
- **Status Labels (`S-`)**:
  - `S-needs-triage`, `S-in-progress`, `S-blocked`, `S-needs-review`.
- **Difficulty Labels (`D-`)**:
  - `D-easy`, `D-medium`, `D-hard`.

---

## Security

For vulnerabilities or security concerns, refer to our [Security Policy](SECURITY.md).

---

## Communication Channels

TBD

---

## License

By contributing to this project, you agree that your contributions will be licensed under the [AGPL-3.0 License](LICENSE).
