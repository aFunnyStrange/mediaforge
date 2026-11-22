# MediaForge · 媒体转换

Windows 10/11 x64 的 Rust 桌面转换工具。参考 Office Drop 的小型悬浮窗交互：拖入文件，选择格式，点击转换。无需终端、Rust 环境、系统 FFmpeg 或 Microsoft Office。

[下载最新版本](https://github.com/aFunnyStrange/mediaforge/releases/latest) · [Windows CI](https://github.com/aFunnyStrange/mediaforge/actions/workflows/windows.yml)

## 直接使用

完整解压 `MediaForge-windows-x64.zip`，双击其中的 **MediaForge.exe**。请保留旁边的 `engines` 文件夹；不要只复制主程序，也不要在压缩包预览中运行。

1. 拖入一个或多个文件，或点击“选择文件…”。一次可处理多张图片，或多个音视频文件；图片和音视频请分别选择。
2. 选择目标格式、质量。图片可以限制最大宽度，保持比例，不放大小图。
3. 选择同时转换的文件数：1 / 2 / 4，默认 2。低内存电脑或大分辨率视频建议选择 1。
4. 点击“开始转换”。结果写到每个原文件旁的 **MediaForge 输出** 文件夹。原文件不变，同名输出自动加编号。
5. 完成后点击“打开输出目录”。如果输入分属多个目录，此按钮打开最后一个成功任务的目录，全部输出路径可在结果框查看和复制。

### 暂停、继续和取消

- **暂停**：队列不再启动新任务；正在运行的音视频任务通过 Windows 挂起 FFmpeg 进程，保留内存和临时输出，约 100 毫秒轮询响应。正在运行的图片转换会完成当前文件，然后暂停。
- **继续**：恢复进程和队列，从保留的进度继续。
- **取消**：停止队列，终止正在转码的 FFmpeg，清理未完成的临时输出；已成功的文件保留。图片转换在当前文件完成后响应。
- 暂停不是断点存档，退出应用或重启电脑后不能恢复。正常关闭前请先取消；应用异常退出时，Windows Job Object 会终止它拥有的 FFmpeg 子进程。强制结束/断电可能留下 `.mediaforge-*` 临时文件，可手动删除。
- 窗口默认置顶，可取消勾选“保持置顶”。转换期间可以最小化，界面始终在独立主线程响应。

## 支持的格式

| 类型 | 输入 | 可选输出 |
| --- | --- | --- |
| 图片 | PNG、JPG/JPEG、WebP、BMP、GIF、ICO、TIFF、PNM/PPM/PGM/PBM/PAM、TGA、QOI | PNG、JPG、WebP、BMP、TIFF、GIF、ICO、QOI、TGA、PPM、PGM |
| 音频 | MP3、M4A、AAC、FLAC、WAV、OGG、Opus、WMA、AIFF/AIF、APE、ALAC、M4B | MP3、M4A、AAC、FLAC、WAV、OGG、Opus、WMA、AIFF |
| 视频 | MP4、MKV、MOV、WebM、AVI、WMV、FLV、MPEG/MPG、TS、M4V、MTS/M2TS、3GP、VOB | MP4、MKV、MOV、WebM、AVI、WMV、FLV、MPEG、TS、动态 GIF，或提取为上述音频格式 |

具体输入是否可解码取决于文件实际编码，损坏、加密文件或无音轨的视频会在结果框说明失败；其他排队文件继续处理。

### 质量和兼容性

- JPEG 质量：较小 70 / 均衡 85 / 较高 95；其他图片编码不使用这一参数。WebP 使用无损编码。
- JPEG/PPM/PGM 不保留透明通道，透明部分合成白底；ICO 自动限制在 256×256 内。读取支持的方向信息后自动旋转。
- GIF、动态 WebP、APNG 作为图片输入时仅转换第一帧，多页 TIFF 仅处理第一幅图。**视频转 GIF** 为完整动画，宽 480 像素、12 fps，无音频；长视频的 GIF 可能很大。
- 图片转换不保留 EXIF、ICC 等元数据，不是专业色彩管理或无损元数据迁移工具；不支持 SVG、RAW、HEIC/HEIF、AVIF。
- 常见有损音频目标码率为 128 / 192 / 320 kbps，默认输出双声道。FLAC 不主动降采样；WAV/AIFF 输出 16-bit PCM。无损编码的文件大小不受质量选项控制。MP3 等有损格式互转会再次损失音质，转回 FLAC 不能恢复音质。
- MP4/MKV/MOV/FLV/TS 默认 H.264 + AAC；WebM 为 VP9 + Opus；AVI 为 MPEG-4 + MP3；WMV 为 WMV2 + WMA；MPEG 为 MPEG-2 + MP2。H.264/VP9 使用 CRF 28 / 23 / 18，旧格式视频固定质量，音频仍随质量选项调整。
- 视频输出仅保留第一路非封面视频和第一路音频（可无音频），不保留字幕、附件或额外轨道；像素格式 yuv420p，奇数尺寸收缩到偶数。当前不提供无损封装复制、HDR 色调映射或 GPU 编码。

## 架构

`src/gui.rs` 是原生 Win32 窗口；`batch.rs` 管理有界队列、唯一输出名和逐文件结果；`images.rs` 使用 Rust image 编解码；`media.rs` 直接启动打包的 FFmpeg，不使用 shell 拼接命令。`control.rs` 管理暂停与子进程生命周期；`output.rs` 在输出目录暂存，成功后提交，默认拒绝覆盖。

主线程只处理 UI，通过消息通道收取进度。后台调度线程拥有 1–4 个 scoped 工作线程，每个音视频任务拥有独立 FFmpeg 子进程。每个进程的编解码线程数按逻辑 CPU / 并发数分配，限制在 1–8；滤镜线程限制为 1。实际总线程数还包含 FFmpeg 的内部 I/O 等线程。图片解码设有 512 MiB 分配限制，但这不是整个进程或所有并发任务的总内存上限。

暂停使用 `NtSuspendProcess` / `NtResumeProcess` Native API，仅作用于自有子进程，失败会中止对应任务并报告。它不是微软承诺稳定的 Win32 暂停接口。实现依据：[System Informer 的 phnt 声明](https://github.com/winsiderss/phnt/blob/master/ntpsapi.h)；FFmpeg 的交互命令可见[上游实现](https://github.com/FFmpeg/FFmpeg/blob/master/fftools/ffmpeg.c)。

## 开发与打包

仅开发者需要 Rust MSVC 工具链、Windows SDK 和完整的 [Gyan FFmpeg Windows 静态发行包](https://www.gyan.dev/ffmpeg/builds/)。`.cargo/config.toml` 为 x64 MSVC 静态链接 C 运行库。

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
# 含真实音视频转换、暂停/恢复和子进程清理测试
$env:MEDIAFORGE_TEST_FFMPEG = (Get-Command ffmpeg).Source
cargo test -- --include-ignored
# 生成完整便携包（开发者可显式传入 -FfmpegPath）
.\scripts\package.ps1
```

产物在 `dist/MediaForge-windows-x64/` 和同名 ZIP。程序不从 PATH 查找 FFmpeg，也不自动下载组件。只需附带 `ffmpeg.exe`；当前转换功能不需要 FFprobe，减少打包体积。开发时单独 `cargo run` 可进行图片转换；音视频需要在 `target/debug/engines/` 放入引擎，或使用完整便携包测试。

自动化测试分别覆盖纯逻辑、图片读写、并发队列、FFmpeg 转码矩阵、暂停/恢复和异常清理；真实窗口的操作验证记录见 `docs/VALIDATION.md`。

GitHub Actions 对 main、PR 和手动运行执行 Windows 检查与打包，固定 Rust 1.93.1 与经 SHA-256 校验的 FFmpeg 9.0。提交版本匹配的 `v*` 标签后，只有该标签上的完整测试和解压包检查成功，才会发布 Release（ZIP 和 SHA-256）。发布说明位于 `docs/releases/`。
