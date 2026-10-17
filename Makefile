# Underlane: one-command build and the dev loop. See README.md.
CARGO ?= $(shell command -v cargo 2>/dev/null || echo $(HOME)/.cargo/bin/cargo)
export PATH := $(dir $(CARGO)):$(PATH)

APP_RELEASE := target/release/bundle/macos/Underlane.app
APP_E2E := target/debug/bundle/macos/Underlane.app

.PHONY: build dev test e2e install icons clean security hooks

## Build the release app bundle from a clean checkout -> target/release/bundle/macos/Underlane.app
build:
	pnpm install --frozen-lockfile
	pnpm tauri build --bundles app
	@echo "\nBuilt $(APP_RELEASE)"

## Hot-reloading dev loop (Vite + cargo)
dev:
	pnpm install
	pnpm tauri dev

## Unit and integration tests (Rust core + frontend helpers + type check)
test:
	$(CARGO) test --workspace
	pnpm test
	pnpm run check

## Scan every local branch and tag for secrets (brew install gitleaks)
security:
	gitleaks git --log-opts="--all --full-history -m" --redact=100 --no-banner

## Enable this checkout's staged-file and pre-push secret checks
hooks:
	@command -v gitleaks >/dev/null || { echo "Install Gitleaks first: brew install gitleaks"; exit 1; }
	git config --local core.hooksPath .githooks

## End-to-end check: real app (debug build with the e2e control server) + a real quick tunnel
e2e:
	pnpm install
	pnpm tauri build --debug --bundles app --features e2e
	node e2e/run.mjs $(APP_E2E)

## Install the release build to ~/Applications (required for launch at login)
install: build
	rm -rf $(HOME)/Applications/Underlane.app
	mkdir -p $(HOME)/Applications
	cp -R $(APP_RELEASE) $(HOME)/Applications/Underlane.app
	@echo "Installed to ~/Applications/Underlane.app"

## Redraw the tray glyph and app icon
icons:
	swift scripts/make-icons.swift apps/desktop/src-tauri/icons
	pnpm tauri icon src-tauri/icons/app-icon.png -o src-tauri/icons
	rm -rf apps/desktop/src-tauri/icons/android apps/desktop/src-tauri/icons/ios apps/desktop/src-tauri/icons/Square*.png apps/desktop/src-tauri/icons/StoreLogo.png

clean:
	rm -rf apps/desktop/dist target node_modules apps/desktop/node_modules
