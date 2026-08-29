# Contributing to AIverse Contracts

We welcome contributions to the AIverse Contracts project! Please follow the guidelines below to ensure a smooth collaboration process.

## Development Workflow

1.  **Fork and Clone**: Fork the repository and clone it locally.
2.  **Create a Branch**: Create a feature branch for your changes (`git checkout -b feature/my-feature`).
3.  **Make Changes**: Implement your changes, ensuring code quality and safety.
4.  **Test**: Run tests locally (if available) and verify your changes.
5.  **Format and Lint**: Ensure your code is formatted and passes clippy checks.
    ```bash
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
    ```
6.  **Build**: Verify that contracts build for Wasm.
    ```bash
    cargo build --release --target wasm32-unknown-unknown
    ```
7.  **Commit and Push**: Commit your changes with clear messages and push to your fork.
8.  **Pull Request**: Submit a Pull Request (PR) to the `main` branch.

## CI Configuration

We use GitHub Actions to enforce code quality and security. The CI pipeline runs automatically on push to `main` and on pull requests. It performs the following checks:

-   **Formatting**: Checks if code is formatted using `cargo fmt --check`.
-   **Linting**: Runs `cargo clippy` with warnings treated as errors to catch common mistakes and improve code quality.
-   **Security Audit**: Runs `cargo audit` to check for security vulnerabilities in dependencies.
-   **Build**: Compiles the contracts for the `wasm32-unknown-unknown` target in release mode to ensure they are buildable.

If any of these checks fail, the build will fail, and you will need to address the issues before your PR can be merged.

## Reporting Issues

If you find a bug or have a suggestion, please open an issue in the repository. Provide as much detail as possible.
