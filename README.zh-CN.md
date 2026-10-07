# Mobile Preview Plugin

[English](README.md)

Mobile Preview Plugin（MPP）是独立的 Rust 框架，用于为 DeepSeek Harness Web 和
Desktop 连接移动设备。目标产品将提供模拟器与真机的实时预览和输入控制，不依赖
mobile-dev-harness（mdh）。

**当前状态：Rust 连接框架与本地 DSH 开发插件。** 已通过 CLI 和 Web/Desktop
共用的 DSH 按钮及面板提供 Android 设备发现、显式启动模拟器、连接探测和会话
所有权管理。实时视频与输入注入尚未实现。连接成功返回的 `transport_ready` 仅
表示已检查并保留所选 adb 连接，尚无可用视频流。

## 范围

| 能力 | 状态 |
| --- | --- |
| 发现已连接的 Android 设备和已安装的 AVD | 已实现 |
| 启动明确选定的 Android AVD | 已实现 |
| 按精确序列号探测设备，管理进程内会话租约 | 已实现 |
| JSON-lines 控制协议与有大小限制的二进制媒体分帧 | 已实现 |
| Android NDK 编码器生命周期与输出封装 | 原型，提供独立的编码器探测程序 |
| 屏幕采集、实时视频传输与播放、设备输入 | 未实现 |
| DSH Web/Desktop 连接按钮与设备面板 | 已实现，已在固定的源码开发基线上验证 |
| iOS Simulator 与 iOS 真机 | 计划中，尚无后端 |

目前可以通过 adb 发现和探测 Android 真机，但这不代表已支持真机预览或控制。音频
和录制不在当前范围内。MPP 参考 scrcpy 将视频与控制分离的设计，但不运行、分发
或依赖 scrcpy，也不实现其通信协议。

## 构建与试用

需要 Rust 1.88 或以上版本。设备命令还需要 Android SDK platform-tools（`adb`）；
AVD 发现与启动需要 SDK emulator 组件以及已创建的 AVD。

```sh
cargo build --workspace --locked
./target/debug/mpp --version
./target/debug/mpp devices
./target/debug/mpp probe --device emulator-5554
```

将 `emulator-5554` 替换为发现结果中的序列号。发现与探测不会隐式选择或启动设备。
需要启动现有 AVD 时，使用 `devices` 返回的精确名称明确执行：

```sh
./target/debug/mpp boot --avd Pixel_9
```

模拟器以无窗口模式启动，并等待 Android 完成启动。成功启动后，命令或 Host 退出
不会关闭模拟器；断开会话也不会关闭设备。

SDK 工具优先从 `ANDROID_HOME`、`ANDROID_SDK_ROOT` 和常规 SDK 安装位置查找，
然后查找 `PATH`。也可以显式指定：

```sh
./target/debug/mpp --adb /path/to/adb --emulator /path/to/emulator devices
```

指定 `--adb` 后，如果需要发现或启动 AVD，也应指定 `--emulator`。缺少 emulator
工具时仍可发现已连接设备，同时会返回警告。

## DSH 开发插件

适配层位于 `packages/dsh-plugin`，不依赖 mdh，也没有新增第三方 Node 依赖；
界面使用 DSH 运行时提供的 React 和 UI 服务。当前兼容基线为 DSH 源码
`0.2.1-alpha.1`，提交 `5badb15009ae1756c3afe0ae0cef1faafc290ccc`。

准备好该源码后，在本仓库中选择一个界面启动：

```sh
node scripts/dev-dsh.mjs web --source /path/to/deepseek-harness --port 3081
node scripts/dev-dsh.mjs desktop --source /path/to/deepseek-harness
```

启动器在 `target/dsh/` 下使用独立的开发 home 和 Desktop 用户数据，不会复制
已有 profile 的凭据或设置，也不需要卸载正常使用的 DSH 客户端。Node/pnpm 环境、
首次构建和本地插件加载方法见 [DSH 开发说明](docs/DSH-DEVELOPMENT.md)。

固定版本的 DSH 源码已完整构建通过，未修改 DSH 源码。Web 界面与原生 Harness Dev
Desktop 均成功加载按钮和面板，并列出本机四个 AVD。Web 面板已启动
`Pixel_6_API_32`、连接并断开；原生 Desktop 面板也已达到 `transport_ready`。
这些结果验证了连接行为，不代表已支持投屏或设备输入。适配层 56 项测试与现有
52 项 Rust 测试均通过；更广泛的生命周期和平台验证仍需分别完成。

所有权仅在各自的进程内生效，因此 Web 与 Desktop 应依次测试同一台设备。Web
操作的是 DSH Host 所在机器连接的设备。面板没有注册 agent 工具，也不会赋予
模型视觉能力；stdio 桥接属于内部协议，不是 MCP。

## 本地 Host 协议

需要会话的客户端应启动一个持续运行的 Host 进程：

```sh
./target/debug/mpp serve --stdio
```

每行发送一个 JSON 对象，例如：

```json
{"id":"intro","method":"hello"}
{"id":"devices","method":"devices.list"}
{"id":"connect","method":"session.connect","params":{"owner":"chat-a","device":"android:emulator-5554"}}
```

后续状态查询与断开操作使用返回的会话 ID 和 generation。Host 退出或输入到达 EOF
时释放全部租约。所有权仅在该进程内生效，不会锁住其他 adb 客户端或阻止设备上的
触摸。这是本地子进程协议，不是 MCP 或网络 API；owner 字符串不是身份认证凭据。

Android 租约还绑定 adb transport ID，同一序列号被新的连接复用时，旧会话会失效。
重启 adb server 后应同时重启 MPP。

消息结构、限制和未支持操作见[控制与媒体协议](docs/CONTROL-PROTOCOL.md)；边界与
后续阶段见[设计文档](docs/DESIGN.md)。

## 开发

工作区包含四个 crate：

| Crate | 职责 |
| --- | --- |
| `mpp-core` | 设备类型、会话租约、输入校验与媒体分帧，无平台 I/O |
| `mpp-android` | Android SDK 发现以及有资源限制的 adb/emulator 子进程调用 |
| `mpp-android-device` | 设备端 Rust NDK MediaCodec 生命周期、输入 surface 和编码输出封装 |
| `mpp-host` | `mpp` CLI 与持续运行的 stdio Host |

已批准的基础依赖为 Tokio、Serde、serde_json 和 thiserror。设备与媒体行为由
Rust 负责；少量 JavaScript 适配代码将其会话绑定到 DSH 聊天，并提供
Web/Desktop 共用界面。

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm --prefix packages/dsh-plugin run check
```

默认测试不需要设备、Android SDK 或网络，覆盖协议、所有权、分帧和子进程行为；
通过这些测试不代表已完成实时视频或性能验证。

### Android 编码器探测

设备端原型可以在 macOS 或 Linux x86_64 上交叉编译，目标为 Android API 32 或
以上版本、`aarch64-linux-android`；目前仅测试过 API 32。需要已安装的 Android
NDK 和所选编译器对应的 Rust 目标。脚本使用 `--locked`，可能下载锁定的 Cargo
依赖，但不会安装 SDK 组件、Rust 工具链或目标：

```sh
./scripts/build-android-device.sh
```

依赖已缓存时，可设置 `CARGO_NET_OFFLINE=true` 以禁止构建访问网络。

可通过 `ANDROID_NDK_HOME` 选择已安装的 NDK。如果目标安装在另一套工具链下，应
显式选择，例如 `RUSTUP_TOOLCHAIN=1.88 ./scripts/build-android-device.sh`。脚本会
输出 `libmpp_android_device.so` 和 `codec_probe` 可执行文件的路径；默认位于
`target/aarch64-linux-android/debug/` 下。

运行探测前，选择一台已授权、运行 API 32 或以上版本的 arm64 Android 设备。
从 `adb devices -l` 中读取其数字 `transport_id`，替换下方的 `1`。使用 `-t`
将每条命令绑定到这一次设备连接，避免序列号复用导致目标变化。以下命令上传并在
使用后删除本次探测文件：

```sh
adb devices -l
MPP_TRANSPORT_ID=1
adb -t "$MPP_TRANSPORT_ID" push target/aarch64-linux-android/debug/examples/codec_probe /data/local/tmp/mpp-codec-probe
adb -t "$MPP_TRANSPORT_ID" shell chmod 700 /data/local/tmp/mpp-codec-probe
adb -t "$MPP_TRANSPORT_ID" shell /data/local/tmp/mpp-codec-probe
adb -t "$MPP_TRANSPORT_ID" shell rm /data/local/tmp/mpp-codec-probe
```

成功时，探测程序输出包含 `encoder`、`input_surface`、`capture_verified: false`
以及可选 `first_output` 元数据的 JSON。它执行编码器创建、输入 surface 获取和
输出轮询，但不会把屏幕连接到该 surface。编码器初始化成功或返回输出元数据，均
不能证明屏幕采集或硬件加速已实现。平台采集桥接方案尚未确定；Host 中的
`preview.start` 与 `input.send` 仍返回 `UNSUPPORTED`。

在 `Pixel_6_API_32` 模拟器（Android API 32、arm64-v8a）上连续运行三次探测，均
完成编码器生命周期，并返回 `encoder: "video/avc"`、`input_surface: true`、
`capture_verified: false` 和 `first_output: null`。这仅验证了配置、创建输入
surface、启动、输出出队、停止和释放；尚未证明能够产出编码帧或采集屏幕。

仓库处于开发阶段，各 crate 均设置了 `publish = false`。尚未确定公开包发布或许可
协议。纳入 DSH 官方分发属于未来的上游贡献目标，不代表目前已被内置或获得认可。
