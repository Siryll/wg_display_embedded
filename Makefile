.PHONY: all clean frontend app build flash monitor help

build: build

run: flash

help:
	@echo "ESP32 WG Display Build System"
	@echo ""
	@echo "Targets:"
	@echo "  frontend    - Build frontend with trunk"
	@echo "  app-release - Build ESP32 application (release)"
	@echo "  build       - Build frontend + prepare + app"
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

# Flash to ESP32
run: frontend
	@echo "Flashing to ESP32..."
	cd embedded_app && cargo run --release

# Clean all build artifacts
clean:
	@echo "Cleaning ESP32 build artifacts..."
	cd embedded_app && cargo clean
	@echo "Cleaning frontend build artifacts..."
	cd frontend && cargo clean && rm -rf dist node_modules/.cache
	@echo "Clean complete"
