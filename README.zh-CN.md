# Mobile Preview Plugin

[技术方案与使用指南](https://mobile-dev-harness.github.io/mobile-preview-plugin-site/)

[English](README.md)

Mobile Preview Plugin（MPP）是独立的 Rust 框架，用于为 DeepSeek Harness Web 和
Desktop 连接移动设备。目标产品将提供模拟器与真机的实时预览和输入控制，不依赖
mobile-dev-harness（mdh）。

首个公开预发布版 [**`v0.1.0-preview.5`**](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/tag/v0.1.0-preview.5)
已发布，范围为现有 Android 支持加上本机 iOS Simulator。精确 CI 归档已通过溯源及
记录的 Web／Desktop／原生检查；具体验收范围和剩余限制见
[发布记录](docs/RELEASING.md)。npm 包继续保持私有。这是开发预览，不是 1.0 稳定版。

**当前状态：已实现 Android 视频与控制，以及按设备能力启用的实验性 iOS Simulator
视频、单指点击／拖动和 Home。** 当前本地源码构建已在
macOS 26.7 Apple Silicon／Xcode 26.4／iOS 26.4／iPhone 17 上，
通过原版 DSH Web `0.2.1-alpha.1` 和官方 Desktop `0.2.0-rc.2` 验证 Home、画布点击
和可见的拖动滚动效果。来自 `31a2bdb` 的 preview.5 归档还单独通过了干净 Web／Desktop
视频与控制、显式启动 Simulator、Pause／Resume，以及原生清理／重启检查。该目标上的
原生输入效果，以及 Web 画布调整大小后的点击坐标也已通过验证。
初始范围仅限正向竖屏，不支持键盘／文本／IME、多点触控、Back 或 iOS 真机。
只有原生能力探测通过后才启用输入，否则保留只读预览。清理时会通过结束当前
iOS 手势来释放触点，因此可能完成该次点击或拖动。其他 Xcode／runtime
组合及部分生命周期场景仍未验收；详见
[iOS Simulator 证据与剩余检查](docs/IOS-SIMULATOR.md)。

对于 Android，Rust 后端与本地 DSH 插件提供设备发现、显式启动模拟器、会话所有权、
H.264 视频流以及单指触摸和基础
按键输入。当前发布版已实现 **Android 10–17 / API 29–37 arm64-v8a 适配**，
需要 Unix 主机和支持 WebCodecs 的客户端。运行验收仍在进行，进入允许范围不代表
兼容性通过。API 30–37 矩阵目标的原生采集／输入／清理已通过，其中 API 37 模拟器
使用 16 KiB 页。原版 DSH Web 的 API 30–34 界面检查使用本地矩阵归档通过，API 35–37
使用单独标识的下载 CI 归档通过。同一个 CI 包还在 API 37 上通过官方 Desktop 的 GUI
检查，包括输入、暂停／恢复、窗口缩放、卸载和重新安装。所测
API 29 模拟器环境受编码器 surface 路径问题阻塞。此前 API 32 模拟器和 API 36
真机结果作为历史证据保留。
各目标结果与缺口见 [Android 兼容性矩阵](docs/ANDROID-COMPATIBILITY.md)。
`transport_ready` 仍然只是设备连接状态；新连接建立后，可见面板会自动启动预览，
并显示真实的解码进度。

## 范围

| 能力 | 状态 |
| --- | --- |
| 发现已连接的 Android 设备和已安装的 AVD | 已实现 |
| 启动明确选定的 Android AVD | 已实现 |
| 按精确序列号探测设备，管理进程内会话租约 | 已实现 |
| JSON-lines 控制协议与有大小限制的二进制媒体分帧 | 已实现 |
| Rust/JNI 采集、NDK H.264 编码和 WebCodecs 播放 | 已实现 API 29–37 arm64 适配，按兼容性矩阵逐目标验收 |
| 单指触摸与基础 Android 按键 | 已实现，已在目标模拟器验证 Web/Desktop 点击、拖动和导航 |
| DSH Web/Desktop 连接按钮与设备面板 | 已实现，原版 Web 与官方 Desktop GUI 已在记录的归档及目标上验证 |
| Android／iOS 平台选择与可选的 `open_mobile_preview` agent 工具 | 已实现；为当前聊天打开对应平台面板，不选择或连接设备 |
| iOS Simulator 发现、显式启动和视频 | 实验性源码实现；前一阶段已在 Xcode 26.4／iOS 26.4／iPhone 17 通过 Web／Desktop 视频检查 |
| iOS Simulator 单指点击／拖动和 Home | 原生输入能力验证后启用；已在记录的目标上通过原生、原版 Web 和官方 Desktop 实际效果检查 |
| iOS 键盘／文本／IME、多点触控、Back 和真机 | 尚未实现 |

目前可以通过 adb 发现和探测 Android 真机；预览／输入验收独立于设备发现，按下方
实际测试目标记录。音频
和录制不在当前范围内。MPP 参考 scrcpy 将视频与控制分离的设计，但不运行、分发
或依赖 scrcpy，也不实现其通信协议。

## 安装预览发布包

打包脚本会生成自带运行文件的 `.tgz`，当前面向 **macOS 14 或更新版本的 Apple Silicon DSH Host**
并提供 **Android API 29–37 arm64-v8a 适配**，以及通过已安装 Xcode 使用本机
**iOS Simulator** 的能力。iOS 验收限于 macOS 26.7／Xcode 26.4／iOS 26.4／iPhone 17，
不包含 iOS 真机。已验证的 `0.1.0-preview.5` 归档身份和具体验收范围见
[发布检查表](docs/RELEASING.md)；此前安装包检查保留为历史证据。
`0.1.0-preview.1` 安装包已在官方 npm DSH `0.2.1-alpha.1`
Web 发行版的独立 profile 中通过干净安装、播放和卸载验收，未使用源码补丁、
运行文件路径覆盖或模型 API key。官方 Desktop `0.2.0-rc.2` 应用已通过签名和
公证检查；`0.1.0-preview.3` 已通过其内置 CLI 在独立 profile 中安装并启用。
随后，下载的精确 CI preview.3 归档在 API 37 上通过了该官方应用的 GUI 预览、输入
和生命周期检查。此前源码构建 Desktop 的检查继续作为独立历史证据保留。

按照[打包说明](docs/DSH-DEVELOPMENT.md#build-a-self-contained-preview-package)生成安装包后，
在 DSH 的 **Plugins → Add plugin** 中输入 `.tgz` 的绝对路径，安装并启用。
Web profile 对应的 CLI 命令为：

```sh
dsh plugin --profile web add /absolute/path/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz
```

下载 [macOS Apple Silicon 归档](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz)、
[SHA-256 文件](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz.sha256)
和[验收记录](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/qualification-0.1.0-preview.5.json)。
不可变归档与标签均使用源码提交 `31a2bdb`；本次后续文档更新不会替换安装包。
安装前请验证校验和。

安装包包含 Rust 主程序和配套的 Android bootstrap／原生库，启动时会按已安装
插件的位置自动解析路径。使用安装包不需要设置 `MPP_EXECUTABLE`，也不需要
Rust、NDK 或 JDK。使用 Android 时，DSH Host 需安装 Android SDK platform-tools，并连接已授权的
API 29–37 arm64 设备；使用前请查看兼容性矩阵中的验收状态。使用模拟器时还需要
emulator 组件和现有的受支持 AVD。
Web 客户端需要 H.264 WebCodecs 支持。
原版 DSH 的侧栏宽度继续手动调整；预览和输入不依赖可选的宽度补丁。

打开设备面板后，先选择 **Android** 或 **iOS**。新聊天默认不选择平台，也不会
触发 adb 设备发现。选择 **Android** 后加载设备列表，再选择明确的设备并点击
**Connect** 开始预览。在 macOS 上使用当前源码构建时，选择 **iOS** 会列出本机
Xcode Simulator，供用户明确启动并查看视频；所选 Simulator 的输入探测通过后，
可使用点击／拖动和 Home，否则保留只读视频。其他主机会显示后端不可用。
目前不支持 iOS 真机。此前已验收的 Android 归档不包含或
证明本次 iOS 实现。前置条件、限制和卸载方法见
[插件包说明](packages/dsh-plugin/README.md)。源码开发仍支持显式配置
`executable` 和 `deviceAssets`。

使用 iOS 返回主屏幕手势时，从真实屏幕内部最底部的 2% 区域开始，短距离向上滑动。
装饰边框不属于触摸区域。手势验收范围见 [iOS 指南](docs/IOS-SIMULATOR.md)。
用户手动操作的 Web 应用切换器手势已于 2026-10-10 通过；Desktop 手动应用切换器
仍未验收。

安装新的原生插件包版本后，请先重启 DSH Host 再连接设备；热重载可能仍保留上一版
包内运行文件的相对路径。

此前 `0.1.0-preview.1` 安装包的 Web 检查使用 API 32 模拟器，覆盖自动首帧、
点击／拖动、Home／Back、手动调整侧栏宽度，以及
面板重新打开后的 Pause／Resume。连接中卸载插件后，界面完成卸载，MPP 主程序、
设备 bootstrap 进程和 adb reverse 映射均已清理。本地证据为
`target/stock-validation/evidence/stock-web-live.png`。打包说明还介绍了由相关路径
推送或显式指定手动版本触发的 GitHub Actions 构建，仅上传工作流产物，不发布版本。
从运行 `37652914514` 下载的 preview.3 归档（SHA-256
`d6f6450826638bdb44cce5a2b8669965036f7de87ec8d79e2468d3b5d08f9b9c`）
已通过源码溯源、API 37 原生运行和 Web／Desktop 干净 CLI 安装检查。随后，原版
Web `0.2.1-alpha.1` 在 API 35–37 上解码首帧，并验证拖动、Settings／Apps 点击
以及 Back／Home。API 37 还通过重开面板后的 Pause／Resume，以及 1280×820 和
窄视口来回切换保持 Live 的检查。

官方 Desktop `0.2.0-rc.2` 在 API 37 上解码了同一个归档，验证 Settings／Apps
点击、拖动、Back／Home、主动暂停在重开面板后仍保持、Resume 返回 Live，以及
原生窗口缩放来回切换。在插件管理器移除插件后，入口和 renderer 已卸载，依赖、
bundle、helper 与所属 reverse 映射完成清理；通过 GUI 重新安装并启用后恢复 Live。
打开插件管理器会隐藏预览，因此卸载测试从此前已连接的聊天进入。Web 在 API 35
已连接会话之后执行 GUI 卸载，同样移除了入口，将设备视图替换为 DSH 的无 renderer
占位，清理依赖／bundle，且没有遗留 helper 或所属 reverse 映射。Web 先收起面板，
再进入插件管理器；通过 GUI 重新安装、启用并连接 API 35 后已恢复真实 Live 画面。
证据与截图位于 `target/release-readiness/gui/`；归档身份与
剩余范围见[发布检查清单](docs/RELEASING.md)。

`0.1.0-preview.2` 安装包已安装到 macOS arm64 上的官方 npm DSH Web
`0.2.1-alpha.1` 发行版。nubia P0110（Android 16/API 36、arm64）的 1264×2800
屏幕已以 576×1280 显示实时画面，并验证了 Home／Back、点击进入显示与亮度、
拖动设置列表以及 Pause／Resume。暂停后没有遗留 helper；三轮独立的原生
启动／采集／停止均清理了 helper、adb reverse 映射和 MPP 虚拟显示。相同的新
主程序与设备端产物还通过了 API 32 模拟器的采集／清理回归。本地证据为
`target/android36/evidence/phone-live.png`。本次未测试真机旋转、热拔插和长时间
GUI 运行。这些手机结果不代表后续 CI 归档已经通过真机或其他厂商验收。

## 从源码构建与试用

需要 Rust 1.88 或以上版本。Android 命令还需要 Android SDK platform-tools（`adb`）；
AVD 发现与启动需要 SDK emulator 组件以及已创建的 AVD。iOS Simulator 命令则需要
macOS Apple Silicon、Xcode 和已安装的 iOS Simulator runtime；iOS 预览不需要
Android SDK 或 Android 设备端产物。

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

实验性的 iOS 后端使用 `devices` 返回的精确 Simulator UDID；启动仍需明确请求：

```sh
./target/debug/mpp boot --simulator <UDID>
./target/debug/mpp probe --simulator <UDID>
```

CLI 接受 `--xcrun /absolute/path/to/xcrun`，默认使用 `/usr/bin/xcrun`。
当前选定的 Xcode 提供 runtime 与 CoreSimulator 组件。源码配置、采集边界和具体
验收范围见 [iOS 指南](docs/IOS-SIMULATOR.md)。断开预览后，Simulator 仍保持运行。

## DSH 开发插件

适配层位于 `packages/dsh-plugin`，不依赖 mdh，也没有新增第三方 Node 依赖；
界面使用 DSH 运行时提供的 React 和 UI 服务。当前兼容基线为 DSH 源码
`0.2.1-alpha.1`，提交 `5badb15009ae1756c3afe0ae0cef1faafc290ccc`。
手机预览的自动侧栏宽度还需要本地 DSH UI 扩展；它不属于该上游提交，也不是已经
被官方合入的 API。

使用 Android 时，先构建[设备端产物](#android-设备端构建与编码器探测)，再准备好固定版本的
DSH 源码，在本仓库中选择一个界面启动：

```sh
node scripts/dev-dsh.mjs web --source /path/to/deepseek-harness --port 3081
node scripts/dev-dsh.mjs desktop --source /path/to/deepseek-harness
```

启动器在 `target/dsh/` 下使用独立的开发 home 和 Desktop 用户数据，不会复制
已有 profile 的凭据或设置，也不需要卸载正常使用的 DSH 客户端。Node/pnpm 环境、
首次构建、设备端产物与本地插件加载方法见 [DSH 开发说明](docs/DSH-DEVELOPMENT.md)。

应用可选的[侧栏宽度补丁](docs/DSH-DEVELOPMENT.md#automatic-sidebar-width-local-dsh-extension)后，
MPP 根据可用高度、手机宽高比以及外围机身请求合适宽度。DSH 保留自身布局限制，
手动拖动优先。离开当前停靠面板、进入全屏、拆分或浮动面板时，会释放临时宽度请求，
恢复已保存的宽度偏好。未应用补丁的 DSH 继续使用手动侧栏宽度，视频与输入不变。
手机外框加入前，仅调整宽度的 Desktop 视觉检查已通过（95/100）：在 1280×820
CSS 视口中，侧栏从 576 自动
缩至 300 CSS 像素，画面高度没有减少。手动设为 430 像素后，可用高度变化不会
覆盖该宽度；切到引导页恢复这一偏好，返回 MPP 后再次自动适配。全屏和窗口高度
适配检查通过；拆分／浮动仅有单元测试覆盖，未单独实机验证。本地证据为
`target/dsh/evidence/desktop-adaptive-sidebar.png`。

最初的投屏验证使用了未修改的固定版本 DSH 源码。Web 界面与原生 Harness Dev
Desktop 均成功加载按钮和面板，并列出本机四个 AVD。在 `Pixel_6_API_32` 上，Web
已解码真实 H.264 视频流；画布点击从 Settings 进入 Apps，Back 返回 Settings，
拖动使页面滚动，Home 返回启动器。本地截图证据位于
`target/dsh/evidence/web-live-control.jpg`。

原生链路连续运行 600.1 秒保持正常，前 180 秒画面静止，之后每 30 秒执行一次
Home 按下／抬起。记录了 1 个配置包、8 个关键帧、228 个增量帧和 1,184,670 字节。
原生检查还确认：2.6 秒看门狗检查时触摸已释放，控制通道 EOF 后同样释放；90°
和 180° 旋转按预期结束视频流；10/10 次重新启动／停止均正常清理，设备设置已恢复。
这种画面变化稀疏的运行检查不是吞吐量或延迟基准测试。

修正解码背压后，Desktop 已解码首帧。Home 从 Calendar 返回启动器；拖动打开应用
抽屉；点击从 Settings 进入 Apps；Back 返回 Settings。约两分钟的交互中，调整
侧栏大小后预览保持实时。一次明确的停止检查后，没有残留 MPP 原生 helper 或自建
adb reverse 映射。随后完整重启 Desktop 进程，在移除临时诊断代码后再次成功解码
实时画面。最终实时画面截图为 `target/dsh/evidence/desktop-live-control.png`。
竞争采集请求被拒绝时，已有视频流也未中断。此前一次原因未明的 Web EOF 未在
原生持续运行或最新 Web 检查中再次出现，但原因仍未查明。这些结果不能证明更广泛
的平台兼容性或端到端 GUI 的持续可靠性，也不构成吞吐量或延迟承诺。

选择 **Android** 后，选择已授权且处于当前 API／ABI 实现范围内的设备，点击
**连接**后，可见的手机屏幕会自动启动预览。
聊天会在
调整大小、暂时隐藏／进入后台或视图替换期间保留预览意图。隐藏时暂停采集并释放
输入；只要连接仍有效，返回可见视图后会等待旧采集清理完毕，再自动恢复。主动
**暂停**后将保持暂停，直到点击**继续**。真正的错误会阻止当前可见视图不断重试；
只要预览意图和连接仍有效，点击**重试**／**继续**、进入新的可见视图或重新回到
前台，都可以进行一次新尝试。普通重绘和心跳不会触发重试。设备旋转或几何变化仍
会结束对应采集 epoch，不会重放输入。断开、租约过期和插件卸载会清除意图，不会
自动重连已经失效的设备连接。

屏幕外新增圆角金属风格边框、听筒和装饰侧键。这些装饰位于真实屏幕之外，不接受
输入；画布宽高比和触摸坐标仍然对应设备像素。自动适配会计入边框、内边距和外部
留白。这次连接即预览、自动恢复与手机外观更新仅修改 MPP，复用原有可选 DSH 宽度
扩展。

Desktop 运行检查中，只点**连接**即进入 Live。点击外框内 Settings 屏幕的 Apps
成功进入 Apps，Home 也正常。调整窗口大小和系统窗口缩放均保持 Live，各次检查中
helper 进程没有更换。收起再打开面板后，旧采集先完成清理，再自动恢复，没有 BUSY。
主动暂停后 helper 数量为零，收起／重开仍保持暂停，点击继续后恢复 Live。后台
document 事件有单元测试覆盖，但尚未单独测试真实 OS 最小化／后台返回。此前画面
高度不变的结果来自加入外框之前；现在外框会占用部分可用高度。本地证据为
`target/dsh/evidence/desktop-auto-preview-device-frame.png`。本轮视觉审核已通过
（96/100）：边框、听筒和侧键均位于屏幕外，没有拉伸画面，也没有装饰遮挡状态栏
或应用内容；控制按钮可读，侧栏保持紧凑。

连接租约仅在各自的进程内生效；设备端 helper 还会拒绝同一设备上竞争采集的其他
MPP 进程，但不会阻止外部 adb 或人工触摸。Web 与 Desktop 应依次测试同一台设备。
Web 操作的是 DSH Host 所在机器连接的设备；stdio 桥接属于内部协议，不是 MCP。

DSH 提供 tools 服务时，插件会注册唯一的可选 agent 工具：
`open_mobile_preview({platform: "android"})`（也可传入 `"ios"`）。例如，可以对
agent 说：“为当前聊天打开 Android 预览。”工具会选择平台并请求打开调用它的聊天
面板，具体设备仍由你选择和连接。它不能启动或连接设备、发送输入或截取屏幕，
也不会赋予模型视觉能力。iOS 请求会在 macOS 上打开 Simulator 选择界面，
可用的手动控制由所选设备的原生能力决定。没有 tools 服务时，仍可手动使用插件。

平台选择按聊天保存在 DSH Host 的有容量上限的内存中；记录仍保留时，浏览器刷新后
可以恢复，但插件服务或 DSH Host 重启后会重置。已有不同平台的设备连接时，切换
会返回 `BUSY`，需要先明确断开连接。页面可见时，已挂载的当前聊天每两秒检查请求，
每个新的 agent 请求只打开一次面板，非当前聊天不会抢占焦点。关闭面板后，同一个
请求不会反复打开它；新的 agent 请求可以再次打开。

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

工作区包含五个 crate：

| Crate | 职责 |
| --- | --- |
| `mpp-core` | 设备／会话类型、采集 epoch、输入状态和媒体分帧，无平台 I/O |
| `mpp-android` | SDK／设备发现、生命周期、helper 部署与通道认证 |
| `mpp-android-device` | Rust/JNI Android 采集与输入，以及 NDK H.264 编码 |
| `mpp-ios` | 本机 Simulator 发现／启动、隔离的 CoreSimulator／IOSurface → VideoToolbox 采集，以及按能力启用的 DTUHID 输入 |
| `mpp-host` | `mpp` CLI、stdio 生命周期协议与私有媒体／控制转发 |

基础依赖为 Tokio、Serde、serde_json 和 thiserror；另已批准 `jni` 0.22 与极小的
Java bootstrap，用于访问 Android framework。Java 只初始化运行时和加载 Rust；
采集、媒体、传输与输入仍由 Rust 负责。JavaScript 适配层将会话绑定到 DSH 聊天，
并提供 Web/Desktop 共用界面。
iOS crate 没有新增外部依赖，通过小型原生 FFI 边界调用已安装的 Apple framework；
这些 framework 不随插件分发。

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm --prefix packages/dsh-plugin run check
node --test scripts/tests/*.test.mjs
```

默认测试不需要设备、Android SDK 或网络，覆盖协议、所有权、分帧和子进程行为；
通过这些测试不代表已完成实时视频或性能验证。

### Android 设备端构建与编码器探测

可以在 macOS 或 Linux x86_64 上构建，需要已安装的 Android NDK、Rust
`aarch64-linux-android` 目标、JDK 17 或以上版本、含 D8 的 Android Build Tools，
以及 API 29 或以上版本的 Android platform `android.jar`。原生与 DEX 产物最低
API 均为 29；共享的 `android_framework(api)` 策略选择 API 29–33 或 34–37
实现，并要求 arm64。构建会验证 ELF LOAD 的 16 KiB 对齐；另有 API 37 矩阵测试
验证了一台 16 KiB 页模拟器上的原生运行。脚本使用 `--locked`，可能下载 Cargo 依赖，
不安装 SDK 或 Rust 组件：

```sh
./scripts/build-android-device.sh
```

默认使用 `debug` 配置；设置 `MPP_BUILD_PROFILE=release` 可构建设备端 release
产物。自包含安装包的打包脚本会为主程序和 Android 产物都选择 release。

依赖已缓存时，可设置 `CARGO_NET_OFFLINE=true` 以禁止构建访问网络。

可通过 `ANDROID_NDK_HOME` 和 `JAVA_HOME` 选择已安装的工具链。
`MPP_BUILD_TOOLS` 可指定 build-tools 版本目录的绝对路径；`MPP_ANDROID_JAR` 可
指定 platform JAR 的绝对路径。如果 Rust 目标位于另一套工具链中，可显式选择，
例如 `RUSTUP_TOOLCHAIN=1.88 ./scripts/build-android-device.sh`。脚本使用
`javac --release 8` 编译 bootstrap，再经 D8 转换，输出三个产物路径：

```text
target/android-device/bootstrap.jar
target/android-device/libmpp_android_device.so
target/aarch64-linux-android/debug/examples/codec_probe
```

`CARGO_TARGET_DIR` 可以改变全部产物的位置。使用其他输出位置启动 DSH 时，将
`MPP_DEVICE_ASSETS` 设置为 `android-device` 输出目录的绝对路径。

运行探测前，选择一台已授权、运行 API 29 或以上版本的 arm64 Android 设备。
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
不能证明屏幕采集或硬件加速。实际预览使用单独的 bootstrap 和原生采集路径；
探测程序返回 `capture_verified: false` 是有意保留的行为。

在 `Pixel_6_API_32` 模拟器（Android API 32、arm64-v8a）上连续运行三次探测，均
完成编码器生命周期，并返回 `encoder: "video/avc"`、`input_surface: true`、
`capture_verified: false` 和 `first_output: null`。这仅验证了配置、创建输入
surface、启动、输出出队、停止和释放；这些历史探测没有验证当前的实时视频链路。

源码仓库已公开并处于开发阶段，各 crate 均设置了 `publish = false`，源码和暂存插件的
manifest 均保留 `private: true`；生成本地 `.tgz` 不会发布它。MPP 采用
[Apache-2.0](LICENSE) 许可；随包依赖保留各自的许可，记录于
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) 和 `licenses/third-party/`，
这些文件也随预览包分发。
[GitHub 预发布版 `v0.1.0-preview.5`](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/tag/v0.1.0-preview.5)
已发布，本次不包含 npm 发布。
已测范围、剩余限制和发布状态见
[RELEASING.md](docs/RELEASING.md)。纳入 DSH 官方分发属于未来的上游贡献目标，
不代表目前已被内置或获得认可。这是开发预览，不是 1.0 稳定版。
