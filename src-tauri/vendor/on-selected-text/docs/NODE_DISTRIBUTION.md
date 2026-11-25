# Node 包分发规划

本文档规划 `on-selected-text` 的 Node.js 绑定与 npm 分发方式。Rust crate 继续作为核心能力层，Node 包只做薄绑定，避免把平台逻辑复制到 JS 侧。

## 目标

- 复用当前 Rust Pull / Push / bounds / permission API。
- 为 Node.js、Electron、CLI 工具提供稳定接口。
- 发布预编译 native addon，普通用户安装时不需要本机 Rust 工具链。
- 保持 Rust crate 与 Node binding 可独立测试、独立发布。

## 推荐技术路线

采用 `napi-rs`：

- 使用 Node-API，避免跟具体 Node ABI 强绑定。
- 支持 `napi build --platform --release` 生成平台 native addon。
- 使用 `napi.targets` 声明预编译目标。
- 发布前使用 `napi prepublish -t npm` 生成 npm 平台包映射。

## 建议目录结构

```text
bindings/
└── node/
    ├── Cargo.toml              # cdylib crate，依赖根 crate
    ├── build.rs                # napi build hook
    ├── package.json            # npm 包元数据与 napi.targets
    ├── src/lib.rs              # napi-rs 导出函数
    ├── index.d.ts              # 对外 TypeScript 类型
    ├── index.js                # napi 生成或包装后的入口
    ├── smoke.js                # 真实选区 smoke CLI
    └── smoke.test.js           # Node 层 smoke 参数与断言测试
```

当前仓库已经升级为 Cargo workspace，根 Rust crate 与 Node binding 统一纳入验证：

```toml
[workspace]
members = [".", "bindings/node"]
resolver = "2"
```

## Node API 草案

```ts
export interface ScreenRect {
  x: number
  y: number
  width: number
  height: number
}

export interface SelectionResult {
  text: string
  bounds?: ScreenRect
}

export interface GetTextOptions {
  allowClipboardFallback?: boolean
  maxWaitMs?: number
  fetchBounds?: boolean
}

export interface DaemonOptions {
  dragThresholdPx?: number
  triggerDelayMs?: number
  getTextOptions?: GetTextOptions
}

export type PermissionStatus =
  | { status: 'granted' }
  | { status: 'denied'; platform: string; permission: string; guidance: string }
  | { status: 'not_required' }
  | { status: 'unsupported' }

export function getSelectedText(options?: GetTextOptions): Promise<SelectionResult>
export function checkPermissions(): PermissionStatus
export function startSelectionDaemon(
  options: DaemonOptions | undefined | null,
  callback: (err: null | Error, selection: SelectionResult) => void,
): { stop(): void; isStopped(): boolean }
```

实现约定：

- JS 使用 camelCase，Rust core 保持 snake_case。
- `getSelectedText()` 建议返回 Promise，避免调用方在 Electron 主线程里误以为无阻塞。
- `startSelectionDaemon()` 返回 JS handle；`stop()` 必须释放 Rust `DaemonHandle`，`isStopped()` 可用于 UI 状态展示。
- 错误映射为 `Error`，并在 `message` 前缀保留稳定错误码：`NO_TEXT_SELECTED`、`PERMISSION_DENIED`、`FALLBACK_DISABLED`、`FALLBACK_FAILED`、`UNSUPPORTED_PLATFORM`。后续如需要更强 JS 语义，可在包装层补 `error.code`。

## 预编译目标

首批建议支持：

| npm platform | Rust target | 说明 |
| --- | --- | --- |
| `darwin-arm64` | `aarch64-apple-darwin` | Apple Silicon |
| `darwin-x64` | `x86_64-apple-darwin` | Intel macOS |
| `win32-x64-msvc` | `x86_64-pc-windows-msvc` | Windows 主目标 |
| `win32-arm64-msvc` | `aarch64-pc-windows-msvc` | Windows ARM64 |
| `linux-x64-gnu` | `x86_64-unknown-linux-gnu` | glibc Linux |
| `linux-arm64-gnu` | `aarch64-unknown-linux-gnu` | ARM64 Linux |

`musl` 目标暂缓：当前 Linux 依赖 AT-SPI2 / X11 / Wayland 相关系统库，先保证 glibc 桌面发行版可用。

## package.json 要点

```json
{
  "name": "node-on-selected-text",
  "version": "0.1.0",
  "main": "index.js",
  "types": "index.d.ts",
  "repository": {
    "type": "git",
    "url": "git+https://github.com/uxiew/on-selected-text.git",
    "directory": "bindings/node"
  },
  "publishConfig": {
    "registry": "https://registry.npmjs.org/",
    "access": "public"
  },
  "homepage": "https://github.com/uxiew/on-selected-text/tree/main/bindings/node",
  "files": ["README.md", "index.js", "index.d.ts", "smoke.js", "smoke.test.js", "binding.test.js"],
  "scripts": {
    "build": "napi build --platform --release",
    "prepublishOnly": "napi prepublish -t npm --skip-optional-publish",
    "test": "node --test smoke.test.js",
    "test:binding": "node --test binding.test.js",
    "pack:dry-run": "npm pack --dry-run --json",
    "verify:package": "npm run build && npm test && npm run test:binding && npm run pack:dry-run",
    "create-npm-dirs": "napi create-npm-dirs",
    "release:artifacts": "napi artifacts --output-dir ./artifacts --npm-dir ./npm",
    "release:prepare": "napi prepublish -t npm --skip-optional-publish",
    "release:sync-platform-metadata": "node scripts/sync-platform-metadata.js",
    "release:clean-root-binaries": "node scripts/clean-root-binaries.js",
    "verify:platform-packages": "node scripts/verify-platform-packages.js",
    "smoke": "node smoke.js",
    "smoke:interactive": "node smoke.js --delay-ms 5000 --settle-ms 250 --retries 2 --retry-delay-ms 250"
  },
  "napi": {
    "binaryName": "node_on_selected_text",
    "targets": [
      "aarch64-apple-darwin",
      "x86_64-apple-darwin",
      "x86_64-pc-windows-msvc",
      "aarch64-pc-windows-msvc",
      "x86_64-unknown-linux-gnu",
      "aarch64-unknown-linux-gnu"
    ]
  },
  "optionalDependencies": {
    "node-on-selected-text-darwin-arm64": "0.1.0",
    "node-on-selected-text-darwin-x64": "0.1.0",
    "node-on-selected-text-win32-x64-msvc": "0.1.0",
    "node-on-selected-text-win32-arm64-msvc": "0.1.0",
    "node-on-selected-text-linux-x64-gnu": "0.1.0",
    "node-on-selected-text-linux-arm64-gnu": "0.1.0"
  }
}
```

Root package 不携带 `.node` 文件；native addon 只存在于 `npm/<platform>` 平台包中。平台包通过 `os` / `cpu` / Linux `libc` 元数据限制安装范围，root package 只用 `optionalDependencies` 指向它们。

## CI / Release 阶段

1. **Rust core CI**
   - `cargo fmt --check`
   - `cargo test`
   - `cargo clippy --all-targets -- -D warnings`
   - Linux / Windows target `cargo check`
2. **Node binding CI**
   - Node LTS matrix：Node 20 / 22。
   - macOS / Windows / Linux 三平台 runner 原生构建，不做跨平台伪构建。
   - `npm install`。
   - `napi build --platform --release`。
   - `npm test` 覆盖 smoke CLI 断言逻辑。
   - `npm run test:binding` 验证 native addon 可加载、`checkPermissions` 可调用；headless CI 默认跳过 daemon 生命周期，macOS job 通过 `RUN_SELECTION_DAEMON_SMOKE=1` 额外验证 `startSelectionDaemon` 可启动和停止。
   - `npm run pack:dry-run` 验证 npm 包内容。
   - napi-rs Windows prebuild 需要在 Windows runner 上执行；不要在 macOS/Linux runner 上直接 cross-check Windows Node addon，否则构建脚本会寻找 `libnode.dll`。
3. **Prebuild release**
   - `node-v*` tag 或手动 workflow 触发；tag 默认只构建/打包，不自动发布。
   - 六个平台 runner 原生构建 `.node` artifacts：`macos-14` arm64、`macos-13` x64、`windows-latest` x64、`windows-11-arm` arm64、`ubuntu-latest` x64、`ubuntu-24.04-arm` arm64。
   - `npm run release:sync-platform-metadata` 把 root package 的 `repository` / `homepage` / `bugs` 等发布元数据同步到平台包，并重写平台包 README。
   - `napi artifacts --output-dir ./artifacts --npm-dir ./npm` 把下载的 `.node` artifacts 放入平台 npm 包。
   - `napi prepublish -t npm --skip-optional-publish` 生成 root package 的 optionalDependencies 映射，但不隐式发布平台包。
   - `npm run release:clean-root-binaries` 删除 root package 里的 release `.node`，避免主包重复携带所有平台二进制。
   - `npm run verify:platform-packages -- --require-binaries --forbid-root-binaries` 校验每个平台包 manifest、二进制 payload，并确认 root 包不携带 release 二进制。
   - `npm pack --dry-run --json` 分别验证 root 包和所有平台包内容；root 包中不应出现 `.node` 文件。
   - 只有手动运行 `Node Release` workflow 且 `publish=true` 时才执行 `npm publish --provenance --access public`；发布前必须补齐与 GitHub 仓库匹配的 `repository.url`，并在 npm 为 root 包和所有平台包配置 trusted publisher。

## Consumer 验证

仓库内只保留核心 Rust crate 与 Node binding。Electron、桌面工具条等集成示例应放在独立 consumer 仓库中，避免把宿主应用样例、图标和打包配置混入核心项目。

推荐独立 consumer 的验证方式：

```bash
cd bindings/node
npm run verify:package
npm pack --json
# 在独立 consumer 仓库中安装上一步生成的 node-on-selected-text-*.tgz，再运行宿主侧 smoke。
```

验证重点：

- consumer 应从 npm tarball 或正式 npm 包安装 `node-on-selected-text`，不要直接依赖本仓库源码目录。
- smoke 应检查 `checkPermissions` / `getSelectedText` / `startSelectionDaemon` 导出，至少覆盖 daemon 启动与停止。
- GUI 宿主应用还需要额外记录权限主体、后台常驻策略、窗口生命周期和 CPU/内存占用。

## 落地顺序

1. 新增 `bindings/node` 最小 napi-rs crate，只导出 `checkPermissions()` 与 `getSelectedText()`。
2. 补 TypeScript 类型与 Node smoke 测试；真实应用 smoke 支持 `--expect-text` / `--expect-contains` / `--expect-regex` / `--min-length` 断言。
3. 使用独立 consumer 仓库验证打包后的 Node 产物能被 Electron 或其他宿主进程消费。
4. 接入 `startSelectionDaemon()`，重点验证 handle 生命周期与后台常驻。
5. 增加 GitHub Actions prebuild matrix 与平台 npm 包目录。
6. 手动执行 `Node Release` workflow 的非发布路径，确认 artifacts、platform packages、pack dry-run 全部通过。
7. 发布 alpha npm 包，使用 `docs/TEST_MATRIX.md` 的真实应用矩阵做人工回归。

## 风险点

- Electron 主进程和渲染进程权限不同，文档要明确推荐在主进程调用。
- macOS Accessibility 权限以宿主应用为准；Node CLI、Electron App、测试 runner 需要分别授权。系统权限提示需要应用显式调用 prompt API（例如 `AXIsProcessTrustedWithOptions` + `kAXTrustedCheckOptionPrompt`），不要假设普通读取失败会自动弹窗。
- Linux prebuild 不能覆盖所有桌面依赖，首版只承诺主流 glibc 桌面发行版。
- npm trusted publishing 要求 Node/npm 版本、OIDC 权限和 `repository.url` 同时正确；若使用私有仓库，provenance 可能不会生成。
- Push daemon 回调跨 Rust/JS 边界时要避免阻塞 Hook 线程，必须继续保持“事件触发”和“读取/回调”分离。
