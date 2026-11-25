# 发布流程

本文档记录 `on-selected-text` 的 Cargo 与 npm 发布门禁。真实发布是不可逆操作：crates.io 版本不能覆盖，npm 首发包名也会被占用；发布前必须先完成 dry-run。

## Cargo crate

当前 crate：`on-selected-text`。

发布前检查：

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo publish --dry-run --registry crates-io
cargo package --list --registry crates-io
```

首次发布需要在本机登录 crates.io：

```bash
cargo login --registry crates-io
cargo publish --registry crates-io
```

首次发布后，可在 crates.io 配置 GitHub Actions trusted publishing，并使用 `.github/workflows/cargo-release.yaml` 手动发布后续版本。该 workflow 默认只 dry-run；只有手动 dispatch 且 `publish=true` 时才会请求 OIDC token 并执行 `cargo publish`。

## npm packages

Root package：`node-on-selected-text`。平台包：

- `node-on-selected-text-darwin-arm64`
- `node-on-selected-text-darwin-x64`
- `node-on-selected-text-win32-x64-msvc`
- `node-on-selected-text-win32-arm64-msvc`
- `node-on-selected-text-linux-x64-gnu`
- `node-on-selected-text-linux-arm64-gnu`

Root package 只发布 JS loader、类型和 smoke 工具，不携带 `.node` 二进制。平台包各自声明 `os` / `cpu` / `libc`，并由 root package 的 `optionalDependencies` 引用；npm 安装时只会安装与当前平台匹配的 native binding，避免所有端二进制都进入用户依赖树。

本地发布前检查：

```bash
cd bindings/node
npm run verify:package
npm run verify:platform-packages
npm pack --dry-run --json
```

真实 npm 发布应走 `.github/workflows/node-release.yaml`：

1. 在 npmjs.com 为 root package 和 6 个平台包分别配置 trusted publisher。
2. Publisher 配置使用同一个 GitHub 仓库 `uxiew/on-selected-text` 和 workflow 文件名 `node-release.yaml`。
3. 手动运行 `Node Release` workflow，先保持 `publish=false` 验证 artifacts 和 pack dry-run。
4. 验证通过后再手动运行 `publish=true` 发布平台包，最后发布 root package。

不要在本机直接 `npm publish` root package；本机无法构建和验证所有平台二进制，也无法生成完整 provenance。
