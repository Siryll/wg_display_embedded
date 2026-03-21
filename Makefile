.PHONY: all clean frontend app build flash monitor help

build: build

run: flash

help:
	@echo "ESP32 WG Display Build System"
	@echo ""
	@echo "Targets:"
	@echo "  frontend    - Build frontend"
	@echo "  run         - Build and flash ESP32 application"
	@echo "  build       - Build all components"
	@echo "  clean       - Clean all build artifacts"

frontend:
	@echo "Building tailwind CSS..."
	cd frontend && npm run tailwind-build
	@echo "Building frontend with trunk..."
	cd frontend && trunk build --release
	@echo "Compressing WASM file..."
	gzip -k -f frontend/dist/frontend_bg.wasm
	@echo "Frontend build complete"

app-build: frontend
	@echo "Building release..."
	cd embedded_app && cargo build --release

# Complete build process
build: frontend app-build

build-all: install-deps build

install-deps:
	@echo "Installing cargo dependencies..."
	cargo install --locked trunk
	@echo "Installing tailwindcss dependencies..."
	cd frontend && npm install && rustup target add wasm32-unknown-unknown

# Flash to ESP32
run:
	@echo "Flashing to ESP32..."
	cd embedded_app && cargo run --release

# Clean all build artifacts
clean:
	@echo "Cleaning ESP32 build artifacts..."
	cd embedded_app && cargo clean
	@echo "Cleaning frontend build artifacts..."
	cd frontend && cargo clean && rm -rf dist node_modules/.cache
	@echo "Clean complete"
