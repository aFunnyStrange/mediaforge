//! Shared workflow used by the GUI and end-to-end tests.
use crate::{
    images,
    media::{self, Engine, Quality},
    Error, Result,
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Image,
    Audio,
    Video,
}
pub fn kind(path: &Path) -> Option<Kind> {
    let ext = crate::extension(path).ok()?;
    if images::FORMATS.contains(&ext.as_str()) {
        Some(Kind::Image)
    } else if media::AUDIO.contains(&ext.as_str())
        || ["aif", "alac", "ape", "m4b"].contains(&ext.as_str())
    {
        Some(Kind::Audio)
    } else if media::VIDEO.contains(&ext.as_str())
        || ["m4v", "mpg", "mts", "m2ts", "3gp", "vob"].contains(&ext.as_str())
    {
        Some(Kind::Video)
    } else {
        None
    }
}
pub fn targets(files: &[PathBuf]) -> Vec<&'static str> {
    if files.is_empty() || files.iter().any(|f| !f.is_file() || kind(f).is_none()) {
        return vec![];
    }
    if files.iter().all(|f| kind(f) == Some(Kind::Image)) {
        return vec![
            "png", "jpg", "webp", "bmp", "tiff", "gif", "ico", "qoi", "tga", "ppm", "pgm",
        ];
    }
    if files.iter().any(|f| kind(f) == Some(Kind::Image)) {
        return vec![];
    }
    let mut result = media::AUDIO.to_vec();
    if files.iter().all(|f| kind(f) == Some(Kind::Video)) {
        result.splice(0..0, media::VIDEO.iter().copied().chain(["gif"]));
    }
    result
}

/// Output stays beside the input in a dedicated folder. Never replace an existing file.
pub fn destination(input: &Path, ext: &str) -> Result<PathBuf> {
    destination_reserved(input, ext, &std::collections::HashSet::new())
}
fn destination_reserved(
    input: &Path,
    ext: &str,
    reserved: &std::collections::HashSet<String>,
) -> Result<PathBuf> {
    if !images::FORMATS.contains(&ext)
        && !media::AUDIO.contains(&ext)
        && !media::VIDEO.contains(&ext)
    {
        return Err(Error::Invalid("不支持的格式".into()));
    }
    let folder = input
        .parent()
        .unwrap_or(Path::new("."))
        .join("MediaForge 输出");
    let stem = input
        .file_stem()
        .ok_or_else(|| Error::Invalid("无效的文件名".into()))?;
    for i in 0..100_000 {
        let mut name = stem.to_os_string();
        if i > 0 {
            name.push(format!(" ({i})"));
        }
        name.push(format!(".{ext}"));
        let path = folder.join(name);
        if !path.exists() && !reserved.contains(&path.to_string_lossy().to_lowercase()) {
            return Ok(path);
        }
    }
    Err(Error::Invalid("输出文件重名过多".into()))
}

pub fn convert(
    engine: &Engine,
    input: &Path,
    ext: &str,
    quality: Quality,
    max_width: Option<u32>,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Invalid("已取消".into()));
    }
    if !targets(&[input.to_owned()]).contains(&ext) {
        return Err(Error::Invalid("该文件不能转换为所选格式".into()));
    }
    let output = destination(input, ext)?;
    convert_to(
        engine,
        input,
        &output,
        quality,
        max_width,
        &crate::control::Control::default(),
        cancel,
    )?;
    Ok(output)
}

fn convert_to(
    engine: &Engine,
    input: &Path,
    output: &Path,
    quality: Quality,
    max_width: Option<u32>,
    control: &crate::control::Control,
    cancel: &AtomicBool,
) -> Result<()> {
    if kind(input) == Some(Kind::Image) {
        let options = images::Options {
            quality: match quality {
                Quality::Small => 70,
                Quality::Balanced => 85,
                Quality::High => 95,
            },
            width: max_width,
            ..Default::default()
        };
        images::convert(input, output, &options)?;
    } else {
        engine.convert_controlled(input, output, quality, control, cancel)?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum Event {
    Progress(String),
    File(String, Option<PathBuf>),
    Done(String),
}
pub struct BatchOptions {
    pub target: String,
    pub quality: Quality,
    pub max_width: Option<u32>,
    pub concurrency: usize,
}

/// Bounded workers; one owned child process per active media task. All workers join before Done.
pub fn run(
    engine: &Engine,
    files: &[PathBuf],
    options: &BatchOptions,
    control: &crate::control::Control,
    events: &std::sync::mpsc::Sender<Event>,
) {
    use std::sync::atomic::AtomicUsize;
    let workers = options.concurrency.clamp(1, 4).min(files.len());
    let mut reserved = std::collections::HashSet::new();
    let plans: Vec<_> = files
        .iter()
        .map(|input| {
            let result = if targets(&[input.to_owned()]).contains(&options.target.as_str()) {
                destination_reserved(input, &options.target, &reserved)
            } else {
                Err(Error::Invalid("该文件不能转换为所选格式".into()))
            };
            if let Ok(output) = &result {
                reserved.insert(output.to_string_lossy().to_lowercase());
            }
            result
        })
        .collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let mut limited = engine.clone();
    limited.threads = (std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2)
        / workers.max(1))
    .clamp(1, 8);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while control.wait_ready() {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(input) = files.get(index) else {
                        break;
                    };
                    let _ = events.send(Event::Progress(format!(
                        "已处理 {}/{} · 转换中（最多 {} 个并发）",
                        done.load(Ordering::SeqCst) + failed.load(Ordering::SeqCst),
                        files.len(),
                        workers
                    )));
                    let result = match &plans[index] {
                        Ok(output) => convert_to(
                            &limited,
                            input,
                            output,
                            options.quality,
                            options.max_width,
                            control,
                            &control.cancelled,
                        )
                        .map(|()| output),
                        Err(error) => Err(Error::Invalid(error.to_string())),
                    };
                    match result {
                        Ok(output) => {
                            done.fetch_add(1, Ordering::SeqCst);
                            let _ = events.send(Event::File(
                                format!("✓ {}", output.display()),
                                output.parent().map(PathBuf::from),
                            ));
                        }
                        Err(error) => {
                            if !control.cancelled.load(Ordering::SeqCst) {
                                failed.fetch_add(1, Ordering::SeqCst);
                            }
                            let _ = events.send(Event::File(
                                format!("× {}\r\n  {error}", input.display()),
                                None,
                            ));
                        }
                    }
                }
            });
        }
    });
    let done = done.load(Ordering::SeqCst);
    let failed = failed.load(Ordering::SeqCst);
    let _ = events.send(Event::Done(format!(
        "成功 {done} 个，失败 {failed} 个，取消/未处理 {} 个",
        files.len() - done - failed
    )));
}
