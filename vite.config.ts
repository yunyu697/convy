import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // 必须绑 IPv4：Windows 上 Node 有时只监听 ::1，WebView2 走 127.0.0.1 会连不上
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "chrome105",
    minify: "esbuild",
    sourcemap: false,
  },
});
