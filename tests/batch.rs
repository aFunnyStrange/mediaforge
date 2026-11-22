use mediaforge::{
    batch::{self, BatchOptions, Event},
    control::Control,
    media::{Engine, Quality},
};
use std::sync::{atomic::Ordering, mpsc};

#[test]
fn concurrent_same_stems_are_unique_and_failure_does_not_stop_queue() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("同名.png");
    let bmp = dir.path().join("同名.bmp");
    let broken = dir.path().join("损坏.png");
    image::RgbImage::new(128, 64).save(&png).unwrap();
    image::RgbImage::new(128, 64).save(&bmp).unwrap();
    std::fs::write(&broken, b"broken").unwrap();
    let (tx, rx) = mpsc::channel();
    batch::run(
        &Engine {
            ffmpeg: "missing.exe".into(),
            threads: 1,
        },
        &[png, bmp, broken],
        &BatchOptions {
            target: "jpg".into(),
            quality: Quality::Balanced,
            max_width: None,
            concurrency: 4,
        },
        &Control::default(),
        &tx,
    );
    assert!(dir.path().join("MediaForge 输出/同名.jpg").is_file());
    assert!(dir.path().join("MediaForge 输出/同名 (1).jpg").is_file());
    let events: Vec<_> = rx.try_iter().collect();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Done(s) if s.contains("成功 2 个，失败 1 个"))));
}

#[test]
fn paused_queue_can_be_cancelled_without_starting_work() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("image.png");
    image::RgbImage::new(16, 16).save(&input).unwrap();
    let control = Control::default();
    control.paused.store(true, Ordering::SeqCst);
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let job = scope.spawn(|| {
            batch::run(
                &Engine {
                    ffmpeg: "missing.exe".into(),
                    threads: 1,
                },
                &[input],
                &BatchOptions {
                    target: "jpg".into(),
                    quality: Quality::Balanced,
                    max_width: None,
                    concurrency: 2,
                },
                &control,
                &tx,
            )
        });
        assert!(rx
            .recv_timeout(std::time::Duration::from_millis(200))
            .is_err());
        assert!(!dir.path().join("MediaForge 输出").exists());
        control.cancelled.store(true, Ordering::SeqCst);
        job.join().unwrap();
    });
    assert!(rx
        .try_iter()
        .any(|e| matches!(e, Event::Done(s) if s.contains("取消/未处理 1 个"))));
}
