use mediaforge::images::{convert, Options};

#[test]
fn common_image_formats_roundtrip_and_resize() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("透明 图片.png");
    image::RgbaImage::from_pixel(64, 32, image::Rgba([90, 140, 210, 128]))
        .save(&input)
        .unwrap();
    for ext in [
        "png", "jpg", "webp", "bmp", "gif", "ico", "tiff", "qoi", "tga", "ppm", "pgm",
    ] {
        let out = dir.path().join(format!("result.{ext}"));
        convert(
            &input,
            &out,
            &Options {
                width: Some(32),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("{ext}: {e}"));
        let decoded = image::open(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (32, 16), "{ext}");
    }
}

#[test]
fn protects_original_existing_output_and_failed_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("original.png");
    image::RgbImage::new(8, 8).save(&input).unwrap();
    let original = std::fs::read(&input).unwrap();
    assert!(convert(
        &input,
        &input,
        &Options {
            overwrite: true,
            ..Default::default()
        }
    )
    .is_err());
    let output = dir.path().join("protected.jpg");
    std::fs::write(&output, b"keep me").unwrap();
    assert!(convert(&input, &output, &Options::default()).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"keep me");
    let broken = dir.path().join("broken.png");
    std::fs::write(&broken, b"not an image").unwrap();
    let absent = dir.path().join("absent.jpg");
    assert!(convert(&broken, &absent, &Options::default()).is_err());
    assert!(!absent.exists());
    assert_eq!(std::fs::read(&input).unwrap(), original);
    assert!(std::fs::read_dir(dir.path()).unwrap().all(|p| !p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".mediaforge-")));
}
