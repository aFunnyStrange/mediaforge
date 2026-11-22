# 第三方组件

MediaForge 通过独立进程调用随包提供的 FFmpeg。随包的 Gyan 9.0 full build 为 GPL v3；原始 LICENSE、README（包含确切源码版本和构建配置）、版本输出及二进制 SHA-256 均保留在便携包中。

- FFmpeg 构建来源：https://www.gyan.dev/ffmpeg/builds/
- 本次构建源码版本：https://github.com/FFmpeg/FFmpeg/commit/d32b387f2b
- FFmpeg 源代码：https://github.com/FFmpeg/FFmpeg
- FFmpeg 编解码说明：https://ffmpeg.org/ffmpeg-codecs.html
- image 格式支持：https://docs.rs/image/0.25.10/image/codecs/index.html

便携包中 `licenses/rust/` 收录 Cargo 锁定依赖的许可证文件，`rust-dependencies.json` 记录其名称、版本和声明的许可证。项目原始 Rust 代码为 MIT，第三方代码继续适用各自许可证。FFmpeg 的对应源码及其静态依赖不在此项目中重新授权；如对外再分发，应随发行版履行对应许可证的源码提供要求。
