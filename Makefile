# Burrow: one-command build and the dev loop. See README.md.
CARGO ?= $(shell command -v cargo 2>/dev/null || echo $(HOME)/.cargo/bin/cargo)
export PATH := $(dir $(CARGO)):$(PATH)

APP_RELEASE := target/release/bundle/macos/Burrow.app
APP_E2E := target/debug/bundle/macos/Burrow.app

.PHONY: build dev test e2e install icons clean

## Build the release app bundle from a clean checkout -> target/release/bundle/macos/Burrow.app
build:
	npm ci
	npx tauri build --bundles app
	@echo "\nBuilt $(APP_RELEASE)"

## Hot-reloading dev loop (Vite + cargo)
dev:
	npm install
	npx tauri dev

## Unit and integration tests (Rust core + frontend helpers + type check)
test:
	$(CARGO) test --workspace
	npm test
	npm run check

## End-to-end check: real app (debug build with the e2e control server) + a real quick tunnel
e2e:
	npm install
	npx tauri build --debug --bundles app --features e2e
	node e2e/run.mjs $(APP_E2E)

## Install the release build to ~/Applications (required for launch at login)
install: build
	rm -rf $(HOME)/Applications/Burrow.app
	mkdir -p $(HOME)/Applications
	cp -R $(APP_RELEASE) $(HOME)/Applications/Burrow.app
	@echo "Installed to ~/Applications/Burrow.app"

## Redraw the tray glyph and app icon
icons:
	swift scripts/make-icons.swift src-tauri/icons
	npx tauri icon src-tauri/icons/app-icon.png -o src-tauri/icons
	rm -rf src-tauri/icons/android src-tauri/icons/ios src-tauri/icons/Square*.png src-tauri/icons/StoreLogo.png

clean:
	rm -rf dist target node_modules
