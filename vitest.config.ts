import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// vitest 会把转译结果缓存到 os.tmpdir() 下的临时目录。受限环境（沙箱 / 受管机器）
// 里系统临时目录常常不可写，写失败时 vitest 只打一行 "Unhandled Error" 然后
// **静默跳过**受影响的测试文件 —— 看起来“通过了”，实际少跑了一半。
// 把临时目录指到项目内，写不写得进去至少是确定的。
const here = dirname(fileURLToPath(import.meta.url));
const tmp = join(here, ".vitest-tmp");
mkdirSync(tmp, { recursive: true });
process.env.TMPDIR = tmp;
process.env.TMP = tmp;
process.env.TEMP = tmp;

// 独立于 vite.config.ts：构建配置里不需要 test 字段，
// 而 vitest 会优先读取本文件，两边互不干扰。
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    restoreMocks: true,
    /**
     * 串行跑测试文件。
     *
     * vitest 2.x 的模块缓存在每个进程里各自记账，但文件名只按模块 id 做哈希，
     * 两个 worker 并发写同一个缓存文件时 Windows 会直接报 EPERM，
     * 整个文件被丢弃（表现就是“只跑了一个测试文件”）。
     * 现在只有两个测试文件、几百毫秒的事，串行最省心；
     * 以后文件多到影响体验，再升级 vitest 或换 pool。
     */
    fileParallelism: false,
  },
});
