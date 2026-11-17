# RackDesk development commands. Run `make` or `make help` to list them.

.DEFAULT_GOAL := help
.PHONY: help setup run web build check fmt fmt-check clippy test web-check e2e deny new-password clean

DATA_DIR ?= ./dev-data

help: ## List the commands
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F ':.*## ' '{printf "  %-14s %s\n", $$1, $$2}'

setup: ## Install the web UI packages
	cd web && npm ci

run: ## Start the server on 127.0.0.1:7171
	cargo run -p rackdeskd -- --data-dir $(DATA_DIR)

web: ## Start the UI dev server on :5173 (needs `make run` in another terminal)
	cd web && npm run dev

build: ## Build the release binary with the UI inside it
	cd web && npm run build
	cargo build --release -p rackdeskd

check: fmt-check clippy test web-check ## Run the fast checks that CI runs. `make e2e` and `make deny` are separate

fmt: ## Format the Rust code
	cargo fmt --all

fmt-check: ## Fail if the Rust code is not formatted
	cargo fmt --all --check

clippy: ## Run the Rust linter, with warnings as errors
	cargo clippy --workspace --all-targets -- -D warnings

test: ## Run the Rust tests and fail if the generated TypeScript types are stale
	cargo test --workspace
	git diff --exit-code web/src/api/generated

web-check: ## Lint and build the UI
	cd web && npm run lint && npm run build

e2e: ## Run the browser tests against a temporary server (downloads a test browser on first use)
	cd web && npm run build
	cargo build -p rackdeskd
	cd web && npx playwright install chromium && npm run e2e

deny: ## Check dependencies for advisories and licences (needs cargo-deny)
	cargo deny check

new-password: ## Delete the dev account, so the next `make run` prints a new password
	rm -rf ./dev-data

clean: ## Delete build output
	cargo clean
	rm -rf web/dist
