use crate::{
    control::{suspend, Control},
    output::Output,
    Error, Result,
};
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

pub const AUDIO: &[&str] = &[
    "mp3", "m4a", "aac", "flac", "wav", "ogg", "opus", "wma", "aiff",
];
pub const VIDEO: &[&str] = &[
    "mp4", "mkv", "mov", "webm", "avi", "wmv", "flv", "mpeg", "ts",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    Small,
    #[default]
    Balanced,
    High,
}

#[derive(Debug, Clone)]
pub struct Engine {
    pub ffmpeg: PathBuf,
    pub threads: usize,
}
impl Engine {
    /// Portable distribution: use the sibling executable, never depend on PATH.
    pub fn bundled() -> Result<Self> {
        let root = std::env::current_exe()?
            .parent()
            .ok_or_else(|| Error::Invalid("无法确定程序目录".into()))?
            .to_owned();
        Ok(Self {
            ffmpeg: root.join("engines").join("ffmpeg.exe"),
            threads: 2,
        })
    }
    pub fn convert(
        &self,
        input: &Path,
        output: &Path,
        quality: Quality,
        cancel: &AtomicBool,
    ) -> Result<()> {
        self.convert_controlled(input, output, quality, &Control::default(), cancel)
    }
    pub fn convert_controlled(
        &self,
        input: &Path,
        output: &Path,
        quality: Quality,
        control: &Control,
        cancel: &AtomicBool,
    ) -> Result<()> {
        if !control.wait_ready() || cancel.load(Ordering::Relaxed) {
            return Err(Error::Invalid("已取消".into()));
        }
        if !self.ffmpeg.is_file() {
            return Err(Error::Invalid(
                "缺少随程序附带的 engines/ffmpeg.exe，请完整解压便携包后再运行。".into(),
            ));
        }
        let ext = crate::extension(output)?;
        let profile = profile(&ext, quality)?;
        let pending = Output::new(input, output, false)?;
        let mut log = tempfile::tempfile()?;
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args([
                "-threads",
                &self.threads.clamp(1, 8).to_string(),
                "-filter_threads",
                "1",
                "-filter_complex_threads",
                "1",
            ])
            .arg("-i")
            .arg(input.canonicalize()?)
            .args(profile)
            .args(["-threads", &self.threads.clamp(1, 8).to_string()])
            .arg(pending.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log.try_clone()?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(|e| Error::Process {
            program: "FFmpeg".into(),
            message: format!("无法启动转换引擎：{e}"),
        })?;
        #[cfg(windows)]
        let _job = match crate::control::ProcessJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let mut suspended = false;
        let status = loop {
            if cancel.load(Ordering::Relaxed) || control.cancelled.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Invalid("已取消，未保存未完成的文件".into()));
            }
            let pause = control.paused.load(Ordering::SeqCst);
            if pause != suspended {
                if let Err(e) = suspend(&child, pause) {
                    // The child may have exited between the pause request and the native call.
                    if let Ok(Some(status)) = child.try_wait() {
                        break status;
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e);
                }
                suspended = pause;
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e.into());
                }
            }
        };
        if !status.success() {
            let length = log.metadata()?.len();
            log.seek(SeekFrom::Start(length.saturating_sub(16 * 1024)))?;
            let mut bytes = Vec::new();
            log.read_to_end(&mut bytes)?;
            return Err(Error::Process {
                program: "FFmpeg".into(),
                message: format!(
                    "文件可能损坏、没有所需音视频轨道或编码不受支持。\n{}",
                    String::from_utf8_lossy(&bytes)
                ),
            });
        }
        pending.commit()
    }
}

fn profile(ext: &str, quality: Quality) -> Result<Vec<String>> {
    let bitrate = match quality {
        Quality::Small => "128k",
        Quality::Balanced => "192k",
        Quality::High => "320k",
    };
    let crf = match quality {
        Quality::Small => "28",
        Quality::Balanced => "23",
        Quality::High => "18",
    };
    let mut args: Vec<&str> = Vec::new();
    if AUDIO.contains(&ext) {
        args.extend(["-map", "0:a:0", "-vn"]);
        let codec = match ext {
            "mp3" => "libmp3lame",
            "m4a" | "aac" => "aac",
            "flac" => "flac",
            "wav" => "pcm_s16le",
            "aiff" => "pcm_s16be",
            "ogg" => "libvorbis",
            "opus" => "libopus",
            "wma" => "wmav2",
            _ => unreachable!(),
        };
        args.extend(["-c:a", codec]);
        if !matches!(ext, "flac" | "wav" | "aiff") {
            args.extend(["-b:a", bitrate, "-ac", "2"]);
        }
        if ext == "opus" {
            args.extend(["-ar", "48000"]);
        }
        if ext == "wma" {
            args.extend(["-ar", "44100"]);
        }
    } else if VIDEO.contains(&ext) {
        args.extend([
            "-map",
            "0:V:0",
            "-map",
            "0:a:0?",
            "-sn",
            "-dn",
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-pix_fmt",
            "yuv420p",
        ]);
        match ext {
            "webm" => args.extend([
                "-c:v",
                "libvpx-vp9",
                "-crf",
                crf,
                "-b:v",
                "0",
                "-c:a",
                "libopus",
            ]),
            "avi" => args.extend(["-c:v", "mpeg4", "-q:v", "4", "-c:a", "libmp3lame"]),
            "wmv" => args.extend(["-c:v", "wmv2", "-q:v", "4", "-c:a", "wmav2"]),
            "mpeg" => args.extend([
                "-c:v",
                "mpeg2video",
                "-q:v",
                "4",
                "-c:a",
                "mp2",
                "-ar",
                "48000",
            ]),
            _ => args.extend([
                "-c:v", "libx264", "-preset", "medium", "-crf", crf, "-c:a", "aac",
            ]),
        }
        args.extend(["-b:a", bitrate, "-ac", "2"]);
        if matches!(ext, "mp4" | "mov") {
            args.extend(["-movflags", "+faststart"]);
        }
    } else if ext == "gif" {
        args.extend(["-an", "-filter_complex", "[0:V:0]fps=12,scale=480:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse", "-loop", "0"]);
    } else {
        return Err(Error::Invalid(format!("不支持的输出格式：{ext}")));
    }
    Ok(args.into_iter().map(String::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_extraction_and_optional_video_audio() {
        let audio = profile("mp3", Quality::Small).unwrap().join(" ");
        assert!(audio.contains("-map 0:a:0 -vn"));
        assert!(audio.contains("128k"));
        let video = profile("mp4", Quality::Balanced).unwrap().join(" ");
        assert!(video.contains("0:a:0?"));
        assert!(profile("xyz", Quality::High).is_err());
    }
}
