use mediaforge::{
    batch,
    media::{Engine, Quality, AUDIO, VIDEO},
};
use std::{path::PathBuf, process::Command, sync::atomic::AtomicBool};

fn engine() -> Engine {
    Engine {
        threads: 2,
        ffmpeg: PathBuf::from(
            std::env::var_os("MEDIAFORGE_TEST_FFMPEG")
                .expect("Set MEDIAFORGE_TEST_FFMPEG to bundled ffmpeg.exe"),
        ),
    }
}
fn ffmpeg(e: &Engine, args: &[&std::ffi::OsStr]) {
    let out = Command::new(&e.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
#[ignore = "requires packaged FFmpeg; run with MEDIAFORGE_TEST_FFMPEG"]
fn audio_video_conversion_matrix() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("测试 片段.mp4");
    ffmpeg(
        &e,
        &[
            "-f".as_ref(),
            "lavfi".as_ref(),
            "-i".as_ref(),
            "testsrc2=size=64x48:rate=25:duration=0.4".as_ref(),
            "-f".as_ref(),
            "lavfi".as_ref(),
            "-i".as_ref(),
            "sine=frequency=440:duration=0.4".as_ref(),
            "-c:v".as_ref(),
            "libx264".as_ref(),
            "-c:a".as_ref(),
            "aac".as_ref(),
            "-shortest".as_ref(),
            source.as_os_str(),
        ],
    );
    let cancel = AtomicBool::new(false);
    let flac = batch::convert(&e, &source, "flac", Quality::Balanced, None, &cancel).unwrap();
    for ext in AUDIO {
        let out = batch::convert(&e, &flac, ext, Quality::Balanced, None, &cancel)
            .unwrap_or_else(|err| panic!("{ext}: {err}"));
        ffmpeg(
            &e,
            &[
                "-i".as_ref(),
                out.as_os_str(),
                "-f".as_ref(),
                "null".as_ref(),
                "-".as_ref(),
            ],
        );
        // Lossy-to-lossy input decoding is verified as well.
        let back = dir.path().join(format!("back-{ext}.wav"));
        e.convert(&out, &back, Quality::Small, &cancel).unwrap();
    }
    for ext in VIDEO.iter().chain(["gif"].iter()) {
        let out = batch::convert(&e, &source, ext, Quality::Balanced, None, &cancel)
            .unwrap_or_else(|err| panic!("{ext}: {err}"));
        ffmpeg(
            &e,
            &[
                "-i".as_ref(),
                out.as_os_str(),
                "-f".as_ref(),
                "null".as_ref(),
                "-".as_ref(),
            ],
        );
    }
    let again = batch::convert(&e, &source, "flac", Quality::High, None, &cancel).unwrap();
    assert_ne!(again, flac);
    assert!(flac.exists());
}

#[test]
#[ignore = "requires packaged FFmpeg"]
fn missing_audio_failure_and_cancellation_leave_no_partial_output() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("silent.mp4");
    ffmpeg(
        &e,
        &[
            "-f".as_ref(),
            "lavfi".as_ref(),
            "-i".as_ref(),
            "testsrc2=size=64x48:duration=0.2".as_ref(),
            source.as_os_str(),
        ],
    );
    let output = dir.path().join("missing.mp3");
    assert!(e
        .convert(&source, &output, Quality::Balanced, &AtomicBool::new(false))
        .is_err());
    assert!(!output.exists());
    let video = dir.path().join("silent.mkv");
    e.convert(&source, &video, Quality::Balanced, &AtomicBool::new(false))
        .unwrap();
    let cancelled = dir.path().join("cancelled.mp4");
    assert!(e
        .convert(
            &source,
            &cancelled,
            Quality::Balanced,
            &AtomicBool::new(true)
        )
        .is_err());
    assert!(!cancelled.exists());
    assert!(std::fs::read_dir(dir.path()).unwrap().all(|p| !p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".mediaforge-")));
}

#[test]
#[ignore = "requires packaged FFmpeg"]
fn cancel_an_inflight_paused_conversion() {
    use mediaforge::control::Control;
    use std::{
        sync::{atomic::Ordering, Arc},
        thread,
        time::{Duration, Instant},
    };
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("long.mp4");
    ffmpeg(
        &e,
        &[
            "-f".as_ref(),
            "lavfi".as_ref(),
            "-i".as_ref(),
            "testsrc2=size=640x360:rate=30:duration=20".as_ref(),
            "-c:v".as_ref(),
            "libx264".as_ref(),
            "-preset".as_ref(),
            "ultrafast".as_ref(),
            source.as_os_str(),
        ],
    );
    let output = dir.path().join("cancelled.webm");
    let control = Arc::new(Control::default());
    let other = control.clone();
    let target = output.clone();
    let job = thread::spawn(move || {
        e.convert_controlled(&source, &target, Quality::High, &other, &other.cancelled)
    });
    let start = Instant::now();
    while !std::fs::read_dir(dir.path()).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".mediaforge-")
    }) {
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(200));
    control.paused.store(true, Ordering::SeqCst);
    thread::sleep(Duration::from_millis(800));
    assert!(!job.is_finished());
    control.cancelled.store(true, Ordering::SeqCst);
    let stopped = Instant::now();
    assert!(job.join().unwrap().is_err());
    assert!(stopped.elapsed() < Duration::from_secs(3));
    assert!(!output.exists());
    assert!(std::fs::read_dir(dir.path()).unwrap().all(|p| !p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".mediaforge-")));
}
