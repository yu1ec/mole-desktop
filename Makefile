# Mole Desktop — build automation
APP_NAME     := Mole Desktop
APP_DIR      := src-tauri
BIN_NAME     := mole-desktop
TARGET_DIR   := $(APP_DIR)/target
CARGO        ?= cargo
NPM          ?= npm

# OS: macos | linux | windows
# ARCH: arm64 | x86_64 | universal (macos only)
HOST_OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
ifeq ($(HOST_OS),darwin)
  HOST_OS := macos
endif
ifneq ($(findstring mingw,$(HOST_OS)),)
  HOST_OS := windows
endif
ifneq ($(findstring msys,$(HOST_OS)),)
  HOST_OS := windows
endif

HOST_ARCH_RAW := $(shell uname -m)
ifeq ($(HOST_ARCH_RAW),arm64)
  HOST_ARCH := arm64
else ifeq ($(HOST_ARCH_RAW),aarch64)
  HOST_ARCH := arm64
else ifeq ($(HOST_ARCH_RAW),x86_64)
  HOST_ARCH := x86_64
else ifeq ($(HOST_ARCH_RAW),amd64)
  HOST_ARCH := x86_64
else
  HOST_ARCH := $(HOST_ARCH_RAW)
endif

OS   ?= $(HOST_OS)
ARCH ?= $(HOST_ARCH)

# SIGN=0 (default) passes --no-sign to tauri build
SIGN ?= 0
# SIGN=1 时使用的签名身份（证书 CN），留空则由 tauri 自动选择
SIGN_IDENTITY ?=

# INIT_TARGETS=all adds all common rustup targets during init
INIT_TARGETS ?= current

.PHONY: help init init-rust init-tauri init-frontend icons build-cli build-frontend build-app dev-app run-cli release notes bump-version check-version clean

.DEFAULT_GOAL := help

help:
	@echo "$(APP_NAME) — Makefile targets"
	@echo ""
	@echo "Targets:"
	@echo "  init        Fetch deps, install tauri-cli, npm packages, add rustup targets"
	@echo "  icons       Regenerate app/installer icons from $(APP_DIR)/icons/app-icon.png"
	@echo "  build-cli   Build desktop binary without installers ($(BIN_NAME))"
	@echo "  build-frontend  Build React UI to dist/"
	@echo "  build-app   Build desktop app ($(APP_NAME)) installers"
	@echo "  dev-app     Run Tauri dev server (hot reload)"
	@echo "  run-cli     Build (if needed) and run the desktop binary"
	@echo "  release       Auto bump version, generate release notes, print git tag commands"
	@echo "  notes         Generate release notes for a version (AI-assisted)"
	@echo "  bump-version  Sync version to an explicit version"
	@echo "  check-version Validate version matches tag (CI check)"
	@echo "  clean       Remove build artifacts (cargo clean)"
	@echo ""
	@echo "Variables:"
	@echo "  OS          Target OS: macos | linux | windows  (default: $(HOST_OS))"
	@echo "  ARCH        Target arch: arm64 | x86_64 | universal  (default: $(HOST_ARCH))"
	@echo "  BUNDLES     Installer types (default per OS)"
	@echo "  SIGN        1=enable signing, 0=--no-sign  (default: 0)"
	@echo "  SIGN_IDENTITY  Signing identity CN, e.g. \"Mole Desktop Signing\" (macOS)"
	@echo "  INIT_TARGETS  current | all  (default: current)"
	@echo "  TYPE        Release bump type: patch | minor | major (for release)"
	@echo "  DRY_RUN     1=preview release without changing files"
	@echo "  VERSION     Target version for bump-version / notes (e.g. 0.1.2)"
	@echo "  NO_AI       1=build notes from commit subjects without AI"
	@echo "  PUBLISH     1=publish generated notes to an existing GitHub Release"
	@echo "  FROM        Previous tag for notes (default: latest reachable tag)"
	@echo "  TAG         Tag version for check-version (default: current git tag)"
	@echo ""
	@echo "Examples:"
	@echo "  make init"
	@echo "  make build-cli"
	@echo "  make build-app"
	@echo "  make build-app OS=macos ARCH=universal BUNDLES=app,dmg"
	@echo "  make build-app OS=windows ARCH=x86_64 BUNDLES=msi,nsis"
	@echo "  make dev-app"
	@echo "  make run-cli"
	@echo "  make release TYPE=patch"
	@echo "  make release TYPE=minor DRY_RUN=1"
	@echo "  make bump-version VERSION=0.1.2"
	@echo "  make notes VERSION=0.8.0"
	@echo "  make notes VERSION=0.8.0 NO_AI=1"
	@echo "  make notes VERSION=0.8.0 PUBLISH=1"
	@echo "  make check-version TAG=0.1.2"
	@echo ""
	@echo "Note: Tauri app builds must run on the target OS (macOS universal is an exception)."
	@echo "      GitHub Actions release workflow packages macOS (universal) and Windows (x86_64)."

init: init-rust init-tauri init-frontend
	@echo ""
	@echo "==> System dependencies (install manually if missing):"
	@case "$(HOST_OS)" in \
		macos)  echo "    macOS: Xcode Command Line Tools" ;; \
		linux)  echo "    Linux: webkit2gtk, libayatana-appindicator, etc. (see Tauri docs)" ;; \
		*)      echo "    Windows: WebView2, Visual Studio C++ Build Tools" ;; \
	esac

init-rust:
	@echo "==> Fetching Rust dependencies..."
	$(CARGO) fetch --manifest-path $(APP_DIR)/Cargo.toml
	@echo "==> Adding rustup targets..."
	@targets=""; \
	if [ "$(INIT_TARGETS)" = "all" ]; then \
		targets="aarch64-apple-darwin x86_64-apple-darwin \
		         aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu \
		         x86_64-pc-windows-msvc"; \
	else \
		case "$(OS)" in \
			macos) \
				case "$(ARCH)" in \
					arm64)     targets="aarch64-apple-darwin" ;; \
					x86_64)    targets="x86_64-apple-darwin" ;; \
					universal) targets="aarch64-apple-darwin x86_64-apple-darwin" ;; \
				esac ;; \
			linux) \
				case "$(ARCH)" in \
					arm64)  targets="aarch64-unknown-linux-gnu" ;; \
					x86_64) targets="x86_64-unknown-linux-gnu" ;; \
				esac ;; \
			windows) \
				targets="x86_64-pc-windows-msvc" ;; \
		esac; \
	fi; \
	for t in $$targets; do \
		echo "    rustup target add $$t"; \
		rustup target add $$t; \
	done

init-tauri:
	@echo "==> Checking tauri-cli..."
	@if $(CARGO) tauri --version >/dev/null 2>&1; then \
		echo "    tauri-cli already installed: $$($(CARGO) tauri --version)"; \
	else \
		echo "    Installing tauri-cli..."; \
		$(CARGO) install tauri-cli --locked; \
	fi

init-frontend:
	@echo "==> Installing frontend dependencies..."
	$(NPM) install

icons:
	@echo "==> Regenerating app/installer icons from $(APP_DIR)/icons/app-icon.png..."
	$(CARGO) tauri icon $(APP_DIR)/icons/app-icon.png
	@rm -rf $(APP_DIR)/icons/android $(APP_DIR)/icons/ios
	@echo "    Removed mobile icon sets (android/ios)"

build-frontend:
	@echo "==> Building frontend..."
	$(NPM) run build

# Build the raw desktop binary (no installers). Web assets are embedded at
# compile time, so the frontend is built first.
build-cli: build-frontend
	@triple=$$( $(MAKE) -s print-triple OS=$(OS) ARCH=$(ARCH) ); \
	native=$$( $(MAKE) -s is-native-triple TRIPLE=$$triple ); \
	echo "==> Building binary for $(OS)/$(ARCH) ($$triple)..."; \
	if [ "$(OS)" = "macos" ] && [ "$(ARCH)" = "universal" ]; then \
		$(CARGO) build --release --manifest-path $(APP_DIR)/Cargo.toml --target aarch64-apple-darwin --bin $(BIN_NAME); \
		$(CARGO) build --release --manifest-path $(APP_DIR)/Cargo.toml --target x86_64-apple-darwin --bin $(BIN_NAME); \
		mkdir -p $(TARGET_DIR)/universal-apple-darwin/release; \
		lipo -create \
			$(TARGET_DIR)/aarch64-apple-darwin/release/$(BIN_NAME) \
			$(TARGET_DIR)/x86_64-apple-darwin/release/$(BIN_NAME) \
			-output $(TARGET_DIR)/universal-apple-darwin/release/$(BIN_NAME); \
		echo "==> Output: $(TARGET_DIR)/universal-apple-darwin/release/$(BIN_NAME)"; \
	elif [ "$$native" = "1" ]; then \
		$(CARGO) build --release --manifest-path $(APP_DIR)/Cargo.toml --bin $(BIN_NAME); \
		echo "==> Output: $(TARGET_DIR)/release/$(BIN_NAME)"; \
	else \
		$(CARGO) build --release --manifest-path $(APP_DIR)/Cargo.toml --target $$triple --bin $(BIN_NAME); \
		echo "==> Output: $(TARGET_DIR)/$$triple/release/$(BIN_NAME)"; \
	fi

build-app: check-app-host
	@triple=$$( $(MAKE) -s print-triple OS=$(OS) ARCH=$(ARCH) ); \
	bundles=$$( $(MAKE) -s print-bundles OS=$(OS) ); \
	if [ -n "$(BUNDLES)" ]; then bundles="$(BUNDLES)"; fi; \
	sign_args=""; \
	if [ "$(SIGN)" != "1" ]; then sign_args="--no-sign"; fi; \
	if [ -n "$(SIGN_IDENTITY)" ]; then export APPLE_SIGNING_IDENTITY="$(SIGN_IDENTITY)"; fi; \
	echo "==> Building $(APP_NAME) for $(OS)/$(ARCH) ($$triple)"; \
	echo "    bundles: $$bundles"; \
	$(CARGO) tauri build --target $$triple --bundles $$bundles $$sign_args; \
	bundle_root="$(CURDIR)/$(TARGET_DIR)/$$triple/release/bundle"; \
	echo ""; \
	echo "==> Build complete. Artifacts:"; \
	if [ -d "$$bundle_root" ]; then \
		find "$$bundle_root" \( \
			-type f \( -name "*.dmg" -o -name "*.deb" -o -name "*.AppImage" -o \
			          -name "*.msi" -o -name "*-setup.exe" \) \
			-o -type d -name "*.app" \
		\) 2>/dev/null | sort | sed 's/^/    /'; \
	else \
		echo "    (bundle directory not found: $$bundle_root)"; \
	fi

dev-app:
	$(CARGO) tauri dev

run-cli: build-cli
	@bin=$$( $(MAKE) -s print-bin-path OS=$(OS) ARCH=$(ARCH) ); \
	echo "==> Running $$bin"; \
	"$$bin"

release:
	@test -n "$(TYPE)" || (echo "Usage: make release TYPE=patch|minor|major" >&2; exit 1)
	@if [ "$(DRY_RUN)" = "1" ]; then \
		bash scripts/release.sh "$(TYPE)" --dry-run; \
	else \
		bash scripts/release.sh "$(TYPE)"; \
	fi

notes:
	@args=""; \
	if [ -n "$(VERSION)" ]; then args="$$args $(VERSION)"; fi; \
	if [ -n "$(FROM)" ]; then args="$$args --from $(FROM)"; fi; \
	if [ -n "$(EFFORT)" ]; then args="$$args --effort $(EFFORT)"; fi; \
	if [ "$(NO_AI)" = "1" ]; then args="$$args --no-ai"; fi; \
	if [ "$(PUBLISH)" = "1" ]; then args="$$args --publish"; fi; \
	if [ "$(DRY_RUN)" = "1" ]; then args="$$args --dry-run"; fi; \
	bash scripts/release-notes.sh $$args

bump-version:
	@test -n "$(VERSION)" || (echo "Usage: make bump-version VERSION=0.1.2" >&2; exit 1)
	@bash scripts/bump-version.sh "$(VERSION)"

check-version:
	@bash scripts/check-version.sh "$(TAG)"

clean:
	$(CARGO) clean --manifest-path $(APP_DIR)/Cargo.toml

# --- internal helpers (not .PHONY — safe to call via make -s) ---

print-triple:
	@case "$(OS)" in \
		macos) \
			case "$(ARCH)" in \
				arm64)     echo "aarch64-apple-darwin" ;; \
				x86_64)    echo "x86_64-apple-darwin" ;; \
				universal) echo "universal-apple-darwin" ;; \
				*) echo "ERROR: unsupported ARCH=$(ARCH) for OS=macos (use arm64, x86_64, or universal)" >&2; exit 1 ;; \
			esac ;; \
		linux) \
			case "$(ARCH)" in \
				arm64)  echo "aarch64-unknown-linux-gnu" ;; \
				x86_64) echo "x86_64-unknown-linux-gnu" ;; \
				*) echo "ERROR: unsupported ARCH=$(ARCH) for OS=linux (use arm64 or x86_64)" >&2; exit 1 ;; \
			esac ;; \
		windows) \
			case "$(ARCH)" in \
				x86_64) echo "x86_64-pc-windows-msvc" ;; \
				*) echo "ERROR: unsupported ARCH=$(ARCH) for OS=windows (use x86_64)" >&2; exit 1 ;; \
			esac ;; \
		*) echo "ERROR: unsupported OS=$(OS) (use macos, linux, or windows)" >&2; exit 1 ;; \
	esac

print-bundles:
	@case "$(OS)" in \
		macos)   echo "app,dmg" ;; \
		linux)   echo "deb,appimage" ;; \
		windows) echo "msi,nsis" ;; \
		*)       echo "app" ;; \
	esac

print-bin-path:
	@bin="$(BIN_NAME)"; \
	if [ "$(OS)" = "windows" ]; then bin="$(BIN_NAME).exe"; fi; \
	triple=$$( $(MAKE) -s print-triple OS=$(OS) ARCH=$(ARCH) ); \
	native=$$( $(MAKE) -s is-native-triple TRIPLE=$$triple ); \
	if [ "$(OS)" = "macos" ] && [ "$(ARCH)" = "universal" ]; then \
		echo "$(TARGET_DIR)/universal-apple-darwin/release/$$bin"; \
	elif [ "$$native" = "1" ]; then \
		echo "$(TARGET_DIR)/release/$$bin"; \
	else \
		echo "$(TARGET_DIR)/$$triple/release/$$bin"; \
	fi

is-native-triple:
	@host_triple=""; \
	case "$(HOST_OS)" in \
		macos) \
			case "$(HOST_ARCH)" in \
				arm64)  host_triple="aarch64-apple-darwin" ;; \
				x86_64) host_triple="x86_64-apple-darwin" ;; \
			esac ;; \
		linux) \
			case "$(HOST_ARCH)" in \
				arm64)  host_triple="aarch64-unknown-linux-gnu" ;; \
				x86_64) host_triple="x86_64-unknown-linux-gnu" ;; \
			esac ;; \
		*) \
			case "$(HOST_ARCH)" in \
				x86_64) host_triple="x86_64-pc-windows-msvc" ;; \
			esac ;; \
	esac; \
	if [ "$(TRIPLE)" = "$$host_triple" ]; then echo "1"; else echo "0"; fi

check-app-host:
	@case "$(OS)" in \
		macos) \
			if [ "$(HOST_OS)" != "macos" ]; then \
				echo "ERROR: build-app OS=macos requires a macOS host." >&2; exit 1; \
			fi ;; \
		linux) \
			if [ "$(HOST_OS)" != "linux" ]; then \
				echo "ERROR: build-app OS=linux requires a Linux host." >&2; exit 1; \
			fi ;; \
		windows) \
			if [ "$(HOST_OS)" != "windows" ]; then \
				echo "ERROR: build-app OS=windows requires a Windows host." >&2; exit 1; \
			fi ;; \
		*) \
			echo "ERROR: unsupported OS=$(OS)" >&2; exit 1 ;; \
	esac
