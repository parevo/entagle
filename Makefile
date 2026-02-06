.PHONY: dev dev-server dev-client-a dev-client-b

dev:
	bash scripts/dev.sh

dev-server:
	cargo run -p signaling-server

dev-client-a:
	cd apps/desktop-client && VITE_PORT=5173 npm run tauri dev

dev-client-b:
	cd apps/desktop-client && VITE_PORT=5174 npm run tauri dev
