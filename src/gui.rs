use mediaforge::{
    batch::{self, Event},
    control::Control,
    media::{Engine, Quality},
};
use std::{
    cell::RefCell,
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::{atomic::Ordering, mpsc, Arc},
    thread,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::Dialogs::*, HiDpi::*, Input::KeyboardAndMouse::EnableWindow, Shell::*,
        WindowsAndMessaging::*,
    },
};

const PICK: usize = 101;
const START: usize = 102;
const CANCEL: usize = 103;
const OPEN: usize = 104;
const PIN: usize = 105;
const FORMAT: usize = 106;
const QUALITY: usize = 107;
const WIDTH: usize = 108;
const REPORT: usize = 109;
const STATUS: usize = 110;
const HEADER: usize = 111;
const CONCURRENCY: usize = 112;
const PAUSE: usize = 113;

struct Job {
    receiver: mpsc::Receiver<Event>,
    control: Arc<Control>,
    handle: thread::JoinHandle<()>,
}
struct Ui {
    files: Vec<PathBuf>,
    targets: Vec<&'static str>,
    font: HFONT,
    job: Option<Job>,
    report: String,
    folder: Option<PathBuf>,
}
thread_local! { static UI: RefCell<Option<Ui>> = const { RefCell::new(None) }; }
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn set(hwnd: HWND, id: usize, text: &str) {
    SetWindowTextW(GetDlgItem(hwnd, id as i32), w(text).as_ptr());
}
unsafe fn enable(hwnd: HWND, id: usize, yes: bool) {
    EnableWindow(GetDlgItem(hwnd, id as i32), yes as i32);
}
unsafe fn selection(hwnd: HWND, id: usize) -> usize {
    SendMessageW(GetDlgItem(hwnd, id as i32), CB_GETCURSEL, 0, 0) as usize
}

pub fn run() {
    unsafe {
        SetProcessDPIAware();
        let name = w("MediaForgeWindow");
        let instance = GetModuleHandleW(null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            lpszClassName: name.as_ptr(),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_ACCEPTFILES,
            name.as_ptr(),
            w("MediaForge · 媒体转换").as_ptr(),
            WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            500,
            460,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            MessageBoxW(
                null_mut(),
                w("无法创建窗口").as_ptr(),
                w("MediaForge").as_ptr(),
                MB_ICONERROR,
            );
            return;
        }
        ShowWindow(hwnd, SW_SHOW);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

unsafe fn setup(hwnd: HWND) -> Ui {
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let px = |v: i32| (v * dpi + 48) / 96;
    let font = CreateFontW(
        -px(14),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET.into(),
        0,
        0,
        5,
        0,
        w("Microsoft YaHei UI").as_ptr(),
    );
    let child = |class: &str, text: &str, style, x, y, width, height, id: usize| {
        let h = CreateWindowExW(
            0,
            w(class).as_ptr(),
            w(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            px(x),
            px(y),
            px(width),
            px(height),
            hwnd,
            id as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(h, WM_SETFONT, font as usize, 1);
        h
    };
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: px(456),
        bottom: px(470),
    };
    AdjustWindowRectEx(
        &mut rect,
        WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        0,
        WS_EX_TOPMOST | WS_EX_ACCEPTFILES,
    );
    SetWindowPos(
        hwnd,
        null_mut(),
        0,
        0,
        rect.right - rect.left,
        rect.bottom - rect.top,
        SWP_NOMOVE | SWP_NOZORDER,
    );
    child(
        "STATIC",
        "图片 / 音乐 / 视频，拖进来就能转",
        0,
        18,
        16,
        420,
        24,
        0,
    );
    child(
        "STATIC",
        "尚未选择文件 · 支持一次拖入多个文件",
        0x4000,
        18,
        49,
        420,
        24,
        HEADER,
    );
    child("BUTTON", "选择文件…", WS_TABSTOP, 18, 82, 115, 30, PICK);
    child(
        "BUTTON",
        "保持置顶",
        WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        326,
        82,
        110,
        30,
        PIN,
    );
    SendMessageW(GetDlgItem(hwnd, PIN as i32), BM_SETCHECK, 1, 0);
    child("STATIC", "转换为", 0, 18, 130, 64, 23, 0);
    child(
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_TABSTOP | WS_VSCROLL,
        88,
        125,
        350,
        350,
        FORMAT,
    );
    child("STATIC", "质量", 0, 18, 171, 64, 23, 0);
    let quality = child(
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_TABSTOP,
        88,
        166,
        150,
        160,
        QUALITY,
    );
    for text in ["较小文件", "推荐 · 均衡", "更高质量"] {
        SendMessageW(quality, CB_ADDSTRING, 0, w(text).as_ptr() as isize);
    }
    SendMessageW(quality, CB_SETCURSEL, 1, 0);
    let width = child(
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_TABSTOP,
        250,
        166,
        188,
        180,
        WIDTH,
    );
    for text in [
        "图片：保持尺寸",
        "图片：最宽 1920",
        "图片：最宽 1280",
        "图片：最宽 800",
    ] {
        SendMessageW(width, CB_ADDSTRING, 0, w(text).as_ptr() as isize);
    }
    SendMessageW(width, CB_SETCURSEL, 0, 0);
    child("STATIC", "并发", 0, 18, 211, 64, 23, 0);
    let concurrency = child(
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_TABSTOP,
        88,
        206,
        150,
        160,
        CONCURRENCY,
    );
    for text in ["1 个 · 节省资源", "2 个 · 推荐", "4 个 · 更快处理"] {
        SendMessageW(concurrency, CB_ADDSTRING, 0, w(text).as_ptr() as isize);
    }
    SendMessageW(concurrency, CB_SETCURSEL, 1, 0);
    child("BUTTON", "暂停", WS_TABSTOP, 294, 204, 144, 30, PAUSE);
    child(
        "BUTTON",
        "开始转换",
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        18,
        250,
        160,
        34,
        START,
    );
    child("BUTTON", "取消", WS_TABSTOP, 190, 250, 92, 34, CANCEL);
    child(
        "BUTTON",
        "打开输出目录",
        WS_TABSTOP,
        294,
        250,
        144,
        34,
        OPEN,
    );
    child(
        "STATIC",
        "输出到原文件旁的「MediaForge 输出」，保留原文件",
        0,
        18,
        296,
        420,
        22,
        STATUS,
    );
    child("EDIT", "支持 PNG、JPG、WebP、FLAC、MP3、MP4 等常见格式。\r\n拖入文件后，自动显示可用的目标格式。\r\n图片动图转静态图仅取首帧；视频转 GIF 保留动画。", WS_BORDER | WS_VSCROLL | WS_TABSTOP | ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32, 18, 325, 420, 125, REPORT);
    for id in [START, CANCEL, OPEN, FORMAT, WIDTH, PAUSE] {
        enable(hwnd, id, false);
    }
    Ui {
        files: vec![],
        targets: vec![],
        font,
        job: None,
        report: String::new(),
        folder: None,
    }
}

unsafe fn choose(hwnd: HWND) -> Vec<PathBuf> {
    let mut buffer = vec![0u16; 65536];
    let filter = w("媒体文件\0*.png;*.jpg;*.jpeg;*.webp;*.gif;*.bmp;*.ico;*.tif;*.tiff;*.pnm;*.ppm;*.pgm;*.pbm;*.pam;*.tga;*.qoi;*.flac;*.mp3;*.wav;*.m4a;*.aac;*.ogg;*.opus;*.wma;*.aiff;*.aif;*.ape;*.alac;*.m4b;*.mp4;*.mkv;*.mov;*.webm;*.avi;*.wmv;*.flv;*.mpeg;*.mpg;*.ts;*.m4v;*.mts;*.m2ts;*.3gp;*.vob\0所有文件\0*.*\0");
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFilter: filter.as_ptr(),
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        Flags: OFN_ALLOWMULTISELECT | OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if GetOpenFileNameW(&mut dialog) == 0 {
        return vec![];
    }
    let parts: Vec<PathBuf> = buffer
        .split(|v| *v == 0)
        .take_while(|part| !part.is_empty())
        .map(|p| PathBuf::from(OsString::from_wide(p)))
        .collect();
    if parts.len() <= 1 {
        parts
    } else {
        parts[1..].iter().map(|p| parts[0].join(p)).collect()
    }
}

unsafe fn select_files(hwnd: HWND, ui: &mut Ui, files: Vec<PathBuf>) {
    if files.is_empty() || ui.job.is_some() {
        return;
    }
    ui.targets = batch::targets(&files);
    ui.folder = None;
    enable(hwnd, OPEN, false);
    ui.files = files;
    let summary = if ui.files.len() == 1 {
        ui.files[0]
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        format!("已选择 {} 个文件", ui.files.len())
    };
    set(hwnd, HEADER, &summary);
    let combo = GetDlgItem(hwnd, FORMAT as i32);
    SendMessageW(combo, CB_RESETCONTENT, 0, 0);
    let is_image = ui
        .files
        .iter()
        .all(|p| batch::kind(p) == Some(batch::Kind::Image));
    let is_video = ui
        .files
        .iter()
        .all(|p| batch::kind(p) == Some(batch::Kind::Video));
    for ext in &ui.targets {
        let description = if is_video && mediaforge::media::AUDIO.contains(ext) {
            " · 提取音频"
        } else if is_video && *ext == "gif" {
            " · 动画（宽 480 / 12 帧）"
        } else if is_image && *ext == "gif" {
            " · 单帧图片"
        } else if *ext == "webp" {
            " · 无损图片"
        } else {
            ""
        };
        SendMessageW(
            combo,
            CB_ADDSTRING,
            0,
            w(&format!("{}{description}", ext.to_uppercase())).as_ptr() as isize,
        );
    }
    SendMessageW(combo, CB_SETCURSEL, 0, 0);
    enable(hwnd, FORMAT, !ui.targets.is_empty());
    enable(hwnd, START, !ui.targets.is_empty());
    enable(hwnd, WIDTH, is_image);
    set(
        hwnd,
        STATUS,
        if ui.targets.is_empty() {
            "格式不支持或类型混合，请分别选择图片和音视频。"
        } else {
            "输出到原文件旁的「MediaForge 输出」，重名自动编号"
        },
    );
    ui.report = ui
        .files
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\r\n");
    set(hwnd, REPORT, &ui.report);
}

unsafe fn start(hwnd: HWND, ui: &mut Ui) {
    if ui.job.is_some() {
        return;
    }
    let Some(&target) = ui.targets.get(selection(hwnd, FORMAT)) else {
        return;
    };
    let quality = match selection(hwnd, QUALITY) {
        0 => Quality::Small,
        2 => Quality::High,
        _ => Quality::Balanced,
    };
    let width = match selection(hwnd, WIDTH) {
        1 => Some(1920),
        2 => Some(1280),
        3 => Some(800),
        _ => None,
    };
    let files = ui.files.clone();
    let concurrency = match selection(hwnd, CONCURRENCY) {
        0 => 1,
        2 => 4,
        _ => 2,
    };
    let (tx, receiver) = mpsc::channel();
    let control = Arc::new(Control::default());
    let flag = control.clone();
    let handle = thread::spawn(move || {
        let engine = match Engine::bundled() {
            Ok(e) => e,
            Err(e) => {
                let _ = tx.send(Event::Done(e.to_string()));
                return;
            }
        };
        batch::run(
            &engine,
            &files,
            &batch::BatchOptions {
                target: target.into(),
                quality,
                max_width: width,
                concurrency,
            },
            &flag,
            &tx,
        );
    });
    ui.job = Some(Job {
        receiver,
        control,
        handle,
    });
    ui.report.clear();
    ui.folder = None;
    set(hwnd, REPORT, "正在处理，请稍候…");
    for id in [PICK, START, FORMAT, QUALITY, WIDTH, OPEN, CONCURRENCY] {
        enable(hwnd, id, false);
    }
    enable(hwnd, CANCEL, true);
    enable(hwnd, PAUSE, true);
    set(hwnd, PAUSE, "暂停");
    SetTimer(hwnd, 1, 100, None);
}

unsafe fn poll(hwnd: HWND, ui: &mut Ui) {
    let Some(job) = &ui.job else {
        return;
    };
    let mut events = Vec::new();
    let mut disconnected = false;
    loop {
        match job.receiver.try_recv() {
            Ok(event) => events.push(event),
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                disconnected = true;
                break;
            }
        }
    }
    let mut complete = false;
    for event in events {
        match event {
            Event::Progress(message) => {
                if !ui
                    .job
                    .as_ref()
                    .unwrap()
                    .control
                    .paused
                    .load(Ordering::SeqCst)
                {
                    set(hwnd, STATUS, &message);
                }
            }
            Event::File(message, folder) => {
                ui.report.push_str(&message);
                ui.report.push_str("\r\n\r\n");
                if let Some(folder) = folder {
                    ui.folder = Some(folder);
                }
                set(hwnd, REPORT, &ui.report);
            }
            Event::Done(message) => {
                set(hwnd, STATUS, &message);
                ui.report.push_str(&message);
                set(hwnd, REPORT, &ui.report);
                complete = true;
            }
        }
    }
    if complete || disconnected {
        if let Some(job) = ui.job.take() {
            if job.handle.join().is_err() {
                set(hwnd, STATUS, "转换线程异常结束，请重新尝试。");
            }
        }
        KillTimer(hwnd, 1);
        for id in [PICK, START, FORMAT, QUALITY, CONCURRENCY] {
            enable(hwnd, id, true);
        }
        enable(
            hwnd,
            WIDTH,
            ui.files
                .iter()
                .all(|p| batch::kind(p) == Some(batch::Kind::Image)),
        );
        enable(hwnd, CANCEL, false);
        enable(hwnd, PAUSE, false);
        set(hwnd, PAUSE, "暂停");
        enable(hwnd, OPEN, ui.folder.is_some());
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        WM_CREATE => {
            let ui = setup(hwnd);
            UI.with(|s| *s.borrow_mut() = Some(ui));
        }
        WM_DROPFILES => {
            let drop = wp as HDROP;
            let mut files = Vec::new();
            for i in 0..DragQueryFileW(drop, u32::MAX, null_mut(), 0) {
                let mut buf = vec![0; DragQueryFileW(drop, i, null_mut(), 0) as usize + 1];
                let len = DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32);
                files.push(PathBuf::from(OsString::from_wide(&buf[..len as usize])));
            }
            DragFinish(drop);
            UI.with(|s| {
                if let Some(ui) = s.borrow_mut().as_mut() {
                    select_files(hwnd, ui, files);
                }
            });
        }
        WM_COMMAND if wp & 0xffff == PICK && wp >> 16 == 0 => {
            let files = choose(hwnd);
            UI.with(|s| {
                if let Some(ui) = s.borrow_mut().as_mut() {
                    select_files(hwnd, ui, files);
                }
            });
        }
        WM_COMMAND if wp & 0xffff == START && wp >> 16 == 0 => UI.with(|s| {
            if let Some(ui) = s.borrow_mut().as_mut() {
                start(hwnd, ui);
            }
        }),
        WM_COMMAND if wp & 0xffff == CANCEL && wp >> 16 == 0 => UI.with(|s| {
            if let Some(ui) = s.borrow().as_ref() {
                if let Some(job) = &ui.job {
                    job.control.cancelled.store(true, Ordering::SeqCst);
                    set(hwnd, STATUS, "正在取消…图片任务将在当前文件完成后停止。");
                    enable(hwnd, CANCEL, false);
                    enable(hwnd, PAUSE, false);
                }
            }
        }),
        WM_COMMAND if wp & 0xffff == PAUSE && wp >> 16 == 0 => UI.with(|s| {
            if let Some(job) = s.borrow().as_ref().and_then(|u| u.job.as_ref()) {
                let paused = !job.control.paused.fetch_xor(true, Ordering::SeqCst);
                set(hwnd, PAUSE, if paused { "继续" } else { "暂停" });
                set(
                    hwnd,
                    STATUS,
                    if paused {
                        "暂停请求已发送 · 点击“继续”恢复"
                    } else {
                        "正在继续转换…"
                    },
                );
            }
        }),
        WM_COMMAND if wp & 0xffff == OPEN && wp >> 16 == 0 => {
            let folder = UI.with(|s| s.borrow().as_ref().and_then(|u| u.folder.clone()));
            if let Some(folder) = folder {
                use std::os::windows::ffi::OsStrExt;
                let path: Vec<_> = folder.as_os_str().encode_wide().chain(Some(0)).collect();
                ShellExecuteW(
                    hwnd,
                    w("open").as_ptr(),
                    path.as_ptr(),
                    null(),
                    null(),
                    SW_SHOWNORMAL,
                );
            }
        }
        WM_COMMAND if wp & 0xffff == PIN && wp >> 16 == 0 => {
            let pinned = SendMessageW(GetDlgItem(hwnd, PIN as i32), BM_GETCHECK, 0, 0) != 0;
            SetWindowPos(
                hwnd,
                if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            );
        }
        WM_TIMER => UI.with(|s| {
            if let Some(ui) = s.borrow_mut().as_mut() {
                poll(hwnd, ui);
            }
        }),
        WM_CLOSE => {
            let busy = UI.with(|s| s.borrow().as_ref().is_some_and(|u| u.job.is_some()));
            if busy {
                MessageBoxW(
                    hwnd,
                    w("转换正在进行。如需退出，请先点“取消”，待当前任务停止后关闭。").as_ptr(),
                    w("MediaForge").as_ptr(),
                    MB_OK,
                );
            } else {
                DestroyWindow(hwnd);
            }
        }
        WM_DESTROY => {
            UI.with(|s| {
                if let Some(ui) = s.borrow_mut().take() {
                    DeleteObject(ui.font);
                }
            });
            PostQuitMessage(0);
        }
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    0
}
