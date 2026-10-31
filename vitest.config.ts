import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// 独立于 vite.config.ts：构建配置里不需要 test 字段，
// 而 vitest 会优先读取本文件，两边互不干扰。
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/__tests__/setup.ts"],
    restoreMocks: true,
  },
});
