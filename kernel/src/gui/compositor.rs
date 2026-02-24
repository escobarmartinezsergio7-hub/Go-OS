use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use miniz_oxide::inflate::{decompress_to_vec_with_limit, decompress_to_vec_zlib_with_limit};
use sha2::{Digest, Sha256};

use super::window::{
    ExplorerItem, ExplorerItemKind, NotepadClickAction, Window, WindowState, WINDOW_RESIZE_GRIP,
    WINDOW_TITLE_BAR_H,
};
use super::widgets::{taskbar::Taskbar, Widget};
use super::{Color, Event, Point, Rect, SpecialKey};
use crate::framebuffer;
use crate::fs::FileSystem;
use uefi::runtime::ResetType;
use uefi::Status;

const START_MENU_X: i32 = 5;
const START_MENU_W: u32 = 200;
const START_MENU_ITEM_H: u32 = 24;
const START_MENU_PADDING: i32 = 8;
const START_MENU_ITEMS: usize = 8;

const TOOLS_MENU_W: u32 = 180;
const TOOLS_MENU_PADDING: i32 = 8;
const TOOLS_MENU_ITEMS: usize = 2;
const GAMES_MENU_W: u32 = 180;
const GAMES_MENU_PADDING: i32 = 8;
const GAMES_MENU_ITEMS: usize = 1;
const APPS_MENU_W: u32 = 220;
const APPS_MENU_PADDING: i32 = 8;
const APPS_MENU_MAX_ITEMS: usize = 10;

const DOUBLE_CLICK_MIN_TICKS: u64 = 4;
const DOUBLE_CLICK_TICKS: u64 = 25;
const WINDOW_MIN_FALLBACK_W: u32 = 360;
const WINDOW_MIN_FALLBACK_H: u32 = 240;
const NOTEPAD_MAX_TEXT_BYTES: usize = 32 * 1024;
const RUBY_SCRIPT_MAX_BYTES: usize = 64 * 1024;
const FETCH_MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
const INSTALL_MAX_PACKAGE_BYTES: usize = 256 * 1024 * 1024;
const INSTALL_MAX_DEB_PACKAGE_BYTES: usize = 256 * 1024 * 1024;
const INSTALL_MAX_EXPANDED_FILE_BYTES: usize = 256 * 1024 * 1024;
const INSTALL_MAX_SIGNATURE_BYTES: usize = 32 * 1024;
const INSTALL_DEB_MAX_INFLATED_BYTES: usize = 512 * 1024 * 1024;
const INSTALL_DEB_PREFLIGHT_BYTES: usize = 128 * 1024;
const INSTALL_MIN_TASK_BUDGET_BYTES: usize = 16 * 1024 * 1024;
const INSTALL_TASK_BUDGET_DIVISOR: usize = 2;
const INSTALL_HEAP_HEADROOM_BYTES: usize = 32 * 1024 * 1024;
const INSTALL_UI_PUMP_EVERY_FILES: usize = 32;
const INSTALL_PROGRESS_LOG_EVERY_FILES: usize = 2048;
const INSTALL_VERBOSE_DEBUG: bool = false;
const LINUX_UI_PUMP_EVERY_ITEMS: usize = 16;
const LINUX_PROGRESS_LOG_EVERY_ITEMS: usize = 1024;
const LINUX_RUNTIME_LOOKUP_MAX_CLUSTERS: usize = 4;
const LINUX_RUNTIME_LOOKUP_TICK_BUDGET: u64 = 32;
const LINUX_RUNTIME_LOOKUP_MAX_MANIFESTS: usize = 2;
const LINUX_RUNTIME_LOOKUP_MAX_MAP_ENTRIES: usize = 4096;
const LINUX_RUNLOOP_MAX_STEPS: usize = 64;
const LINUX_RUNLOOP_SLICE_BUDGET: usize = 48;
const LINUX_RUNLOOP_DEP_FILES_PER_STEP: usize = 1;
const LINUX_RUNLOOP_PATHS_PER_STEP: usize = 8;
const LINUX_STEP_CMD_TICK_BUDGET: u64 = 24;
const LINUX_RUNLOOP_CMD_TICK_BUDGET: u64 = 24;
const LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES: usize = 256 * 1024 * 1024;
const LINUX_RUNLOOP_BLOB_TOTAL_BUDGET_BYTES: u64 = 512 * 1024 * 1024;
// Runloop executes Linux ELF in real syscall timeslices and returns to GUI between slices.
// startx/startmx remain aliases for compatibility.
const LINUX_RUNLOOP_REAL_TRANSFER_AUTO_TIMEOUT_GUARD: bool = true;
// Hard safety: without IRQ-preemptable desktop timer, real-slice can trap GUI
// if guest code runs too long before first syscall.
const LINUX_RUNLOOP_REQUIRE_IRQ_FOR_REAL_SLICE: bool = true;
// If guarded real-transfer mode makes no syscall progress for this many slices, abort safely.
const LINUX_RUNLOOP_GUARDED_STALL_TIMEOUT_SLICES: u64 = 2048;
// Keep terminal diagnostics sparse to avoid UI stalls on real hardware.
const LINUX_RUNLOOP_PROGRESS_EVERY_SLICES: u64 = 256;
// Rendering every slice is expensive; cadence keeps Linux bridge fluid without saturating paint loop.
const LINUX_RUNLOOP_BRIDGE_RENDER_EVERY_SLICES: u64 = 16;
const LINUX_RUNLOOP_E2E_MIN_CONNECTED_STREAK: u64 = 64;
const LINUX_RUNLOOP_E2E_MIN_READY_STREAK: u64 = 64;
const LINUX_RUNLOOP_E2E_MIN_FRAME_ADVANCES: u64 = 3;
const LINUX_RUNLOOP_E2E_MAX_RECENT_FRAME_GAP_SLICES: u64 = 192;
const LINUX_RUNLOOP_E2E_CONNECT_TIMEOUT_SLICES: u64 = 4096;
const LINUX_RUNLOOP_E2E_FRAME_TIMEOUT_SLICES: u64 = 512;
const LINUX_RUNLOOP_E2E_POST_VALIDATE_GRACE_SLICES: u64 = 128;
const LINUX_RUNLOOP_E2E_POST_VALIDATE_UNREADY_TIMEOUT_SLICES: u64 = 384;
const LINUX_RUNLOOP_E2E_POST_VALIDATE_FRAME_GAP_TIMEOUT_SLICES: u64 = 768;
const LINUX_RUNLOOP_SYMBOL_TRACE_PREVIEW_MAX: usize = 16;
const LINUX_BRIDGE_DEFAULT_WIDTH: u32 = 800;
const LINUX_BRIDGE_DEFAULT_HEIGHT: u32 = 450;
const COPY_MAX_FILE_BYTES: usize = 256 * 1024 * 1024;
const APP_RUNNER_MAX_LAYOUT_BYTES: usize = 64 * 1024;
const IMAGE_VIEWER_MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
const IMAGE_VIEWER_MAX_INFLATED_BYTES: usize = 32 * 1024 * 1024;
const IMAGE_VIEWER_MAX_PIXELS: usize = 4_000_000;
const DESKTOP_USB_ICON_W: u32 = 112;
const DESKTOP_USB_ICON_H: u32 = 92;
const DESKTOP_USB_MENU_W: u32 = 160;
const DESKTOP_USB_MENU_ITEM_H: u32 = 24;
const DESKTOP_USB_MENU_ITEMS: usize = 2;
const DESKTOP_USB_PROBE_INTERVAL_TICKS: u64 = 20;
const DESKTOP_ITEMS_START_X: i32 = 18;
const DESKTOP_ITEMS_START_Y: i32 = 126;
const DESKTOP_ITEM_W: u32 = 104;
const DESKTOP_ITEM_H: u32 = 92;
const DESKTOP_ITEM_GAP_X: i32 = 14;
const DESKTOP_ITEM_GAP_Y: i32 = 10;
const DESKTOP_ITEMS_MAX: usize = 48;
const DESKTOP_STATUS_MAX_CHARS: usize = 86;
const DESKTOP_CREATE_PROMPT_W: u32 = 360;
const DESKTOP_CREATE_PROMPT_H: u32 = 160;
const DESKTOP_CREATE_PROMPT_BUTTON_W: u32 = 78;
const DESKTOP_CREATE_PROMPT_BUTTON_H: u32 = 24;
const DESKTOP_CREATE_PROMPT_BUTTON_GAP: i32 = 10;
const RENAME_PROMPT_INPUT_MAX_CHARS: usize = 28;
const DESKTOP_DRAG_OPEN_THRESHOLD: i32 = 5;
const COPY_PROGRESS_PROMPT_W: u32 = 420;
const COPY_PROGRESS_PROMPT_H: u32 = 176;
const COPY_PROGRESS_PROMPT_BUTTON_W: u32 = 88;
const COPY_PROGRESS_PROMPT_BUTTON_H: u32 = 26;
const COPY_PROGRESS_PROMPT_HEADER_BUTTON_W: u32 = 22;
const COPY_PROGRESS_PROMPT_HEADER_BUTTON_H: u32 = 14;
const COPY_PROGRESS_PROMPT_MINI_W: u32 = 340;
const COPY_PROGRESS_PROMPT_MINI_H: u32 = 62;
const COPY_PROGRESS_PROMPT_MINI_BUTTON_W: u32 = 72;
const COPY_PROGRESS_PROMPT_MINI_BUTTON_H: u32 = 22;
const COPY_PROGRESS_PAINT_INTERVAL_TICKS: u64 = 4;
const COPY_PROGRESS_INPUT_POLL_INTERVAL_TICKS: u64 = 1;
const COPY_BACKGROUND_MAX_ITEMS_PER_PAINT: usize = 2;
const COPY_BACKGROUND_BUDGET_TICKS: u64 = 4;
const NOTEPAD_SAVE_PROMPT_W: u32 = 420;
const NOTEPAD_SAVE_PROMPT_H: u32 = 244;
const NOTEPAD_SAVE_PROMPT_BUTTON_W: u32 = 78;
const NOTEPAD_SAVE_PROMPT_BUTTON_H: u32 = 24;
const NOTEPAD_SAVE_PROMPT_BUTTON_GAP: i32 = 10;
const NOTEPAD_SAVE_PROMPT_ITEM_H: u32 = 20;
const EXPLORER_CONTEXT_MENU_W: u32 = 172;
const EXPLORER_CONTEXT_MENU_ITEM_H: u32 = 24;
const EXPLORER_CONTEXT_MENU_PADDING: i32 = 4;
const WEB_PROXY_DEFAULT_BASE: &str = "auto";
const WEB_PROXY_FALLBACK_BASE: &str = "http://10.0.2.2:37810";
const WEB_PROXY_DEFAULT_PORT: u16 = 37810;
const WEB_PROXY_ALT_PORT: u16 = 37820;
const WEB_PROXY_PROBE_TIMEOUT_TICKS: u64 = 250;
const WEB_PROXY_FRAME_TIMEOUT_TICKS: u64 = 1200;
const WEB_CEF_FRAME_MAX_PIXELS: usize = 1024 * 1024;
const WEB_CEF_BRIDGE_ENABLED: bool = true;

struct ExplorerClickState {
    win_id: usize,
    kind: ExplorerItemKind,
    cluster: u32,
    label: String,
    tick: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExplorerClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone)]
struct ExplorerClipboardItem {
    source_device_index: Option<usize>,
    source_dir_cluster: u32,
    source_dir_path: String,
    source_item_cluster: u32,
    source_is_directory: bool,
    source_label: String,
}

#[derive(Clone)]
struct ExplorerClipboardState {
    mode: ExplorerClipboardMode,
    source_device_index: Option<usize>,
    source_dir_cluster: u32,
    source_dir_path: String,
    source_item_cluster: u32,
    source_is_directory: bool,
    source_label: String,
    items: Vec<ExplorerClipboardItem>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExplorerContextMenuKind {
    FileItem,
    DirectoryItem,
    PasteArea,
    DesktopArea,
}

#[derive(Clone)]
struct ExplorerContextMenuState {
    win_id: usize,
    kind: ExplorerContextMenuKind,
    x: i32,
    y: i32,
    source_dir_cluster: u32,
    target_item: Option<ExplorerItem>,
    show_paste: bool,
    selection_count: usize,
}

#[derive(Clone)]
struct DesktopSelectionItem {
    source_dir_cluster: u32,
    cluster: u32,
    label: String,
    kind: ExplorerItemKind,
    size: u32,
}

#[derive(Clone)]
struct ExplorerSelectionItem {
    win_id: usize,
    source_dir_cluster: u32,
    cluster: u32,
    label: String,
}

#[derive(Clone)]
struct DesktopIconPosition {
    cluster: u32,
    label: String,
    x: i32,
    y: i32,
}

#[derive(Clone)]
struct DesktopDragState {
    source_dir_cluster: u32,
    source_dir_path: String,
    item: ExplorerItem,
    cluster: u32,
    label: String,
    offset_x: i32,
    offset_y: i32,
    start_mouse_x: i32,
    start_mouse_y: i32,
    moved: bool,
    open_on_release: bool,
}

#[derive(Clone)]
struct DesktopCreateFolderState {
    dir_cluster: u32,
    dir_path: String,
    input: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RenamePromptOrigin {
    ExplorerWindow(usize),
    Desktop,
}

#[derive(Clone)]
struct RenamePromptState {
    origin: RenamePromptOrigin,
    source_dir_cluster: u32,
    source_dir_path: String,
    source_device_index: Option<usize>,
    source_item_cluster: u32,
    source_label: String,
    source_is_directory: bool,
    input: String,
}

#[derive(Clone)]
struct CopyProgressPromptState {
    title: String,
    detail: String,
    percent: u8,
    done_units: usize,
    total_units: usize,
    done_items: usize,
    total_items: usize,
    cancel_requested: bool,
    modal: bool,
    minimized: bool,
    last_paint_tick: u64,
    last_input_tick: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClipboardPasteTarget {
    ExplorerWindow(usize),
    Desktop,
}

struct ClipboardPasteJob {
    target: ClipboardPasteTarget,
    clip: ExplorerClipboardState,
    items: Vec<ExplorerClipboardItem>,
    dst_dir_cluster: u32,
    dst_path: String,
    dst_device_index: usize,
    cursor: usize,
    ok_count: usize,
    err_count: usize,
    cut_all_done: bool,
    moved_sources: Vec<u32>,
    status_lines: Vec<String>,
    first_status: String,
}

#[derive(Clone)]
struct NotepadSaveLocation {
    device_index: Option<usize>,
    cluster: u32,
    path: String,
    label: String,
    is_unit: bool,
    depth: u8,
    parent: Option<usize>,
    expanded: bool,
    has_children: bool,
}

#[derive(Clone)]
struct NotepadSavePromptState {
    win_id: usize,
    locations: Vec<NotepadSaveLocation>,
    selected_index: usize,
    scroll_top: usize,
}

#[derive(Clone, Copy)]
struct WindowMoveCapture {
    win_id: usize,
    grab_offset_x: i32,
    grab_offset_y: i32,
}

#[derive(Clone, Copy)]
struct WindowResizeCapture {
    win_id: usize,
    start_mouse_x: i32,
    start_mouse_y: i32,
    start_width: u32,
    start_height: u32,
}

#[derive(Clone, Copy)]
enum WindowPointerCapture {
    Move(WindowMoveCapture),
    Resize(WindowResizeCapture),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WebBackendMode {
    Builtin,
    Cef,
    Vaev,
}

struct AppRunnerLayoutSpec {
    app_title: String,
    theme: String,
    header_text: String,
    body_text: String,
    button_label: String,
    background_color: u32,
    header_color: u32,
    body_color: u32,
    button_color: u32,
}

#[derive(Clone)]
struct StartAppShortcut {
    label: String,
    command: String,
}

#[derive(Clone, Copy)]
struct LinuxRuntimeTargets {
    root_cluster: u32,
    lib_cluster: u32,
    lib64_cluster: u32,
    usr_lib_cluster: u32,
    usr_lib64_cluster: u32,
}

#[derive(Clone, Copy)]
struct LinuxRuntimeLookup {
    root_cluster: u32,
    lib_cluster: Option<u32>,
    lib64_cluster: Option<u32>,
    usr_lib_cluster: Option<u32>,
    usr_lib64_cluster: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinuxInstallLaunchMode {
    Phase1Static,
    Phase2Dynamic,
}

impl LinuxInstallLaunchMode {
    fn suffix(self) -> &'static str {
        match self {
            LinuxInstallLaunchMode::Phase1Static => "Linux",
            LinuxInstallLaunchMode::Phase2Dynamic => "Linux Dyn",
        }
    }

    fn descriptor(self) -> &'static str {
        match self {
            LinuxInstallLaunchMode::Phase1Static => "phase1 static",
            LinuxInstallLaunchMode::Phase2Dynamic => "phase2 dynamic",
        }
    }

    fn manifest_name(self) -> &'static str {
        match self {
            LinuxInstallLaunchMode::Phase1Static => "phase1-static",
            LinuxInstallLaunchMode::Phase2Dynamic => "phase2-dynamic",
        }
    }
}

#[derive(Clone)]
struct LinuxInstallShortcutCandidate {
    exec_name: String,
    source_path: String,
    rank: u8,
    mode: LinuxInstallLaunchMode,
    interp_path: Option<String>,
    needed: Vec<String>,
}

#[derive(Clone)]
struct LinuxLaunchMetadata {
    file_name: String,
    target: Option<String>,
    exec_local: Option<String>,
    interp_path: Option<String>,
    needed_declared: Option<usize>,
    needed: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinuxProcStage {
    ResolveTarget,
    ReadMainElf,
    InspectMain,
    InspectDynamic,
    CollectRuntime,
    Summarize,
    Complete,
    Failed,
    Stopped,
}

struct LinuxStepContainer {
    active: bool,
    auto: bool,
    win_id: usize,
    stage: LinuxProcStage,
    started_tick: u64,
    last_step_tick: u64,
    steps_done: u64,
    target_request: String,
    target_dir: u32,
    target_leaf: String,
    target_name: String,
    target_entry: Option<crate::fs::DirEntry>,
    current_entries: Vec<crate::fs::DirEntry>,
    raw: Vec<u8>,
    interp_path: Option<String>,
    needed: Vec<String>,
    runtime_wants: Vec<String>,
    runtime_lookup: Option<LinuxRuntimeLookup>,
    runtime_entries: Vec<crate::fs::DirEntry>,
    runtime_dir_cache: Vec<(u32, Vec<crate::fs::DirEntry>)>,
    install_manifest_map: Vec<(String, String, String)>,
    runtime_manifest_map: Vec<(String, String, String)>,
    launch_interp_hint: Option<String>,
    launch_needed_hint: Vec<String>,
    launch_hint_from_manifest: bool,
    launch_manifest_file: Option<String>,
    wanted_cursor: usize,
    items_scanned: usize,
    issues: usize,
    last_note: String,
    error: String,
}

impl LinuxStepContainer {
    fn new(win_id: usize, target_request: &str, auto: bool) -> Self {
        Self {
            active: true,
            auto,
            win_id,
            stage: LinuxProcStage::ResolveTarget,
            started_tick: crate::timer::ticks(),
            last_step_tick: crate::timer::ticks(),
            steps_done: 0,
            target_request: String::from(target_request),
            target_dir: 0,
            target_leaf: String::new(),
            target_name: String::new(),
            target_entry: None,
            current_entries: Vec::new(),
            raw: Vec::new(),
            interp_path: None,
            needed: Vec::new(),
            runtime_wants: Vec::new(),
            runtime_lookup: None,
            runtime_entries: Vec::new(),
            runtime_dir_cache: Vec::new(),
            install_manifest_map: Vec::new(),
            runtime_manifest_map: Vec::new(),
            launch_interp_hint: None,
            launch_needed_hint: Vec::new(),
            launch_hint_from_manifest: false,
            launch_manifest_file: None,
            wanted_cursor: 0,
            items_scanned: 0,
            issues: 0,
            last_note: String::from("inicializado"),
            error: String::new(),
        }
    }

    fn stage_label(stage: LinuxProcStage) -> &'static str {
        match stage {
            LinuxProcStage::ResolveTarget => "resolve-target",
            LinuxProcStage::ReadMainElf => "read-main-elf",
            LinuxProcStage::InspectMain => "inspect-main",
            LinuxProcStage::InspectDynamic => "inspect-dynamic",
            LinuxProcStage::CollectRuntime => "collect-runtime",
            LinuxProcStage::Summarize => "summarize",
            LinuxProcStage::Complete => "complete",
            LinuxProcStage::Failed => "failed",
            LinuxProcStage::Stopped => "stopped",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinuxRunLoopStage {
    Preflight,
    PrepareLaunch,
    LoadDependencies,
    FinalizeLaunch,
    InitShim,
    RegisterRuntimePaths,
    RegisterRuntimeBlobs,
    ProbeShim,
    Running,
    Exited,
    Failed,
    Stopped,
}

#[derive(Clone, Copy)]
enum LinuxBlobSource {
    Main,
    Interp,
    Entry(crate::fs::DirEntry),
}

#[derive(Clone)]
struct LinuxBlobJob {
    path_alias: String,
    size: u64,
    source: LinuxBlobSource,
}

#[derive(Clone)]
struct LinuxDepLoadJob {
    soname: String,
    local_name: String,
    entry: crate::fs::DirEntry,
}

struct LinuxRunLoopContainer {
    active: bool,
    auto: bool,
    win_id: usize,
    stage: LinuxRunLoopStage,
    started_tick: u64,
    last_step_tick: u64,
    steps_done: u64,
    target_request: String,
    argv_items: Vec<String>,
    execfn: String,
    main_name: String,
    target_leaf: String,
    interp_source: String,
    interp_local: String,
    main_raw: Vec<u8>,
    interp_raw: Vec<u8>,
    runtime_paths: Vec<(String, u64)>,
    runtime_path_cursor: usize,
    runtime_paths_registered: usize,
    runtime_blob_jobs: Vec<LinuxBlobJob>,
    runtime_blob_cursor: usize,
    runtime_blobs_registered: usize,
    dep_load_jobs: Vec<LinuxDepLoadJob>,
    dep_load_payloads: Vec<(String, Vec<u8>)>,
    dep_load_cursor: usize,
    session_id: u64,
    run_slices: u64,
    run_calls: u64,
    last_slice_errno: i64,
    progress_stage: u8,
    progress_overall: u8,
    progress_bucket_reported: u8,
    bridge_enabled: bool,
    request_real_transfer: bool,
    real_transfer_guarded: bool,
    stalled_slices: u64,
    e2e_validated: bool,
    e2e_connected_streak: u64,
    e2e_ready_streak: u64,
    e2e_frame_advances: u64,
    e2e_last_frame_seq: u64,
    e2e_last_frame_advance_slice: u64,
    e2e_seen_connected: bool,
    e2e_seen_ready: bool,
    e2e_ready_since_slice: u64,
    e2e_validation_slice: u64,
    e2e_validation_frame_advances: u64,
    e2e_post_validate_unready_streak: u64,
    e2e_regressions: u64,
    plan: Option<crate::linux_compat::LinuxDynLaunchPlan>,
    last_note: String,
    error: String,
}

impl LinuxRunLoopContainer {
    fn new(
        win_id: usize,
        target_request: &str,
        auto: bool,
        bridge_enabled: bool,
        request_real_transfer: bool,
    ) -> Self {
        Self {
            active: true,
            auto,
            win_id,
            stage: LinuxRunLoopStage::Preflight,
            started_tick: crate::timer::ticks(),
            last_step_tick: crate::timer::ticks(),
            steps_done: 0,
            target_request: String::from(target_request),
            argv_items: Vec::new(),
            execfn: String::new(),
            main_name: String::new(),
            target_leaf: String::new(),
            interp_source: String::new(),
            interp_local: String::new(),
            main_raw: Vec::new(),
            interp_raw: Vec::new(),
            runtime_paths: Vec::new(),
            runtime_path_cursor: 0,
            runtime_paths_registered: 0,
            runtime_blob_jobs: Vec::new(),
            runtime_blob_cursor: 0,
            runtime_blobs_registered: 0,
            dep_load_jobs: Vec::new(),
            dep_load_payloads: Vec::new(),
            dep_load_cursor: 0,
            session_id: 0,
            run_slices: 0,
            run_calls: 0,
            last_slice_errno: 0,
            progress_stage: 0,
            progress_overall: 0,
            progress_bucket_reported: 0,
            bridge_enabled,
            request_real_transfer,
            real_transfer_guarded: false,
            stalled_slices: 0,
            e2e_validated: false,
            e2e_connected_streak: 0,
            e2e_ready_streak: 0,
            e2e_frame_advances: 0,
            e2e_last_frame_seq: 0,
            e2e_last_frame_advance_slice: 0,
            e2e_seen_connected: false,
            e2e_seen_ready: false,
            e2e_ready_since_slice: 0,
            e2e_validation_slice: 0,
            e2e_validation_frame_advances: 0,
            e2e_post_validate_unready_streak: 0,
            e2e_regressions: 0,
            plan: None,
            last_note: String::from("inicializado"),
            error: String::new(),
        }
    }

    fn stage_label(stage: LinuxRunLoopStage) -> &'static str {
        match stage {
            LinuxRunLoopStage::Preflight => "preflight",
            LinuxRunLoopStage::PrepareLaunch => "prepare-launch",
            LinuxRunLoopStage::LoadDependencies => "load-deps",
            LinuxRunLoopStage::FinalizeLaunch => "finalize-plan",
            LinuxRunLoopStage::InitShim => "init-shim",
            LinuxRunLoopStage::RegisterRuntimePaths => "register-paths",
            LinuxRunLoopStage::RegisterRuntimeBlobs => "register-blobs",
            LinuxRunLoopStage::ProbeShim => "probe-shim",
            LinuxRunLoopStage::Running => "running",
            LinuxRunLoopStage::Exited => "exited",
            LinuxRunLoopStage::Failed => "failed",
            LinuxRunLoopStage::Stopped => "stopped",
        }
    }

    fn update_progress(&mut self, stage_percent: u8, overall_percent: u8) {
        self.progress_stage = stage_percent.min(100);
        if overall_percent > self.progress_overall {
            self.progress_overall = overall_percent.min(100);
        }
    }

    fn progress_bucket(percent: u8) -> u8 {
        percent.min(100) / 5
    }
}

pub struct Compositor {
    windows: Vec<Window>,
    closed_windows: Vec<Window>,
    active_window_id: Option<usize>,
    next_id: usize,
    width: usize,
    height: usize,
    pub mouse_pos: Point,
    pub taskbar: Taskbar,
    pub minimized_windows: Vec<(usize, String)>,
    last_mouse_down: bool,
    last_mouse_right_down: bool,
    taskbar_window: Window,
    start_tools_open: bool,
    start_games_open: bool,
    start_apps_open: bool,
    start_app_shortcuts: Vec<StartAppShortcut>,
    last_explorer_click: Option<ExplorerClickState>,
    explorer_context_menu: Option<ExplorerContextMenuState>,
    desktop_context_menu: Option<ExplorerContextMenuState>,
    explorer_clipboard: Option<ExplorerClipboardState>,
    pointer_capture: Option<WindowPointerCapture>,
    current_volume_device_index: Option<usize>,
    desktop_usb_device_index: Option<usize>,
    desktop_usb_device_label: String,
    desktop_usb_menu_open: bool,
    desktop_usb_last_click_tick: u64,
    desktop_usb_last_probe_tick: u64,
    desktop_usb_ejected_device_index: Option<usize>,
    desktop_surface_status: String,
    explorer_selected_items: Vec<ExplorerSelectionItem>,
    desktop_selected_items: Vec<DesktopSelectionItem>,
    desktop_icon_positions: Vec<DesktopIconPosition>,
    desktop_drag: Option<DesktopDragState>,
    desktop_create_folder: Option<DesktopCreateFolderState>,
    rename_prompt: Option<RenamePromptState>,
    copy_progress_prompt: Option<CopyProgressPromptState>,
    clipboard_paste_job: Option<ClipboardPasteJob>,
    clipboard_paste_job_busy: bool,
    notepad_save_prompt: Option<NotepadSavePromptState>,
    manual_unmount_lock: bool,
    linux_real_transfer_enabled: bool,
    linux_runtime_lookup_enabled: bool,
    linux_step_container: Option<LinuxStepContainer>,
    linux_step_busy: bool,
    linux_runloop_container: Option<LinuxRunLoopContainer>,
    linux_runloop_busy: bool,
    linux_bridge_window_id: Option<usize>,
    linux_bridge_last_seq: u64,
    web_backend_mode: WebBackendMode,
    web_proxy_endpoint_base: String,
}

impl Compositor {
    fn start_menu_rect(&self) -> Rect {
        let inner_h = (START_MENU_ITEMS as u32) * START_MENU_ITEM_H;
        let menu_h = inner_h + (START_MENU_PADDING as u32 * 2);
        Rect::new(START_MENU_X, self.taskbar.rect.y - menu_h as i32, START_MENU_W, menu_h)
    }

    fn start_menu_item_rect(&self, index: usize) -> Rect {
        let menu = self.start_menu_rect();
        let x = menu.x + START_MENU_PADDING;
        let y = menu.y + START_MENU_PADDING + (index as i32 * START_MENU_ITEM_H as i32);
        Rect::new(
            x,
            y,
            menu.width - (START_MENU_PADDING as u32 * 2),
            START_MENU_ITEM_H,
        )
    }

    fn tools_menu_rect(&self) -> Rect {
        let tools_anchor = self.start_menu_item_rect(4);
        let inner_h = (TOOLS_MENU_ITEMS as u32) * START_MENU_ITEM_H;
        let menu_h = inner_h + (TOOLS_MENU_PADDING as u32 * 2);

        Rect::new(
            tools_anchor.x + tools_anchor.width as i32 + 4,
            tools_anchor.y - TOOLS_MENU_PADDING,
            TOOLS_MENU_W,
            menu_h,
        )
    }

    fn tools_menu_item_rect(&self, index: usize) -> Rect {
        let menu = self.tools_menu_rect();
        let x = menu.x + TOOLS_MENU_PADDING;
        let y = menu.y + TOOLS_MENU_PADDING + (index as i32 * START_MENU_ITEM_H as i32);
        Rect::new(
            x,
            y,
            menu.width - (TOOLS_MENU_PADDING as u32 * 2),
            START_MENU_ITEM_H,
        )
    }

    fn games_menu_rect(&self) -> Rect {
        let games_anchor = self.start_menu_item_rect(5);
        let inner_h = (GAMES_MENU_ITEMS as u32) * START_MENU_ITEM_H;
        let menu_h = inner_h + (GAMES_MENU_PADDING as u32 * 2);

        Rect::new(
            games_anchor.x + games_anchor.width as i32 + 4,
            games_anchor.y - GAMES_MENU_PADDING,
            GAMES_MENU_W,
            menu_h,
        )
    }

    fn games_menu_item_rect(&self, index: usize) -> Rect {
        let menu = self.games_menu_rect();
        let x = menu.x + GAMES_MENU_PADDING;
        let y = menu.y + GAMES_MENU_PADDING + (index as i32 * START_MENU_ITEM_H as i32);
        Rect::new(
            x,
            y,
            menu.width - (GAMES_MENU_PADDING as u32 * 2),
            START_MENU_ITEM_H,
        )
    }

    fn apps_menu_item_count(&self) -> usize {
        if self.start_app_shortcuts.is_empty() {
            1
        } else {
            self.start_app_shortcuts
                .len()
                .min(APPS_MENU_MAX_ITEMS)
        }
    }

    fn apps_menu_rect(&self) -> Rect {
        let apps_anchor = self.start_menu_item_rect(6);
        let inner_h = (self.apps_menu_item_count() as u32) * START_MENU_ITEM_H;
        let menu_h = inner_h + (APPS_MENU_PADDING as u32 * 2);

        Rect::new(
            apps_anchor.x + apps_anchor.width as i32 + 4,
            apps_anchor.y - APPS_MENU_PADDING,
            APPS_MENU_W,
            menu_h,
        )
    }

    fn apps_menu_item_rect(&self, index: usize) -> Rect {
        let menu = self.apps_menu_rect();
        let x = menu.x + APPS_MENU_PADDING;
        let y = menu.y + APPS_MENU_PADDING + (index as i32 * START_MENU_ITEM_H as i32);
        Rect::new(
            x,
            y,
            menu.width - (APPS_MENU_PADDING as u32 * 2),
            START_MENU_ITEM_H,
        )
    }

    fn ascii_lower(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for b in text.bytes() {
            if b.is_ascii_uppercase() {
                out.push((b + 32) as char);
            } else {
                out.push(b as char);
            }
        }
        out
    }

    // Accepts values like "32", "32.." or "32," to be tolerant on serial/OSK input.
    fn parse_loose_positive_usize(text: &str, max: usize) -> Option<usize> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Some(1);
        }
        let mut end = 0usize;
        for b in trimmed.bytes() {
            if b.is_ascii_digit() {
                end += 1;
            } else {
                break;
            }
        }
        if end == 0 {
            return None;
        }
        let value = trimmed[..end].parse::<usize>().ok()?;
        Some(value.max(1).min(max))
    }

    fn explorer_path_root_component(path: &str) -> Option<String> {
        let trimmed = path.trim().trim_matches('/');
        if trimmed.is_empty() {
            return None;
        }

        let head = trimmed.split('/').next().unwrap_or("").trim();
        if head.is_empty() || head.eq_ignore_ascii_case("Quick Access") {
            return None;
        }

        Some(alloc::format!("{}/", head))
    }

    fn path_head_matches_volume_label(head: &str, label: &str) -> bool {
        if head.eq_ignore_ascii_case(label) {
            return true;
        }
        if head.len() <= label.len() {
            return false;
        }
        if !head[..label.len()].eq_ignore_ascii_case(label) {
            return false;
        }
        let rest = head[label.len()..].trim_start();
        rest.starts_with('(') || rest.starts_with('[') || rest.starts_with('-')
    }

    fn is_double_click_delta(delta: u64) -> bool {
        delta >= DOUBLE_CLICK_MIN_TICKS && delta <= DOUBLE_CLICK_TICKS
    }

    fn trim_wrapping_quotes(text: &str) -> &str {
        if text.len() < 2 {
            return text;
        }

        let bytes = text.as_bytes();
        let first = bytes[0];
        let last = bytes[text.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            &text[1..text.len() - 1]
        } else {
            text
        }
    }

    fn sanitize_short_component(text: &str, max_len: usize, fallback: &str) -> String {
        let mut out = String::new();

        for b in text.bytes() {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                out.push(b.to_ascii_uppercase() as char);
            }
            if out.len() >= max_len {
                break;
            }
        }

        if out.is_empty() {
            for b in fallback.bytes() {
                if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                    out.push(b.to_ascii_uppercase() as char);
                }
                if out.len() >= max_len {
                    break;
                }
            }
        }

        if out.is_empty() {
            out.push('X');
        }

        out
    }

    fn normalize_to_short_filename(name: &str, default_stem: &str, default_ext: &str) -> String {
        let trimmed = name.trim();
        let (stem_raw, ext_raw) = if let Some(dot) = trimmed.rfind('.') {
            (&trimmed[..dot], &trimmed[dot + 1..])
        } else {
            (trimmed, "")
        };

        let stem = Self::sanitize_short_component(stem_raw, 8, default_stem);
        let ext = if ext_raw.is_empty() {
            Self::sanitize_short_component(default_ext, 3, "TXT")
        } else {
            Self::sanitize_short_component(ext_raw, 3, default_ext)
        };

        if ext.is_empty() {
            stem
        } else {
            alloc::format!("{}.{}", stem, ext)
        }
    }

    fn derive_filename_from_url(url: &str) -> String {
        let mut text = url.trim();
        if let Some(idx) = text.find('#') {
            text = &text[..idx];
        }
        if let Some(idx) = text.find('?') {
            text = &text[..idx];
        }

        let last = text.rsplit('/').next().unwrap_or("");
        if last.is_empty() {
            String::from("DOWNLOAD.TXT")
        } else {
            Self::normalize_to_short_filename(last, "DOWNLOAD", "TXT")
        }
    }

    fn push_unique_url(candidates: &mut Vec<String>, candidate: String) {
        if !candidates.iter().any(|existing| existing == &candidate) {
            candidates.push(candidate);
        }
    }

    fn canonicalize_url_scheme_host(url: &str) -> String {
        let trimmed = url.trim();
        let lower = Self::ascii_lower(trimmed);
        let (scheme, scheme_len) = if lower.starts_with("https://") {
            ("https://", 8usize)
        } else if lower.starts_with("http://") {
            ("http://", 7usize)
        } else {
            return String::from(trimmed);
        };

        let tail = &trimmed[scheme_len..];
        let authority_end = tail
            .bytes()
            .position(|b| b == b'/' || b == b'?' || b == b'#')
            .unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        let rest = &tail[authority_end..];

        let authority_lower = Self::ascii_lower(authority);
        if rest.is_empty() {
            alloc::format!("{}{}", scheme, authority_lower)
        } else if rest.starts_with('?') || rest.starts_with('#') {
            alloc::format!("{}{}/{}", scheme, authority_lower, rest)
        } else {
            alloc::format!("{}{}{}", scheme, authority_lower, rest)
        }
    }

    fn ascii_title_path(path: &str) -> String {
        let mut out = String::with_capacity(path.len());
        let mut first_alpha_in_segment = false;

        for b in path.bytes() {
            if b == b'/' {
                out.push('/');
                first_alpha_in_segment = false;
                continue;
            }
            if b.is_ascii_alphabetic() {
                if !first_alpha_in_segment {
                    out.push(b.to_ascii_uppercase() as char);
                    first_alpha_in_segment = true;
                } else {
                    out.push(b.to_ascii_lowercase() as char);
                }
            } else {
                out.push(b as char);
            }
        }
        out
    }

    fn build_fetch_url_candidates(url: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        let canonical = Self::canonicalize_url_scheme_host(url);
        if canonical.is_empty() {
            return candidates;
        }
        Self::push_unique_url(&mut candidates, canonical.clone());

        let canonical_lower = Self::ascii_lower(canonical.as_str());
        let scheme_len = if canonical_lower.starts_with("https://") {
            8usize
        } else if canonical_lower.starts_with("http://") {
            7usize
        } else {
            return candidates;
        };

        let scheme = &canonical[..scheme_len];
        let tail = &canonical[scheme_len..];
        let authority_end = tail
            .bytes()
            .position(|b| b == b'/' || b == b'?' || b == b'#')
            .unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        let rest = &tail[authority_end..];

        let (path_raw, suffix) = if let Some(i) = rest.find('?') {
            (&rest[..i], &rest[i..])
        } else if let Some(i) = rest.find('#') {
            (&rest[..i], &rest[i..])
        } else {
            (rest, "")
        };
        let path = if path_raw.is_empty() { "/" } else { path_raw };

        let mut has_alpha = false;
        let mut has_upper = false;
        let mut has_lower = false;
        for b in path.bytes() {
            if b.is_ascii_alphabetic() {
                has_alpha = true;
                if b.is_ascii_uppercase() {
                    has_upper = true;
                } else {
                    has_lower = true;
                }
            }
        }

        if has_alpha && has_upper && !has_lower {
            let title_path = Self::ascii_title_path(path);
            let lower_path = Self::ascii_lower(path);
            Self::push_unique_url(
                &mut candidates,
                alloc::format!("{}{}{}{}", scheme, authority, title_path, suffix),
            );
            Self::push_unique_url(
                &mut candidates,
                alloc::format!("{}{}{}{}", scheme, authority, lower_path, suffix),
            );
        }

        candidates
    }

    fn filename_stem(name: &str) -> &str {
        let trimmed = name.trim();
        if let Some(dot) = trimmed.rfind('.') {
            &trimmed[..dot]
        } else {
            trimmed
        }
    }

    fn normalize_short_candidate_8(name: &str) -> String {
        let mut out = String::new();
        for b in name.bytes() {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                out.push(b.to_ascii_uppercase() as char);
            }
            if out.len() >= 8 {
                break;
            }
        }
        out
    }

    fn shortcut_dir_name_matches(entry_name: &str, shortcut_name: &str) -> bool {
        if entry_name.eq_ignore_ascii_case(shortcut_name) {
            return true;
        }

        let entry_short = Self::normalize_short_candidate_8(entry_name);
        let wanted_short = Self::normalize_short_candidate_8(shortcut_name);
        if !entry_short.is_empty() && entry_short == wanted_short {
            return true;
        }

        // FAT32 8.3 often drops trailing plural 'S' for long names.
        if wanted_short.ends_with('S') {
            let mut singular = wanted_short.clone();
            singular.pop();
            if !singular.is_empty() && entry_short == singular {
                return true;
            }
        }
        if entry_short.ends_with('S') {
            let mut singular = entry_short.clone();
            singular.pop();
            if !singular.is_empty() && singular == wanted_short {
                return true;
            }
        }

        false
    }

    fn explorer_shortcut_default_dir_name(shortcut_name: &str) -> Option<&'static str> {
        if shortcut_name.eq_ignore_ascii_case("Desktop") {
            Some("DESKTOP")
        } else if shortcut_name.eq_ignore_ascii_case("Downloads") {
            Some("DOWNLOAD")
        } else if shortcut_name.eq_ignore_ascii_case("Documents") {
            Some("DOCUMENT")
        } else if shortcut_name.eq_ignore_ascii_case("Images") {
            Some("IMAGES")
        } else if shortcut_name.eq_ignore_ascii_case("Videos") {
            Some("VIDEOS")
        } else {
            None
        }
    }

    fn explorer_shortcut_aliases(shortcut_name: &str) -> &'static [&'static str] {
        const DESKTOP: &[&str] = &["Desktop", "DESKTOP", "DESKTO~1"];
        const DOWNLOADS: &[&str] = &["Downloads", "DOWNLOADS", "DOWNLOAD", "DOWNLO~1"];
        const DOCUMENTS: &[&str] = &["Documents", "DOCUMENTS", "DOCUMENT", "DOCUME~1"];
        const IMAGES: &[&str] = &["Images", "IMAGES", "IMAGE", "IMAGE~1"];
        const VIDEOS: &[&str] = &["Videos", "VIDEOS", "VIDEO", "VIDEO~1"];
        const EMPTY: &[&str] = &[];

        if shortcut_name.eq_ignore_ascii_case("Desktop") {
            DESKTOP
        } else if shortcut_name.eq_ignore_ascii_case("Downloads") {
            DOWNLOADS
        } else if shortcut_name.eq_ignore_ascii_case("Documents") {
            DOCUMENTS
        } else if shortcut_name.eq_ignore_ascii_case("Images") {
            IMAGES
        } else if shortcut_name.eq_ignore_ascii_case("Videos") {
            VIDEOS
        } else {
            EMPTY
        }
    }

    fn is_quick_access_shortcut_name(name: &str) -> bool {
        const QUICK_ACCESS: &[&str] = &["Desktop", "Downloads", "Documents", "Images", "Videos"];

        for shortcut in QUICK_ACCESS.iter() {
            for alias in Self::explorer_shortcut_aliases(shortcut).iter() {
                if name.eq_ignore_ascii_case(alias)
                    || Self::shortcut_dir_name_matches(name, alias)
                {
                    return true;
                }
            }
        }

        false
    }

    fn resolve_named_root_dir_cluster(
        fat: &mut crate::fat32::Fat32,
        shortcut_name: &str,
        create_if_missing: bool,
    ) -> Result<(u32, bool), String> {
        use crate::fs::FileType;

        let root = fat.root_cluster;
        let aliases = Self::explorer_shortcut_aliases(shortcut_name);
        if aliases.is_empty() {
            return Err(alloc::format!("atajo no soportado: {}", shortcut_name));
        }

        if let Ok(entries) = fat.read_dir_entries(root) {
            for entry in entries.iter() {
                if !entry.valid || entry.file_type != FileType::Directory {
                    continue;
                }

                let entry_name = entry.full_name();
                for alias in aliases.iter() {
                    if entry.matches_name(alias)
                        || entry_name.eq_ignore_ascii_case(alias)
                        || Self::shortcut_dir_name_matches(entry_name.as_str(), alias)
                    {
                        return Ok((if entry.cluster == 0 { root } else { entry.cluster }, false));
                    }
                }
            }
        }

        if create_if_missing {
            if let Some(short_name) = Self::explorer_shortcut_default_dir_name(shortcut_name) {
                let cluster = fat
                    .ensure_subdirectory(root, short_name)
                    .map_err(String::from)?;
                return Ok((cluster, true));
            }
        }

        Err(alloc::format!("Folder '{}' not found on available volumes.", shortcut_name))
    }

    fn linux_path_leaf(path: &str) -> &str {
        let mut start = 0usize;
        for (idx, b) in path.bytes().enumerate() {
            if b == b'/' || b == b'\\' {
                start = idx + 1;
            }
        }
        &path[start..]
    }

    fn linux_shim_last_path(status: &crate::syscall::LinuxShimStatus) -> Option<String> {
        let path_len = status.last_path_len.min(status.last_path.len());
        if path_len == 0 {
            return None;
        }
        let text = Self::linux_decode_status_ascii(status.last_path.as_slice(), path_len);
        if text.is_empty() || text == "sin estado" {
            None
        } else {
            Some(text)
        }
    }

    fn linux_shim_path_diag_line(status: &crate::syscall::LinuxShimStatus) -> Option<String> {
        let path_text = Self::linux_shim_last_path(status)?;
        let soname_leaf = Self::linux_path_leaf(path_text.as_str()).trim();
        let soname_hint = if soname_leaf.is_empty() { "n/a" } else { soname_leaf };
        let lookup_sys_name = crate::syscall::linux_syscall_name(status.last_path_sysno);
        let lookup_errno_name = crate::syscall::linux_errno_name(status.last_path_errno);
        Some(alloc::format!(
            "Linux runloop diag: lookup {}({}) errno={}({}) hit={} soname_hint='{}' path='{}'",
            lookup_sys_name,
            status.last_path_sysno,
            status.last_path_errno,
            lookup_errno_name,
            if status.last_path_runtime_hit { "yes" } else { "no" },
            soname_hint,
            path_text
        ))
    }

    fn normalize_linux_path(path: &str) -> String {
        let mut lowered = String::with_capacity(path.len());
        for b in path.bytes() {
            if b == b'\\' {
                lowered.push('/');
            } else if b.is_ascii_uppercase() {
                lowered.push((b + 32) as char);
            } else {
                lowered.push(b as char);
            }
        }

        let bytes = lowered.as_bytes();
        let mut start = 0usize;
        while start + 1 < bytes.len() && bytes[start] == b'.' && bytes[start + 1] == b'/' {
            start += 2;
        }
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }

        if start == 0 {
            lowered
        } else {
            String::from(&lowered[start..])
        }
    }

    fn parse_install_manifest_mapping(text: &str) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some(split_at) = line.find("<-") else {
                continue;
            };

            let left = line[..split_at].trim();
            let right = line[split_at + 2..].trim();
            if right.is_empty() {
                continue;
            }

            let short = left.split_whitespace().last().unwrap_or("").trim();
            if short.is_empty() || short.contains('=') {
                continue;
            }

            let source_norm = Self::normalize_linux_path(right);
            let source_leaf = Self::ascii_lower(Self::linux_path_leaf(right));
            out.push((String::from(short), source_norm, source_leaf));
        }
        out
    }

    fn parse_linux_launch_manifest_text(file_name: &str, text: &str) -> Option<LinuxLaunchMetadata> {
        let mut saw_header = false;
        let mut target: Option<String> = None;
        let mut exec_local: Option<String> = None;
        let mut interp_path: Option<String> = None;
        let mut needed_declared: Option<usize> = None;
        let mut needed: Vec<String> = Vec::new();

        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !saw_header {
                if line.eq_ignore_ascii_case("LINUX LAUNCH") {
                    saw_header = true;
                }
                continue;
            }

            if let Some(rest) = line.strip_prefix("TARGET=") {
                let value = rest.trim();
                if !value.is_empty() {
                    target = Some(String::from(value));
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("EXEC_LOCAL=") {
                let value = rest.trim();
                if !value.is_empty() {
                    exec_local = Some(String::from(value));
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("PT_INTERP=") {
                let value = rest.trim();
                if !value.is_empty() {
                    interp_path = Some(String::from(value));
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("DT_NEEDED=") {
                let value = rest.trim();
                if let Ok(count) = value.parse::<usize>() {
                    needed_declared = Some(count);
                }
                continue;
            }
            if line.starts_with("NEEDED_") {
                if let Some(eq) = line.find('=') {
                    let value = line[eq + 1..].trim();
                    if !value.is_empty()
                        && !needed
                            .iter()
                            .any(|existing| existing.eq_ignore_ascii_case(value))
                    {
                        needed.push(String::from(value));
                    }
                }
                continue;
            }
        }

        if !saw_header {
            return None;
        }
        if let Some(expected) = needed_declared {
            if needed.len() > expected {
                needed.truncate(expected);
            }
        }

        Some(LinuxLaunchMetadata {
            file_name: String::from(file_name),
            target,
            exec_local,
            interp_path,
            needed_declared,
            needed,
        })
    }

    fn linux_launch_manifest_matches_exec(metadata: &LinuxLaunchMetadata, exec_file_name: &str) -> bool {
        if let Some(local) = metadata.exec_local.as_deref() {
            if local.eq_ignore_ascii_case(exec_file_name) {
                return true;
            }
        }
        if let Some(target) = metadata.target.as_deref() {
            if target.eq_ignore_ascii_case(exec_file_name) {
                return true;
            }
            let target_leaf = Self::linux_path_leaf(target);
            if !target_leaf.is_empty() && target_leaf.eq_ignore_ascii_case(exec_file_name) {
                return true;
            }
        }
        false
    }

    fn load_linux_launch_manifest_for_exec(
        fat: &mut crate::fat32::Fat32,
        entries: &[crate::fs::DirEntry],
        exec_file_name: &str,
    ) -> Option<LinuxLaunchMetadata> {
        use crate::fs::FileType;

        let mut fallback: Option<LinuxLaunchMetadata> = None;

        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File || entry.cluster < 2 || entry.size == 0 {
                continue;
            }

            let name = entry.full_name();
            if !Self::ascii_lower(name.as_str()).ends_with(".lnx") {
                continue;
            }
            if entry.size as usize > 16 * 1024 {
                continue;
            }

            let mut raw = Vec::new();
            raw.resize(entry.size as usize, 0);
            let len = match fat.read_file_sized(entry.cluster, entry.size as usize, &mut raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            raw.truncate(len);

            let text = match core::str::from_utf8(raw.as_slice()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(metadata) = Self::parse_linux_launch_manifest_text(name.as_str(), text) else {
                continue;
            };

            if Self::linux_launch_manifest_matches_exec(&metadata, exec_file_name) {
                return Some(metadata);
            }

            if fallback.is_none() {
                fallback = Some(metadata);
            }
        }

        fallback
    }

    fn find_dir_file_entry_by_name(
        entries: &[crate::fs::DirEntry],
        target_name: &str,
    ) -> Option<crate::fs::DirEntry> {
        use crate::fs::FileType;

        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File {
                continue;
            }
            if entry.matches_name(target_name) || entry.full_name().eq_ignore_ascii_case(target_name) {
                return Some(*entry);
            }
        }
        None
    }

    fn load_manifest_for_installed_exec(
        fat: &mut crate::fat32::Fat32,
        entries: &[crate::fs::DirEntry],
        exec_file_name: &str,
    ) -> Option<Vec<(String, String, String)>> {
        use crate::fs::FileType;

        let mut fallback: Option<Vec<(String, String, String)>> = None;

        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File || entry.cluster < 2 || entry.size == 0 {
                continue;
            }

            let name = entry.full_name();
            if !Self::ascii_lower(name.as_str()).ends_with(".lst") {
                continue;
            }
            if entry.size as usize > 256 * 1024 {
                continue;
            }

            let mut raw = Vec::new();
            raw.resize(entry.size as usize, 0);
            let len = match fat.read_file_sized(entry.cluster, entry.size as usize, &mut raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            raw.truncate(len);

            let text = match core::str::from_utf8(raw.as_slice()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let map = Self::parse_install_manifest_mapping(text);
            if map.is_empty() {
                continue;
            }

            let mut contains_exec = false;
            for (short, _, _) in map.iter() {
                if short.eq_ignore_ascii_case(exec_file_name) {
                    contains_exec = true;
                    break;
                }
            }
            if contains_exec {
                return Some(map);
            }

            if fallback.is_none() {
                fallback = Some(map);
            }
        }

        fallback
    }

    fn find_child_directory_cluster(
        fat: &mut crate::fat32::Fat32,
        parent_cluster: u32,
        child_name: &str,
    ) -> Option<u32> {
        use crate::fs::FileType;

        let entries = fat
            .read_dir_entries_limited(parent_cluster, LINUX_RUNTIME_LOOKUP_MAX_CLUSTERS)
            .ok()?;
        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::Directory {
                continue;
            }
            if entry.matches_name(child_name) || entry.full_name().eq_ignore_ascii_case(child_name) {
                return Some(if entry.cluster == 0 {
                    fat.root_cluster
                } else {
                    entry.cluster
                });
            }
        }
        None
    }

    fn push_unique_cluster(clusters: &mut Vec<u32>, cluster: u32) {
        if cluster < 2 {
            return;
        }
        if clusters.iter().any(|c| *c == cluster) {
            return;
        }
        clusters.push(cluster);
    }

    fn append_unique_file_entries_from_dir(
        fat: &mut crate::fat32::Fat32,
        dir_cluster: u32,
        out_files: &mut Vec<crate::fs::DirEntry>,
    ) -> usize {
        use crate::fs::FileType;

        let Ok(entries) = fat.read_dir_entries(dir_cluster) else {
            return 0;
        };

        let mut appended = 0usize;
        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File || entry.cluster < 2 || entry.size == 0 {
                continue;
            }
            out_files.push(*entry);
            appended += 1;
        }
        appended
    }

    fn load_manifest_mappings_from_entries(
        fat: &mut crate::fat32::Fat32,
        entries: &[crate::fs::DirEntry],
    ) -> Vec<(String, String, String)> {
        use crate::fs::FileType;

        let mut out: Vec<(String, String, String)> = Vec::new();
        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File || entry.cluster < 2 || entry.size == 0 {
                continue;
            }

            let name = entry.full_name();
            if !Self::ascii_lower(name.as_str()).ends_with(".lst") {
                continue;
            }
            if entry.size as usize > 256 * 1024 {
                continue;
            }

            let mut raw = Vec::new();
            raw.resize(entry.size as usize, 0);
            let len = match fat.read_file_sized(entry.cluster, entry.size as usize, &mut raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            raw.truncate(len);

            let text = match core::str::from_utf8(raw.as_slice()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let map = Self::parse_install_manifest_mapping(text);
            for candidate in map.into_iter() {
                let duplicate = out.iter().any(|(short, source_norm, _)| {
                    short.eq_ignore_ascii_case(candidate.0.as_str()) && source_norm == &candidate.1
                });
                if !duplicate {
                    out.push(candidate);
                }
            }
        }
        out
    }

    fn collect_global_linux_runtime_support(
        &mut self,
        win_id: usize,
        fat: &mut crate::fat32::Fat32,
    ) -> (Vec<crate::fs::DirEntry>, Vec<(String, String, String)>) {
        use crate::fs::FileType;

        let mut runtime_files = Vec::new();
        let mut runtime_maps = Vec::new();
        let mut scanned_items = 0usize;

        let Ok(root_entries) = fat.read_dir_entries(fat.root_cluster) else {
            return (runtime_files, runtime_maps);
        };

        let mut runtime_root_cluster: Option<u32> = None;
        for entry in root_entries.iter() {
            scanned_items = scanned_items.saturating_add(1);
            self.pump_ui_while_linux_preflight(win_id, scanned_items);
            if !entry.valid || entry.file_type != FileType::Directory {
                continue;
            }
            if entry.matches_name("LINUXRT") || entry.full_name().eq_ignore_ascii_case("LINUXRT") {
                runtime_root_cluster = Some(if entry.cluster == 0 {
                    fat.root_cluster
                } else {
                    entry.cluster
                });
                break;
            }
        }

        let Some(rt_cluster) = runtime_root_cluster else {
            return (runtime_files, runtime_maps);
        };

        let mut runtime_dirs: Vec<u32> = Vec::new();
        Self::push_unique_cluster(&mut runtime_dirs, rt_cluster);

        if let Some(lib) = Self::find_child_directory_cluster(fat, rt_cluster, "LIB") {
            Self::push_unique_cluster(&mut runtime_dirs, lib);
        }
        if let Some(lib64) = Self::find_child_directory_cluster(fat, rt_cluster, "LIB64") {
            Self::push_unique_cluster(&mut runtime_dirs, lib64);
        }

        if let Some(usr) = Self::find_child_directory_cluster(fat, rt_cluster, "USR") {
            Self::push_unique_cluster(&mut runtime_dirs, usr);
            if let Some(usr_lib) = Self::find_child_directory_cluster(fat, usr, "LIB") {
                Self::push_unique_cluster(&mut runtime_dirs, usr_lib);
            }
            if let Some(usr_lib64) = Self::find_child_directory_cluster(fat, usr, "LIB64") {
                Self::push_unique_cluster(&mut runtime_dirs, usr_lib64);
            }
        }

        for dir_cluster in runtime_dirs.iter() {
            let appended = Self::append_unique_file_entries_from_dir(fat, *dir_cluster, &mut runtime_files);
            if appended > 0 {
                scanned_items = scanned_items.saturating_add(appended);
                self.pump_ui_while_linux_preflight(win_id, scanned_items);
            }
        }

        if !runtime_files.is_empty() {
            runtime_maps = Self::load_manifest_mappings_from_entries(fat, runtime_files.as_slice());
            scanned_items = scanned_items.saturating_add(runtime_maps.len());
            self.pump_ui_while_linux_preflight(win_id, scanned_items);
        }
        (runtime_files, runtime_maps)
    }

    fn load_runtime_manifest_mappings_lite(
        fat: &mut crate::fat32::Fat32,
        entries: &[crate::fs::DirEntry],
        max_manifests: usize,
    ) -> Vec<(String, String, String)> {
        use crate::fs::FileType;

        let mut out: Vec<(String, String, String)> = Vec::new();
        let mut loaded = 0usize;
        for entry in entries.iter() {
            if loaded >= max_manifests {
                break;
            }
            if !entry.valid || entry.file_type != FileType::File || entry.cluster < 2 || entry.size == 0 {
                continue;
            }

            let name = entry.full_name();
            let lower = Self::ascii_lower(name.as_str());
            if !lower.ends_with(".lst") {
                continue;
            }
            if !(lower.starts_with("rt") || lower == "rtbase.lst") {
                continue;
            }
            if entry.size as usize > 256 * 1024 {
                continue;
            }

            let mut raw = Vec::new();
            raw.resize(entry.size as usize, 0);
            let len = match fat.read_file_sized(entry.cluster, entry.size as usize, &mut raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            raw.truncate(len);

            let text = match core::str::from_utf8(raw.as_slice()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let mut map = Self::parse_install_manifest_mapping(text);
            if map.is_empty() {
                continue;
            }
            if map.len() > LINUX_RUNTIME_LOOKUP_MAX_MAP_ENTRIES {
                map.truncate(LINUX_RUNTIME_LOOKUP_MAX_MAP_ENTRIES);
            }
            loaded += 1;

            for candidate in map.into_iter() {
                let duplicate = out.iter().any(|(short, source_norm, _)| {
                    short.eq_ignore_ascii_case(candidate.0.as_str()) && source_norm == &candidate.1
                });
                if !duplicate {
                    out.push(candidate);
                }
            }
        }

        out
    }

    fn discover_linux_runtime_lookup(
        &mut self,
        win_id: usize,
        fat: &mut crate::fat32::Fat32,
    ) -> Option<LinuxRuntimeLookup> {
        use crate::fs::FileType;

        let root_entries = fat
            .read_dir_entries_limited(fat.root_cluster, LINUX_RUNTIME_LOOKUP_MAX_CLUSTERS)
            .ok()?;
        let mut scanned_items = 0usize;
        let mut runtime_root_cluster: Option<u32> = None;
        for entry in root_entries.iter() {
            scanned_items = scanned_items.saturating_add(1);
            self.pump_ui_while_linux_preflight(win_id, scanned_items);
            if !entry.valid || entry.file_type != FileType::Directory {
                continue;
            }
            if entry.matches_name("LINUXRT") || entry.full_name().eq_ignore_ascii_case("LINUXRT") {
                runtime_root_cluster = Some(if entry.cluster == 0 {
                    fat.root_cluster
                } else {
                    entry.cluster
                });
                break;
            }
        }

        let rt_cluster = runtime_root_cluster?;
        let lib_cluster = Self::find_child_directory_cluster(fat, rt_cluster, "LIB");
        let lib64_cluster = Self::find_child_directory_cluster(fat, rt_cluster, "LIB64");
        let usr_cluster = Self::find_child_directory_cluster(fat, rt_cluster, "USR");
        let usr_lib_cluster = usr_cluster.and_then(|usr| Self::find_child_directory_cluster(fat, usr, "LIB"));
        let usr_lib64_cluster =
            usr_cluster.and_then(|usr| Self::find_child_directory_cluster(fat, usr, "LIB64"));

        Some(LinuxRuntimeLookup {
            root_cluster: rt_cluster,
            lib_cluster,
            lib64_cluster,
            usr_lib_cluster,
            usr_lib64_cluster,
        })
    }

    fn resolve_linux_dependency_manifest_short(
        map: &[(String, String, String)],
        wanted_name: &str,
    ) -> Option<(String, String)> {
        let wanted_norm = Self::normalize_linux_path(wanted_name);
        let wanted_leaf = Self::ascii_lower(Self::linux_path_leaf(wanted_norm.as_str()));
        let mut fallback_leaf: Option<(String, String)> = None;

        for (short, source_norm, source_leaf) in map.iter() {
            let exact_match = source_norm == &wanted_norm;
            let leaf_match = source_leaf == &wanted_leaf;
            if !exact_match && !leaf_match {
                continue;
            }

            if exact_match {
                return Some((short.clone(), source_norm.clone()));
            }
            if fallback_leaf.is_none() {
                fallback_leaf = Some((short.clone(), source_norm.clone()));
            }
        }

        fallback_leaf
    }

    fn runtime_lookup_cluster_order(
        lookup: LinuxRuntimeLookup,
        source_hint: Option<&str>,
    ) -> Vec<u32> {
        let mut clusters: Vec<u32> = Vec::new();

        if let Some(hint) = source_hint {
            let hint_norm = Self::normalize_linux_path(hint);
            if hint_norm.starts_with("usr/lib64/") || hint_norm.contains("/usr/lib64/") {
                if let Some(cluster) = lookup.usr_lib64_cluster {
                    Self::push_unique_cluster(&mut clusters, cluster);
                }
                if let Some(cluster) = lookup.lib64_cluster {
                    Self::push_unique_cluster(&mut clusters, cluster);
                }
            } else if hint_norm.starts_with("usr/lib/") || hint_norm.contains("/usr/lib/") {
                if let Some(cluster) = lookup.usr_lib_cluster {
                    Self::push_unique_cluster(&mut clusters, cluster);
                }
                if let Some(cluster) = lookup.lib_cluster {
                    Self::push_unique_cluster(&mut clusters, cluster);
                }
            } else if hint_norm.starts_with("lib64/") || hint_norm.contains("/lib64/") {
                if let Some(cluster) = lookup.lib64_cluster {
                    Self::push_unique_cluster(&mut clusters, cluster);
                }
                if let Some(cluster) = lookup.usr_lib64_cluster {
                    Self::push_unique_cluster(&mut clusters, cluster);
                }
            } else {
                if let Some(cluster) = lookup.lib_cluster {
                    Self::push_unique_cluster(&mut clusters, cluster);
                }
                if let Some(cluster) = lookup.usr_lib_cluster {
                    Self::push_unique_cluster(&mut clusters, cluster);
                }
            }
        }

        if let Some(cluster) = lookup.lib64_cluster {
            Self::push_unique_cluster(&mut clusters, cluster);
        }
        if let Some(cluster) = lookup.lib_cluster {
            Self::push_unique_cluster(&mut clusters, cluster);
        }
        if let Some(cluster) = lookup.usr_lib64_cluster {
            Self::push_unique_cluster(&mut clusters, cluster);
        }
        if let Some(cluster) = lookup.usr_lib_cluster {
            Self::push_unique_cluster(&mut clusters, cluster);
        }
        Self::push_unique_cluster(&mut clusters, lookup.root_cluster);

        clusters
    }

    fn collect_targeted_linux_runtime_support(
        &mut self,
        win_id: usize,
        fat: &mut crate::fat32::Fat32,
        wanted_names: &[String],
    ) -> (Vec<crate::fs::DirEntry>, Vec<(String, String, String)>, bool) {
        let mut runtime_files: Vec<crate::fs::DirEntry> = Vec::new();
        let mut runtime_maps: Vec<(String, String, String)> = Vec::new();
        let mut timed_out = false;
        let start_tick = crate::timer::ticks();
        let is_timed_out = |start: u64| -> bool {
            crate::timer::ticks().saturating_sub(start) > LINUX_RUNTIME_LOOKUP_TICK_BUDGET
        };

        let Some(lookup) = self.discover_linux_runtime_lookup(win_id, fat) else {
            return (runtime_files, runtime_maps, timed_out);
        };

        let root_entries = match fat.read_dir_entries_limited(
            lookup.root_cluster,
            LINUX_RUNTIME_LOOKUP_MAX_CLUSTERS,
        ) {
            Ok(entries) => entries,
            Err(_) => return (runtime_files, runtime_maps, timed_out),
        };

        let mut scanned_items = root_entries.len().max(1);
        self.pump_ui_while_linux_preflight(win_id, scanned_items);
        if is_timed_out(start_tick) {
            timed_out = true;
            return (runtime_files, runtime_maps, timed_out);
        }
        if !root_entries.is_empty() {
            runtime_maps = Self::load_runtime_manifest_mappings_lite(
                fat,
                root_entries.as_slice(),
                LINUX_RUNTIME_LOOKUP_MAX_MANIFESTS,
            );
            scanned_items = scanned_items.saturating_add(runtime_maps.len());
            self.pump_ui_while_linux_preflight(win_id, scanned_items);
            if is_timed_out(start_tick) {
                timed_out = true;
                return (runtime_files, runtime_maps, timed_out);
            }
        }

        let mut dir_cache: Vec<(u32, Vec<crate::fs::DirEntry>)> = Vec::new();
        dir_cache.push((lookup.root_cluster, root_entries));

        for (wanted_idx, wanted_name) in wanted_names.iter().enumerate() {
            if is_timed_out(start_tick) {
                timed_out = true;
                break;
            }
            if wanted_name.trim().is_empty() {
                continue;
            }
            if (wanted_idx & 7) == 0 {
                self.pump_ui_while_linux_preflight(win_id, wanted_idx + 1);
            }

            let mut source_hint: Option<String> = None;
            let mut candidate_names: Vec<String> = Vec::new();
            if let Some((short, source_norm)) =
                Self::resolve_linux_dependency_manifest_short(runtime_maps.as_slice(), wanted_name)
            {
                candidate_names.push(short);
                source_hint = Some(source_norm);
            }

            let wanted_norm = Self::normalize_linux_path(wanted_name);
            let wanted_leaf = Self::ascii_lower(Self::linux_path_leaf(wanted_norm.as_str()));
            if !wanted_leaf.is_empty()
                && !candidate_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(wanted_leaf.as_str()))
            {
                candidate_names.push(wanted_leaf);
            }
            if !candidate_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(wanted_name.as_str()))
            {
                candidate_names.push(wanted_name.clone());
            }

            let hint = source_hint.as_deref().unwrap_or(wanted_norm.as_str());
            let cluster_order = Self::runtime_lookup_cluster_order(lookup, Some(hint));
            let mut found: Option<crate::fs::DirEntry> = None;

            'cluster_search: for cluster in cluster_order.iter() {
                if is_timed_out(start_tick) {
                    timed_out = true;
                    break 'cluster_search;
                }
                let cache_idx = match dir_cache
                    .iter()
                    .position(|(cached_cluster, _)| cached_cluster == cluster)
                {
                    Some(idx) => idx,
                    None => {
                        let entries = match fat.read_dir_entries_limited(
                            *cluster,
                            LINUX_RUNTIME_LOOKUP_MAX_CLUSTERS,
                        ) {
                            Ok(entries) => entries,
                            Err(_) => continue,
                        };
                        scanned_items = scanned_items.saturating_add(entries.len().max(1));
                        self.pump_ui_while_linux_preflight(win_id, scanned_items);
                        if is_timed_out(start_tick) {
                            timed_out = true;
                            break 'cluster_search;
                        }
                        dir_cache.push((*cluster, entries));
                        dir_cache.len() - 1
                    }
                };

                let entries = dir_cache[cache_idx].1.as_slice();
                for candidate in candidate_names.iter() {
                    if let Some(entry) = Self::find_dir_file_entry_by_name(entries, candidate) {
                        found = Some(entry);
                        break 'cluster_search;
                    }
                }
            }

            if let Some(entry) = found {
                let entry_name = entry.full_name();
                let duplicate = runtime_files.iter().any(|existing| {
                    existing.cluster == entry.cluster
                        && existing.size == entry.size
                        && existing.full_name().eq_ignore_ascii_case(entry_name.as_str())
                });
                if !duplicate {
                    runtime_files.push(entry);
                }
            }
        }

        (runtime_files, runtime_maps, timed_out)
    }

    fn ensure_linux_runtime_targets(
        fat: &mut crate::fat32::Fat32,
    ) -> Result<LinuxRuntimeTargets, &'static str> {
        let rt_root = fat.ensure_subdirectory(fat.root_cluster, "LINUXRT")?;
        let lib = fat.ensure_subdirectory(rt_root, "LIB")?;
        let lib64 = fat.ensure_subdirectory(rt_root, "LIB64")?;
        let usr = fat.ensure_subdirectory(rt_root, "USR")?;
        let usr_lib = fat.ensure_subdirectory(usr, "LIB")?;
        let usr_lib64 = fat.ensure_subdirectory(usr, "LIB64")?;
        Ok(LinuxRuntimeTargets {
            root_cluster: rt_root,
            lib_cluster: lib,
            lib64_cluster: lib64,
            usr_lib_cluster: usr_lib,
            usr_lib64_cluster: usr_lib64,
        })
    }

    fn runtime_target_cluster_for_source(targets: &LinuxRuntimeTargets, source_path: &str) -> u32 {
        let source_norm = Self::normalize_linux_path(source_path);
        if source_norm.starts_with("usr/lib64/") || source_norm.contains("/usr/lib64/") {
            targets.usr_lib64_cluster
        } else if source_norm.starts_with("usr/lib/") || source_norm.contains("/usr/lib/") {
            targets.usr_lib_cluster
        } else if source_norm.starts_with("lib64/") || source_norm.contains("/lib64/") {
            targets.lib64_cluster
        } else {
            targets.lib_cluster
        }
    }

    fn is_linux_runtime_candidate(source_path: &str, payload: &[u8]) -> bool {
        if !Self::is_elf_payload(payload) {
            return false;
        }
        let source_norm = Self::normalize_linux_path(source_path);
        let leaf = Self::ascii_lower(Self::linux_path_leaf(source_norm.as_str()));
        if leaf.is_empty() {
            return false;
        }
        leaf.ends_with(".so")
            || leaf.contains(".so.")
            || leaf.starts_with("ld-linux")
            || leaf.starts_with("ld-musl")
    }

    fn maybe_stage_linux_runtime_file(
        fat: &mut crate::fat32::Fat32,
        source_path: &str,
        local_name: &str,
        payload: &[u8],
        runtime_targets: &mut Option<LinuxRuntimeTargets>,
        runtime_manifest: &mut String,
        runtime_files_written: &mut usize,
    ) -> Result<(), &'static str> {
        if !Self::is_linux_runtime_candidate(source_path, payload) {
            return Ok(());
        }

        if runtime_targets.is_none() {
            *runtime_targets = Some(Self::ensure_linux_runtime_targets(fat)?);
        }
        let Some(targets) = runtime_targets.as_ref() else {
            return Err("no se pudo inicializar /LINUXRT.");
        };

        let dst_cluster = Self::runtime_target_cluster_for_source(targets, source_path);
        fat.write_text_file_in_dir(dst_cluster, local_name, payload)?;

        *runtime_files_written += 1;
        if runtime_manifest.is_empty() {
            runtime_manifest.push_str("LINUXRT INSTALL\n");
        }
        runtime_manifest.push_str(
            alloc::format!(
                "{:04} {} <- {}\n",
                *runtime_files_written,
                local_name,
                source_path
            )
            .as_str(),
        );
        Ok(())
    }

    fn resolve_linux_dependency_name(
        entries: &[crate::fs::DirEntry],
        manifest_map: Option<&[(String, String, String)]>,
        wanted_name: &str,
    ) -> Option<String> {
        let wanted_trim = wanted_name.trim();
        // Do not feed path-like strings directly into 8.3 matcher. It can yield false positives.
        if !wanted_trim.is_empty()
            && !wanted_trim.contains('/')
            && !wanted_trim.contains('\\')
        {
            if let Some(entry) = Self::find_dir_file_entry_by_name(entries, wanted_trim) {
                return Some(entry.full_name());
            }
        }

        let wanted_norm = Self::normalize_linux_path(wanted_trim);
        let wanted_leaf = Self::ascii_lower(Self::linux_path_leaf(wanted_norm.as_str()));
        if !wanted_leaf.is_empty() {
            if let Some(entry) = Self::find_dir_file_entry_by_name(entries, wanted_leaf.as_str()) {
                return Some(entry.full_name());
            }
        }

        let Some(map) = manifest_map else {
            return None;
        };

        let mut fallback_leaf: Option<String> = None;
        for (short, source_norm, source_leaf) in map.iter() {
            let exact_match = source_norm == &wanted_norm;
            let leaf_match = source_leaf == &wanted_leaf;
            if !exact_match && !leaf_match {
                continue;
            }

            if let Some(entry) = Self::find_dir_file_entry_by_name(entries, short.as_str()) {
                let local_name = entry.full_name();
                if exact_match {
                    return Some(local_name);
                }
                if fallback_leaf.is_none() {
                    fallback_leaf = Some(local_name);
                }
            }
        }

        fallback_leaf
    }

    fn terminal_current_cluster(&self, win_id: usize, fat: &crate::fat32::Fat32) -> u32 {
        match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => {
                if win.current_dir_cluster == 0 {
                    fat.root_cluster
                } else {
                    win.current_dir_cluster
                }
            }
            None => fat.root_cluster,
        }
    }

    fn resolve_terminal_parent_and_leaf(
        fat: &mut crate::fat32::Fat32,
        base_cluster: u32,
        raw_path: &str,
    ) -> Result<(u32, String), &'static str> {
        use crate::fs::FileType;

        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return Err("ruta vacia");
        }

        let mut cluster = if trimmed.starts_with('/') {
            fat.root_cluster
        } else {
            base_cluster
        };

        let mut components: Vec<&str> = Vec::new();
        for part in trimmed.split('/') {
            let p = part.trim();
            if p.is_empty() || p == "." {
                continue;
            }
            components.push(p);
        }
        if components.is_empty() {
            return Err("ruta sin nombre de archivo");
        }

        let leaf = components.pop().unwrap_or("");
        if leaf.is_empty() || leaf == "." || leaf == ".." {
            return Err("nombre de archivo invalido");
        }

        for dir_name in components.iter() {
            if *dir_name == ".." {
                let entries = fat.read_dir_entries(cluster).map_err(|_| "no se pudo leer directorio padre")?;
                let mut parent = fat.root_cluster;
                for entry in entries.iter() {
                    if entry.matches_name("..") {
                        parent = if entry.cluster == 0 {
                            fat.root_cluster
                        } else {
                            entry.cluster
                        };
                        break;
                    }
                }
                cluster = parent;
                continue;
            }

            let entries = fat.read_dir_entries(cluster).map_err(|_| "no se pudo leer ruta")?;
            let mut next_cluster: Option<u32> = None;
            for entry in entries.iter() {
                if !entry.valid || entry.file_type != FileType::Directory {
                    continue;
                }
                if entry.matches_name(dir_name) || entry.full_name().eq_ignore_ascii_case(dir_name) {
                    next_cluster = Some(if entry.cluster == 0 {
                        fat.root_cluster
                    } else {
                        entry.cluster
                    });
                    break;
                }
            }

            cluster = next_cluster.ok_or("directorio no encontrado en ruta")?;
        }

        Ok((cluster, String::from(leaf)))
    }

    fn find_tag_fragment<'a>(layout: &'a str, tag: &str) -> Option<&'a str> {
        let mut needle = String::from("<");
        needle.push_str(tag);
        let start = layout.find(needle.as_str())?;
        let tail = &layout[start..];
        let end = tail.find('>')?;
        Some(&tail[..=end])
    }

    fn parse_tag_attr(tag_fragment: &str, attr_name: &str) -> Option<String> {
        let mut needle = String::with_capacity(attr_name.len() + 1);
        needle.push_str(attr_name);
        needle.push('=');

        let attr_pos = tag_fragment.find(needle.as_str())?;
        let suffix = &tag_fragment[attr_pos + needle.len()..];
        let quote = suffix.as_bytes().first().copied()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }

        let body = &suffix[1..];
        let end = body.bytes().position(|b| b == quote)?;
        Some(String::from(&body[..end]))
    }

    fn parse_hex_color(text: &str) -> Option<u32> {
        let trimmed = text.trim();
        let hex = if let Some(rest) = trimmed.strip_prefix('#') {
            rest
        } else {
            trimmed
        };
        if hex.len() != 6 {
            return None;
        }
        u32::from_str_radix(hex, 16).ok()
    }

    fn parse_rml_layout(layout_text: &str, source_name: &str) -> Result<AppRunnerLayoutSpec, String> {
        let app_tag = match Self::find_tag_fragment(layout_text, "App") {
            Some(tag) => tag,
            None => {
                return Err(String::from(
                    "RunApp error: RML invalido, falta etiqueta <App>.",
                ))
            }
        };

        let app_title = Self::parse_tag_attr(app_tag, "title")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| String::from(Self::filename_stem(source_name)));

        let theme_input = Self::parse_tag_attr(app_tag, "theme").unwrap_or_else(|| String::from("light"));
        let theme = if Self::ascii_lower(theme_input.as_str()) == "dark" {
            String::from("dark")
        } else {
            String::from("light")
        };
        let dark = theme == "dark";

        let mut background_color = if dark { 0x111827 } else { 0xF4F8FC };
        let mut header_color = if dark { 0x22D3EE } else { 0x1F4D78 };
        let mut body_color = if dark { 0xE5E7EB } else { 0x203345 };
        let mut button_color = if dark { 0x0EA5E9 } else { 0x2D89D6 };
        let mut header_text = app_title.clone();
        let mut body_text = alloc::format!("Layout {} loaded in App Runner.", source_name);
        let mut button_label = String::from("Run");

        if let Some(view_tag) = Self::find_tag_fragment(layout_text, "View") {
            if let Some(bg_text) = Self::parse_tag_attr(view_tag, "background") {
                if let Some(color) = Self::parse_hex_color(bg_text.as_str()) {
                    background_color = color;
                }
            }
        }

        if let Some(header_tag) = Self::find_tag_fragment(layout_text, "Header") {
            if let Some(text) = Self::parse_tag_attr(header_tag, "text") {
                if !text.trim().is_empty() {
                    header_text = text;
                }
            }
            if let Some(color_text) = Self::parse_tag_attr(header_tag, "color") {
                if let Some(color) = Self::parse_hex_color(color_text.as_str()) {
                    header_color = color;
                }
            }
        }

        if let Some(text_tag) = Self::find_tag_fragment(layout_text, "Text") {
            if let Some(text) = Self::parse_tag_attr(text_tag, "value")
                .or_else(|| Self::parse_tag_attr(text_tag, "text"))
            {
                if !text.trim().is_empty() {
                    body_text = text;
                }
            }
            if let Some(color_text) = Self::parse_tag_attr(text_tag, "color") {
                if let Some(color) = Self::parse_hex_color(color_text.as_str()) {
                    body_color = color;
                }
            }
        }

        if let Some(button_tag) = Self::find_tag_fragment(layout_text, "Button") {
            if let Some(label) = Self::parse_tag_attr(button_tag, "label") {
                if !label.trim().is_empty() {
                    button_label = label;
                }
            }
            if let Some(color_text) = Self::parse_tag_attr(button_tag, "color") {
                if let Some(color) = Self::parse_hex_color(color_text.as_str()) {
                    button_color = color;
                }
            }
        }

        Ok(AppRunnerLayoutSpec {
            app_title,
            theme,
            header_text,
            body_text,
            button_label,
            background_color,
            header_color,
            body_color,
            button_color,
        })
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return Some(0);
        }
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn extract_http_status_and_body_bytes(raw: &[u8]) -> (Option<u16>, Vec<u8>) {
        if !raw.starts_with(b"HTTP/") {
            return (None, raw.to_vec());
        }

        let body_start = if let Some(idx) = Self::find_bytes(raw, b"\r\n\r\n") {
            idx + 4
        } else if let Some(idx) = Self::find_bytes(raw, b"\n\n") {
            idx + 2
        } else {
            raw.len()
        };

        let status_code = if let Some(end) = raw.iter().position(|b| *b == b'\n') {
            let mut line = &raw[..end];
            if !line.is_empty() && line[line.len() - 1] == b'\r' {
                line = &line[..line.len() - 1];
            }
            match core::str::from_utf8(line) {
                Ok(status_line) => {
                    let mut parts = status_line.split_whitespace();
                    let _ = parts.next();
                    parts.next().and_then(|code| code.parse::<u16>().ok())
                }
                Err(_) => None,
            }
        } else {
            None
        };

        (status_code, raw[body_start..].to_vec())
    }

    fn read_u16_le(raw: &[u8], cursor: &mut usize) -> Option<u16> {
        if *cursor + 2 > raw.len() {
            return None;
        }
        let v = u16::from_le_bytes([raw[*cursor], raw[*cursor + 1]]);
        *cursor += 2;
        Some(v)
    }

    fn read_u32_le(raw: &[u8], cursor: &mut usize) -> Option<u32> {
        if *cursor + 4 > raw.len() {
            return None;
        }
        let v = u32::from_le_bytes([
            raw[*cursor],
            raw[*cursor + 1],
            raw[*cursor + 2],
            raw[*cursor + 3],
        ]);
        *cursor += 4;
        Some(v)
    }

    fn read_u32_be(raw: &[u8], cursor: &mut usize) -> Option<u32> {
        if *cursor + 4 > raw.len() {
            return None;
        }
        let v = u32::from_be_bytes([
            raw[*cursor],
            raw[*cursor + 1],
            raw[*cursor + 2],
            raw[*cursor + 3],
        ]);
        *cursor += 4;
        Some(v)
    }

    fn hex_nibble_to_ascii(nibble: u8) -> char {
        if nibble < 10 {
            (b'0' + nibble) as char
        } else {
            (b'a' + (nibble - 10)) as char
        }
    }

    fn sha256_hex(raw: &[u8]) -> String {
        let digest = Sha256::digest(raw);
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            out.push(Self::hex_nibble_to_ascii((byte >> 4) & 0x0F));
            out.push(Self::hex_nibble_to_ascii(byte & 0x0F));
        }
        out
    }

    fn is_ascii_hex_lower(raw: &str) -> bool {
        !raw.is_empty()
            && raw
                .bytes()
                .all(|b| (b'0'..=b'9').contains(&b) || (b'a'..=b'f').contains(&b))
    }

    fn verify_install_package_signature(
        package_name: &str,
        package_raw: &[u8],
        signature_text: &str,
    ) -> Result<(), String> {
        let mut saw_header = false;
        let mut algo: Option<String> = None;
        let mut sig_package: Option<String> = None;
        let mut sig_size: Option<usize> = None;
        let mut sig_sha256: Option<String> = None;
        let mut sig_field: Option<String> = None;

        for line in signature_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if !saw_header {
                if Self::ascii_lower(trimmed) != "redux-sig-v1" {
                    return Err(String::from("firma invalida (.sig header)."));
                }
                saw_header = true;
                continue;
            }

            let Some(eq_idx) = trimmed.find('=') else {
                continue;
            };
            let key = Self::ascii_lower(trimmed[..eq_idx].trim());
            let value = trimmed[eq_idx + 1..].trim();

            match key.as_str() {
                "algo" => algo = Some(String::from(value)),
                "package" => sig_package = Some(String::from(value)),
                "size" => {
                    if !value.is_empty() {
                        match value.parse::<usize>() {
                            Ok(v) => sig_size = Some(v),
                            Err(_) => {
                                return Err(String::from("firma invalida (.sig SIZE)."));
                            }
                        }
                    }
                }
                "sha256" => sig_sha256 = Some(String::from(value)),
                "sig" => sig_field = Some(String::from(value)),
                _ => {}
            }
        }

        if !saw_header {
            return Err(String::from("firma invalida (.sig vacia)."));
        }

        let algo_lower = Self::ascii_lower(algo.unwrap_or_default().trim());
        if algo_lower != "sha256" {
            return Err(alloc::format!("firma invalida (ALGO={} no soportado).", algo_lower));
        }

        if let Some(pkg) = sig_package {
            let pkg_trim = pkg.trim();
            if !pkg_trim.is_empty() && Self::ascii_lower(pkg_trim) != Self::ascii_lower(package_name) {
                return Err(alloc::format!(
                    "firma invalida (PACKAGE={} != {}).",
                    pkg_trim,
                    package_name
                ));
            }
        }

        if let Some(size) = sig_size {
            if size != package_raw.len() {
                return Err(alloc::format!(
                    "firma invalida (SIZE={} != {}).",
                    size,
                    package_raw.len()
                ));
            }
        }

        let expected_sha = Self::ascii_lower(sig_sha256.unwrap_or_default().trim());
        if expected_sha.len() != 64 || !Self::is_ascii_hex_lower(expected_sha.as_str()) {
            return Err(String::from("firma invalida (SHA256 no valido)."));
        }

        let computed_sha = Self::sha256_hex(package_raw);
        if computed_sha != expected_sha {
            return Err(alloc::format!(
                "firma invalida (SHA256 mismatch esperado={} real={}).",
                expected_sha,
                computed_sha
            ));
        }

        if let Some(sig) = sig_field {
            let sig_text = Self::ascii_lower(sig.trim());
            if !sig_text.is_empty() && sig_text != expected_sha {
                return Err(String::from("firma invalida (SIG mismatch)."));
            }
        }

        Ok(())
    }

    fn parse_zip_central_directory(
        raw: &[u8],
    ) -> Option<(Vec<(usize, usize, usize, u16)>, usize)> {
        if raw.len() < 22 {
            return None;
        }

        let mut eocd_pos = None;
        let mut pos = raw.len().saturating_sub(22);
        loop {
            if pos + 4 <= raw.len()
                && raw[pos] == b'P'
                && raw[pos + 1] == b'K'
                && raw[pos + 2] == 0x05
                && raw[pos + 3] == 0x06
            {
                eocd_pos = Some(pos);
                break;
            }
            if pos == 0 {
                break;
            }
            pos -= 1;
        }

        let eocd = eocd_pos?;
        let mut cursor = eocd + 4;
        let _disk_number = Self::read_u16_le(raw, &mut cursor)?;
        let _cd_start_disk = Self::read_u16_le(raw, &mut cursor)?;
        let _entries_on_disk = Self::read_u16_le(raw, &mut cursor)? as usize;
        let total_entries = Self::read_u16_le(raw, &mut cursor)? as usize;
        let central_size = Self::read_u32_le(raw, &mut cursor)? as usize;
        let central_offset = Self::read_u32_le(raw, &mut cursor)? as usize;
        let comment_len = Self::read_u16_le(raw, &mut cursor)? as usize;

        if eocd + 22 + comment_len > raw.len() {
            return None;
        }
        if central_offset > raw.len() || central_offset + central_size > raw.len() {
            return None;
        }

        let mut entries: Vec<(usize, usize, usize, u16)> = Vec::new();
        let mut cd_cursor = central_offset;
        for _ in 0..total_entries {
            if cd_cursor + 4 > raw.len() {
                return None;
            }
            let sig = u32::from_le_bytes([
                raw[cd_cursor],
                raw[cd_cursor + 1],
                raw[cd_cursor + 2],
                raw[cd_cursor + 3],
            ]);
            if sig != 0x0201_4B50 {
                return None;
            }
            cd_cursor += 4;

            let _version_made = Self::read_u16_le(raw, &mut cd_cursor)?;
            let _version_needed = Self::read_u16_le(raw, &mut cd_cursor)?;
            let _flags = Self::read_u16_le(raw, &mut cd_cursor)?;
            let method = Self::read_u16_le(raw, &mut cd_cursor)?;
            let _mod_time = Self::read_u16_le(raw, &mut cd_cursor)?;
            let _mod_date = Self::read_u16_le(raw, &mut cd_cursor)?;
            let _crc32 = Self::read_u32_le(raw, &mut cd_cursor)?;
            let comp_u32 = Self::read_u32_le(raw, &mut cd_cursor)?;
            let uncomp_u32 = Self::read_u32_le(raw, &mut cd_cursor)?;
            let name_len = Self::read_u16_le(raw, &mut cd_cursor)? as usize;
            let extra_len = Self::read_u16_le(raw, &mut cd_cursor)? as usize;
            let file_comment_len = Self::read_u16_le(raw, &mut cd_cursor)? as usize;
            let _disk_start = Self::read_u16_le(raw, &mut cd_cursor)?;
            let _int_attr = Self::read_u16_le(raw, &mut cd_cursor)?;
            let _ext_attr = Self::read_u32_le(raw, &mut cd_cursor)?;
            let local_offset_u32 = Self::read_u32_le(raw, &mut cd_cursor)?;

            // ZIP64 is not supported in this runtime parser.
            if comp_u32 == u32::MAX || uncomp_u32 == u32::MAX || local_offset_u32 == u32::MAX {
                return None;
            }

            let skip = name_len + extra_len + file_comment_len;
            if cd_cursor + skip > raw.len() {
                return None;
            }
            cd_cursor += skip;

            entries.push((
                local_offset_u32 as usize,
                comp_u32 as usize,
                uncomp_u32 as usize,
                method,
            ));
        }

        Some((entries, central_offset))
    }

    fn short_install_name(app_prefix4: &str, entry_path: &str, index1: usize) -> String {
        let leaf = entry_path.rsplit('/').next().unwrap_or(entry_path).trim();
        let ext_src = if let Some(dot) = leaf.rfind('.') {
            &leaf[dot + 1..]
        } else {
            "BIN"
        };
        let ext = Self::sanitize_short_component(ext_src, 3, "BIN");
        let stem = alloc::format!("{}{:04}", app_prefix4, index1 % 10000);
        let stem8 = Self::sanitize_short_component(stem.as_str(), 8, "APPFILE");
        alloc::format!("{}.{}", stem8, ext)
    }

    fn is_installable_zip_path(path: &str) -> bool {
        let mut normalized = String::with_capacity(path.len());
        for b in path.bytes() {
            if b == b'\\' {
                normalized.push('/');
            } else {
                normalized.push(b as char);
            }
        }

        let trimmed = normalized.trim_matches('/');
        if trimmed.is_empty() {
            return false;
        }

        let lower = Self::ascii_lower(trimmed);
        let leaf = lower.rsplit('/').next().unwrap_or("");
        if leaf.is_empty() {
            return false;
        }

        if lower.starts_with("__macosx/") {
            return false;
        }
        if leaf.starts_with("._") || leaf == ".ds_store" {
            return false;
        }

        true
    }

    fn parse_tar_octal(field: &[u8]) -> Option<usize> {
        let mut value: usize = 0;
        let mut saw_digit = false;
        for &b in field {
            if b == 0 || b == b' ' {
                if saw_digit {
                    break;
                }
                continue;
            }
            if !(b'0'..=b'7').contains(&b) {
                if saw_digit {
                    break;
                }
                continue;
            }
            saw_digit = true;
            value = value.saturating_mul(8).saturating_add((b - b'0') as usize);
        }
        Some(value)
    }

    fn extract_gzip_payload(raw: &[u8]) -> Result<Vec<u8>, &'static str> {
        Self::extract_gzip_payload_with_limit(raw, INSTALL_MAX_PACKAGE_BYTES)
    }

    fn extract_gzip_payload_with_limit(raw: &[u8], max_output: usize) -> Result<Vec<u8>, &'static str> {
        if raw.len() < 18 {
            return Err("GZIP invalido (tamano).");
        }
        if raw[0] != 0x1F || raw[1] != 0x8B {
            return Err("GZIP invalido (firma).");
        }
        if raw[2] != 8 {
            return Err("GZIP invalido (metodo no DEFLATE).");
        }

        let flags = raw[3];
        let mut cursor = 10usize;
        let trailer_start = raw.len().saturating_sub(8);

        if (flags & 0x04) != 0 {
            if cursor + 2 > trailer_start {
                return Err("GZIP invalido (extra len).");
            }
            let xlen = u16::from_le_bytes([raw[cursor], raw[cursor + 1]]) as usize;
            cursor += 2;
            if cursor + xlen > trailer_start {
                return Err("GZIP invalido (extra data).");
            }
            cursor += xlen;
        }

        if (flags & 0x08) != 0 {
            while cursor < trailer_start && raw[cursor] != 0 {
                cursor += 1;
            }
            if cursor >= trailer_start {
                return Err("GZIP invalido (fname).");
            }
            cursor += 1;
        }

        if (flags & 0x10) != 0 {
            while cursor < trailer_start && raw[cursor] != 0 {
                cursor += 1;
            }
            if cursor >= trailer_start {
                return Err("GZIP invalido (fcomment).");
            }
            cursor += 1;
        }

        if (flags & 0x02) != 0 {
            if cursor + 2 > trailer_start {
                return Err("GZIP invalido (fhcrc).");
            }
            cursor += 2;
        }

        if cursor >= trailer_start {
            return Err("GZIP invalido (payload vacio).");
        }

        let payload = &raw[cursor..trailer_start];
        let inflated = decompress_to_vec_with_limit(payload, max_output)
            .map_err(|_| "GZIP DEFLATE invalido.")?;

        let isize = u32::from_le_bytes([
            raw[trailer_start + 4],
            raw[trailer_start + 5],
            raw[trailer_start + 6],
            raw[trailer_start + 7],
        ]) as usize;
        if isize != (inflated.len() & 0xFFFF_FFFFusize) {
            return Err("GZIP invalido (tamano final inconsistente).");
        }

        Ok(inflated)
    }

    fn extract_deb_data_member<'a>(raw: &'a [u8]) -> Result<(&'a str, &'a [u8]), &'static str> {
        if raw.len() < 8 || &raw[..8] != b"!<arch>\n" {
            return Err("DEB invalido (ar global header).");
        }

        let mut cursor = 8usize;
        while cursor + 60 <= raw.len() {
            let hdr = &raw[cursor..cursor + 60];
            if hdr[58] != b'`' || hdr[59] != b'\n' {
                return Err("DEB invalido (ar member header).");
            }

            let mut name_end = 16usize;
            while name_end > 0 && (hdr[name_end - 1] == b' ' || hdr[name_end - 1] == 0) {
                name_end -= 1;
            }
            let mut name_bytes = &hdr[..name_end];
            if !name_bytes.is_empty() && name_bytes[name_bytes.len() - 1] == b'/' {
                name_bytes = &name_bytes[..name_bytes.len() - 1];
            }

            let name = core::str::from_utf8(name_bytes).unwrap_or("");
            let size_text = core::str::from_utf8(&hdr[48..58]).unwrap_or("").trim();
            let size = size_text.parse::<usize>().unwrap_or(0);

            cursor += 60;
            if cursor + size > raw.len() {
                return Err("DEB invalido (ar member size).");
            }

            let data = &raw[cursor..cursor + size];
            if name.starts_with("data.tar") {
                return Ok((name, data));
            }

            cursor += size;
            if (cursor & 1) != 0 {
                cursor += 1;
            }
        }

        Err("DEB invalido: no se encontro data.tar*.")
    }

    /// Extract data.tar* member in-place: shifts member data to the start of the Vec
    /// and truncates, using zero extra memory. Returns (member_name, is_gzip).
    fn extract_deb_data_member_inplace(raw: &mut Vec<u8>) -> Result<(String, bool), &'static str> {
        if raw.len() < 8 || &raw[..8] != b"!<arch>\n" {
            return Err("DEB invalido (ar global header).");
        }

        let mut cursor = 8usize;
        while cursor + 60 <= raw.len() {
            let hdr_start = cursor;
            // Read header fields before we mutate
            if raw[hdr_start + 58] != b'`' || raw[hdr_start + 59] != b'\n' {
                return Err("DEB invalido (ar member header).");
            }

            let mut name_end = 16usize;
            while name_end > 0 && (raw[hdr_start + name_end - 1] == b' ' || raw[hdr_start + name_end - 1] == 0) {
                name_end -= 1;
            }
            let mut name_len = name_end;
            if name_len > 0 && raw[hdr_start + name_len - 1] == b'/' {
                name_len -= 1;
            }
            let name = String::from(
                core::str::from_utf8(&raw[hdr_start..hdr_start + name_len])
                    .unwrap_or("")
            );

            let size_text = core::str::from_utf8(&raw[hdr_start + 48..hdr_start + 58])
                .unwrap_or("")
                .trim();
            let size = size_text.parse::<usize>().unwrap_or(0);

            let data_start = hdr_start + 60;
            if data_start + size > raw.len() {
                return Err("DEB invalido (ar member size).");
            }

            if name.starts_with("data.tar") {
                let is_gz = Self::ascii_lower(name.as_str()).ends_with(".tar.gz");
                // Shift member data to start of Vec in-place
                raw.copy_within(data_start..data_start + size, 0);
                raw.truncate(size);
                return Ok((name, is_gz));
            }

            cursor = data_start + size;
            if (cursor & 1) != 0 {
                cursor += 1;
            }
        }

        Err("DEB invalido: no se encontro data.tar*.")
    }

    fn probe_deb_data_member_name(raw: &[u8]) -> Result<Option<String>, &'static str> {
        if raw.len() < 8 {
            return Ok(None);
        }
        if &raw[..8] != b"!<arch>\n" {
            return Err("DEB invalido (ar global header).");
        }

        let mut cursor = 8usize;
        while cursor < raw.len() {
            if cursor + 60 > raw.len() {
                return Ok(None);
            }
            let hdr = &raw[cursor..cursor + 60];
            if hdr[58] != b'`' || hdr[59] != b'\n' {
                return Err("DEB invalido (ar member header).");
            }

            let mut name_end = 16usize;
            while name_end > 0 && (hdr[name_end - 1] == b' ' || hdr[name_end - 1] == 0) {
                name_end -= 1;
            }
            let mut name_bytes = &hdr[..name_end];
            if !name_bytes.is_empty() && name_bytes[name_bytes.len() - 1] == b'/' {
                name_bytes = &name_bytes[..name_bytes.len() - 1];
            }
            let name = core::str::from_utf8(name_bytes).unwrap_or("");

            let size_text = core::str::from_utf8(&hdr[48..58]).unwrap_or("").trim();
            let size = size_text.parse::<usize>().unwrap_or(0);

            cursor += 60;

            if name.starts_with("data.tar") {
                return Ok(Some(String::from(name)));
            }

            if cursor + size > raw.len() {
                return Ok(None);
            }
            cursor += size;
            if (cursor & 1) != 0 {
                if cursor >= raw.len() {
                    return Ok(None);
                }
                cursor += 1;
            }
        }

        Ok(None)
    }

    fn install_task_budget_bytes() -> usize {
        let heap = crate::allocator::heap_size_bytes();
        if heap == 0 {
            return INSTALL_MIN_TASK_BUDGET_BYTES;
        }
        core::cmp::max(heap / INSTALL_TASK_BUDGET_DIVISOR, INSTALL_MIN_TASK_BUDGET_BYTES)
    }

    fn estimate_install_working_set_bytes(
        package_bytes: usize,
        package_is_zip: bool,
        package_is_targz: bool,
        _package_is_deb: bool,
        package_is_exe: bool,
    ) -> usize {
        let mut factor = 1usize;
        if package_is_zip || package_is_targz || package_is_exe {
            // Compressed packages require source + expanded (×2).
            factor = 2;
        }
        // DEB: ar container -> data.tar(.gz) -> replaces buffer in-place, factor stays 1.
        package_bytes
            .saturating_mul(factor)
            .saturating_add(INSTALL_DEB_PREFLIGHT_BYTES)
    }

    fn try_alloc_zeroed(bytes: usize) -> Result<Vec<u8>, &'static str> {
        let mut buffer = Vec::new();
        if buffer.try_reserve_exact(bytes).is_err() {
            return Err("memoria insuficiente para buffer temporal.");
        }
        buffer.resize(bytes, 0);
        Ok(buffer)
    }

    fn try_copy_slice(raw: &[u8]) -> Result<Vec<u8>, &'static str> {
        let mut buffer = Vec::new();
        if buffer.try_reserve_exact(raw.len()).is_err() {
            return Err("memoria insuficiente para copiar buffer temporal.");
        }
        buffer.extend_from_slice(raw);
        Ok(buffer)
    }

    fn install_tar_archive(
        &mut self,
        win_id: usize,
        fat: &mut crate::fat32::Fat32,
        target_cluster: u32,
        app_tag4: &str,
        package_name: &str,
        tar_raw: &[u8],
        manifest: &mut String,
        files_written: &mut usize,
        shortcut_layout: &mut Option<String>,
        shortcut_linux_candidate: &mut Option<LinuxInstallShortcutCandidate>,
        runtime_targets: &mut Option<LinuxRuntimeTargets>,
        runtime_manifest: &mut String,
        runtime_files_written: &mut usize,
        runtime_stage_warned: &mut bool,
        out: &mut Vec<String>,
    ) {
        let mut cursor = 0usize;
        let mut index1 = 0usize;
        let mut parsed_files = 0usize;

        *manifest = alloc::format!("TAR INSTALL\nPACKAGE={}\n", package_name);

        while out.is_empty() && cursor + 512 <= tar_raw.len() {
            let header = &tar_raw[cursor..cursor + 512];
            let mut all_zero = true;
            for b in header {
                if *b != 0 {
                    all_zero = false;
                    break;
                }
            }
            if all_zero {
                break;
            }

            let name_end = header[..100].iter().position(|b| *b == 0).unwrap_or(100);
            let prefix_end = header[345..500]
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(155);

            let name = String::from_utf8_lossy(&header[..name_end]).into_owned();
            let prefix = String::from_utf8_lossy(&header[345..345 + prefix_end]).into_owned();
            let path_text = if prefix.is_empty() {
                name
            } else {
                alloc::format!("{}/{}", prefix, name)
            };

            let size = match Self::parse_tar_octal(&header[124..136]) {
                Some(v) => v,
                None => {
                    out.push(String::from("Install error: TAR corrupto (size octal)."));
                    break;
                }
            };

            let typeflag = header[156];
            let data_start = cursor + 512;
            let aligned = ((size + 511) / 512) * 512;
            if data_start + aligned > tar_raw.len() {
                out.push(String::from("Install error: TAR corrupto (payload fuera de rango)."));
                break;
            }

            if typeflag == 0 || typeflag == b'0' {
                parsed_files += 1;
                self.pump_ui_while_installing(win_id, parsed_files);
                if Self::is_installable_zip_path(path_text.as_str()) {
                    if size > INSTALL_MAX_EXPANDED_FILE_BYTES {
                        out.push(alloc::format!(
                            "Install error: TAR entry demasiado grande ({} bytes, entry {}).",
                            size,
                            path_text
                        ));
                        break;
                    }

                    index1 += 1;
                    let out_name = Self::short_install_name(app_tag4, path_text.as_str(), index1);
                    let payload = &tar_raw[data_start..data_start + size];
                    match fat.write_text_file_in_dir(target_cluster, out_name.as_str(), payload) {
                        Ok(()) => {
                            *files_written += 1;
                            if shortcut_layout.is_none()
                                && (Self::is_rml_file_name(path_text.as_str())
                                    || Self::is_rml_file_name(out_name.as_str()))
                            {
                                *shortcut_layout = Some(out_name.clone());
                            }
                            Self::consider_linux_shortcut_candidate(
                                path_text.as_str(),
                                out_name.as_str(),
                                payload,
                                shortcut_linux_candidate,
                            );
                            manifest.push_str(
                                alloc::format!("{:04} {} <- {}\n", index1, out_name, path_text)
                                    .as_str(),
                            );
                            if let Err(err) = Self::maybe_stage_linux_runtime_file(
                                fat,
                                path_text.as_str(),
                                out_name.as_str(),
                                payload,
                                runtime_targets,
                                runtime_manifest,
                                runtime_files_written,
                            ) {
                                if !*runtime_stage_warned {
                                    out.push(alloc::format!("Install runtime warning: {}", err));
                                    *runtime_stage_warned = true;
                                }
                            }
                            self.pump_ui_while_installing(win_id, *files_written);
                        }
                        Err(err) => {
                            out.push(alloc::format!("Install error writing {}: {}", out_name, err));
                            break;
                        }
                    }
                }
            }

            cursor = data_start + aligned;
        }

        if out.is_empty() && parsed_files == 0 {
            out.push(String::from("Install error: TAR sin archivos instalables."));
        }
    }

    fn is_rml_file_name(name: &str) -> bool {
        let lower = Self::ascii_lower(name.trim());
        lower.ends_with(".rml")
    }

    fn is_elf_payload(raw: &[u8]) -> bool {
        raw.len() >= 4 && &raw[..4] == b"\x7FELF"
    }

    fn is_pe_payload(raw: &[u8]) -> bool {
        raw.len() >= 2 && raw[0] == b'M' && raw[1] == b'Z'
    }

    fn linux_shortcut_rank(path_text: &str, phase1_ok: bool) -> u8 {
        let lower = Self::ascii_lower(path_text);
        let leaf = lower.rsplit('/').next().unwrap_or("");

        if leaf.ends_with(".so")
            || leaf.contains(".so.")
            || leaf.ends_with(".a")
            || leaf.ends_with(".la")
        {
            return u8::MAX;
        }
        // Never choose dynamic loader/helper blobs as default app entry.
        if leaf == "ld.bin"
            || leaf == "ld.so"
            || leaf.starts_with("ld-linux")
            || leaf.starts_with("ld-musl")
            || leaf.contains("loader")
        {
            return u8::MAX;
        }

        let mut rank = 90u8;
        if lower.starts_with("usr/bin/")
            || lower.starts_with("bin/")
            || lower.contains("/bin/")
        {
            rank = 10;
        } else if lower.starts_with("usr/sbin/")
            || lower.starts_with("sbin/")
            || lower.contains("/sbin/")
        {
            rank = 20;
        } else if lower.starts_with("opt/") {
            rank = 35;
        } else if lower.starts_with("usr/libexec/") {
            rank = 45;
        }

        if leaf.ends_with("test") || leaf.contains("debug") {
            rank = rank.saturating_add(15);
        }
        if phase1_ok {
            rank = rank.saturating_sub(4);
        }

        rank
    }

    fn consider_linux_shortcut_candidate(
        path_text: &str,
        out_name: &str,
        payload: &[u8],
        best_candidate: &mut Option<LinuxInstallShortcutCandidate>,
    ) {
        if !Self::is_elf_payload(payload) {
            return;
        }

        let report = match crate::linux_compat::inspect_elf64(payload) {
            Ok(v) => v,
            Err(_) => return,
        };
        if report.machine != 62 {
            return;
        }

        let mut mode: Option<LinuxInstallLaunchMode> = None;
        let mut interp_path: Option<String> = None;
        let mut needed: Vec<String> = Vec::new();
        let phase1_ok = crate::linux_compat::phase1_static_compatibility(&report).is_ok();
        if phase1_ok {
            mode = Some(LinuxInstallLaunchMode::Phase1Static);
        } else if let Ok(dynamic) = crate::linux_compat::inspect_dynamic_elf64(payload) {
            if crate::linux_compat::phase2_dynamic_compatibility(&report, &dynamic).is_ok() {
                mode = Some(LinuxInstallLaunchMode::Phase2Dynamic);
                interp_path = dynamic.interp_path;
                needed = dynamic.needed;
            }
        }
        let Some(mode) = mode else {
            return;
        };

        let rank = Self::linux_shortcut_rank(path_text, phase1_ok);
        if rank == u8::MAX {
            return;
        }

        let replace = match best_candidate.as_ref() {
            Some(existing) => rank < existing.rank,
            None => true,
        };
        if replace {
            *best_candidate = Some(LinuxInstallShortcutCandidate {
                exec_name: String::from(out_name),
                source_path: String::from(path_text),
                rank,
                mode,
                interp_path,
                needed,
            });
        }
    }

    fn is_png_file_name(name: &str) -> bool {
        let lower = Self::ascii_lower(name.trim());
        lower.ends_with(".png")
    }

    fn png_paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
        let a_i = a as i32;
        let b_i = b as i32;
        let c_i = c as i32;
        let p = a_i + b_i - c_i;
        let pa = (p - a_i).abs();
        let pb = (p - b_i).abs();
        let pc = (p - c_i).abs();
        if pa <= pb && pa <= pc {
            a
        } else if pb <= pc {
            b
        } else {
            c
        }
    }

    fn png_blend_white(channel: u8, alpha: u8) -> u8 {
        let ch = channel as u32;
        let a = alpha as u32;
        let bg = 255u32;
        ((ch * a + bg * (255 - a)) / 255) as u8
    }

    fn decode_png_to_rgb(raw: &[u8]) -> Result<(u32, u32, Vec<u32>), &'static str> {
        const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

        if raw.len() < PNG_SIG.len() || raw[..PNG_SIG.len()] != PNG_SIG {
            return Err("PNG invalido (firma).");
        }

        let mut cursor = PNG_SIG.len();
        let mut saw_ihdr = false;
        let mut saw_iend = false;

        let mut width = 0u32;
        let mut height = 0u32;
        let mut bit_depth = 0u8;
        let mut color_type = 0u8;
        let mut compression = 0u8;
        let mut filter = 0u8;
        let mut interlace = 0u8;
        let mut idat = Vec::new();

        while cursor + 8 <= raw.len() {
            let chunk_len = Self::read_u32_be(raw, &mut cursor)
                .ok_or("PNG corrupto (chunk len).")? as usize;
            if cursor + 4 > raw.len() {
                return Err("PNG corrupto (chunk tipo).");
            }

            let chunk_type = [
                raw[cursor],
                raw[cursor + 1],
                raw[cursor + 2],
                raw[cursor + 3],
            ];
            cursor += 4;

            if cursor + chunk_len + 4 > raw.len() {
                return Err("PNG corrupto (chunk data).");
            }
            let chunk_data = &raw[cursor..cursor + chunk_len];
            cursor += chunk_len;
            cursor += 4; // CRC

            match &chunk_type {
                b"IHDR" => {
                    if chunk_len != 13 {
                        return Err("PNG invalido (IHDR).");
                    }
                    let mut ihdr_cursor = 0usize;
                    width = Self::read_u32_be(chunk_data, &mut ihdr_cursor)
                        .ok_or("PNG invalido (IHDR width).")?;
                    height = Self::read_u32_be(chunk_data, &mut ihdr_cursor)
                        .ok_or("PNG invalido (IHDR height).")?;
                    if ihdr_cursor + 5 > chunk_data.len() {
                        return Err("PNG invalido (IHDR fields).");
                    }
                    bit_depth = chunk_data[ihdr_cursor];
                    color_type = chunk_data[ihdr_cursor + 1];
                    compression = chunk_data[ihdr_cursor + 2];
                    filter = chunk_data[ihdr_cursor + 3];
                    interlace = chunk_data[ihdr_cursor + 4];
                    saw_ihdr = true;
                }
                b"IDAT" => {
                    idat.extend_from_slice(chunk_data);
                    if idat.len() > IMAGE_VIEWER_MAX_FILE_BYTES {
                        return Err("PNG demasiado grande (IDAT).");
                    }
                }
                b"IEND" => {
                    saw_iend = true;
                    break;
                }
                _ => {}
            }
        }

        if !saw_ihdr {
            return Err("PNG invalido (IHDR ausente).");
        }
        if !saw_iend {
            return Err("PNG invalido (IEND ausente).");
        }
        if idat.is_empty() {
            return Err("PNG invalido (IDAT ausente).");
        }
        if width == 0 || height == 0 {
            return Err("PNG invalido (dimensiones).");
        }
        if compression != 0 || filter != 0 {
            return Err("PNG invalido (parametros).");
        }
        if interlace != 0 {
            return Err("PNG interlaced aun no soportado.");
        }
        if bit_depth != 8 {
            return Err("PNG bit depth no soportado (solo 8-bit).");
        }

        let channels = match color_type {
            0 => 1usize, // grayscale
            2 => 3usize, // RGB
            4 => 2usize, // grayscale + alpha
            6 => 4usize, // RGBA
            _ => return Err("PNG color type no soportado."),
        };

        let width_usize = width as usize;
        let height_usize = height as usize;
        let pixel_count = width_usize
            .checked_mul(height_usize)
            .ok_or("PNG dimensiones invalidas.")?;
        if pixel_count == 0 || pixel_count > IMAGE_VIEWER_MAX_PIXELS {
            return Err("PNG demasiado grande para visor.");
        }

        let row_bytes = width_usize
            .checked_mul(channels)
            .ok_or("PNG dimensiones invalidas.")?;
        let inflated_len = row_bytes
            .checked_add(1)
            .and_then(|v| v.checked_mul(height_usize))
            .ok_or("PNG dimensiones invalidas.")?;
        if inflated_len > IMAGE_VIEWER_MAX_INFLATED_BYTES {
            return Err("PNG demasiado grande al descomprimir.");
        }

        let inflated = decompress_to_vec_zlib_with_limit(idat.as_slice(), inflated_len)
            .map_err(|_| "PNG zlib/DEFLATE invalido.")?;
        if inflated.len() != inflated_len {
            return Err("PNG corrupto (tamano de datos).");
        }

        let mut recon = Vec::new();
        recon.resize(row_bytes * height_usize, 0);
        let mut src = 0usize;
        let bpp = channels;

        for row in 0..height_usize {
            if src >= inflated.len() {
                return Err("PNG corrupto (scanline).");
            }
            let filter_type = inflated[src];
            src += 1;
            let row_off = row * row_bytes;

            for col in 0..row_bytes {
                let raw_b = inflated[src + col];
                let left = if col >= bpp {
                    recon[row_off + col - bpp]
                } else {
                    0
                };
                let up = if row > 0 {
                    recon[row_off - row_bytes + col]
                } else {
                    0
                };
                let up_left = if row > 0 && col >= bpp {
                    recon[row_off - row_bytes + col - bpp]
                } else {
                    0
                };

                recon[row_off + col] = match filter_type {
                    0 => raw_b,
                    1 => raw_b.wrapping_add(left),
                    2 => raw_b.wrapping_add(up),
                    3 => raw_b.wrapping_add(((left as u16 + up as u16) / 2) as u8),
                    4 => raw_b.wrapping_add(Self::png_paeth_predictor(left, up, up_left)),
                    _ => return Err("PNG filtro no soportado."),
                };
            }

            src += row_bytes;
        }

        let mut pixels = Vec::with_capacity(pixel_count);
        for row in 0..height_usize {
            let row_off = row * row_bytes;
            for col in 0..width_usize {
                let idx = row_off + col * channels;
                let (r, g, b) = match color_type {
                    0 => {
                        let v = recon[idx];
                        (v, v, v)
                    }
                    2 => (recon[idx], recon[idx + 1], recon[idx + 2]),
                    4 => {
                        let v = recon[idx];
                        let a = recon[idx + 1];
                        let blend = Self::png_blend_white(v, a);
                        (blend, blend, blend)
                    }
                    6 => {
                        let a = recon[idx + 3];
                        (
                            Self::png_blend_white(recon[idx], a),
                            Self::png_blend_white(recon[idx + 1], a),
                            Self::png_blend_white(recon[idx + 2], a),
                        )
                    }
                    _ => return Err("PNG color type no soportado."),
                };
                pixels.push(((r as u32) << 16) | ((g as u32) << 8) | b as u32);
            }
        }

        Ok((width, height, pixels))
    }

    fn is_http_url(url: &str) -> bool {
        let lower = Self::ascii_lower(url.trim());
        lower.starts_with("http://") || lower.starts_with("https://")
    }

    fn volume_label_text(fat: &crate::fat32::Fat32) -> Option<String> {
        if fat.volume_label[0] == 0 {
            return None;
        }

        let mut end = fat.volume_label.len();
        while end > 0 && fat.volume_label[end - 1] == b' ' {
            end -= 1;
        }

        if end == 0 {
            return None;
        }

        match core::str::from_utf8(&fat.volume_label[..end]) {
            Ok(s) => Some(String::from(s)),
            Err(_) => None,
        }
    }

    fn volume_label_from_bytes(label: &[u8; 11]) -> Option<String> {
        if label[0] == 0 {
            return None;
        }

        let mut end = label.len();
        while end > 0 && label[end - 1] == b' ' {
            end -= 1;
        }
        if end == 0 {
            return None;
        }

        match core::str::from_utf8(&label[..end]) {
            Ok(s) => Some(String::from(s)),
            Err(_) => None,
        }
    }

    fn pump_ui_while_blocked_net(&mut self) {
        let mut moved = false;
        while let Some((dx, dy, _wheel_delta, _left_btn, _right_btn)) = crate::input::poll_mouse_uefi() {
            let max_x = self.width.saturating_sub(1) as i32;
            let max_y = self.height.saturating_sub(1) as i32;
            self.mouse_pos.x = self.mouse_pos.x.saturating_add(dx).clamp(0, max_x);
            self.mouse_pos.y = self.mouse_pos.y.saturating_add(dy).clamp(0, max_y);
            moved = true;
        }

        if moved {
            self.paint();
        }
    }

    fn install_debug_log(&mut self, win_id: usize, text: &str) {
        if !INSTALL_VERBOSE_DEBUG {
            return;
        }
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.add_output(text);
            win.render_terminal();
        }
        self.paint();
    }

    fn begin_install_progress_prompt(&mut self, package_name: &str) -> bool {
        if self.copy_progress_prompt.is_some() || self.clipboard_paste_job.is_some() {
            return false;
        }
        let title = alloc::format!(
            "Instalando {}",
            Self::trim_ascii_line(package_name.trim(), 26)
        );
        // Install progress model:
        // 0..1000 internal units, shown as 0..100% on the bar.
        self.begin_copy_progress_prompt(title.as_str(), 1000, 1, false);
        self.copy_progress_touch("Preparando instalacion...");
        true
    }

    fn install_progress_set_target(&mut self, target_units: usize, detail: Option<&str>) {
        if let Some(text) = detail {
            self.copy_progress_touch(text);
        }
        let Some(prompt) = self.copy_progress_prompt.as_ref() else {
            return;
        };
        let target = target_units.min(prompt.total_units);
        if target > prompt.done_units {
            self.copy_progress_advance_units(target - prompt.done_units);
        }
    }

    fn finish_install_progress_prompt(&mut self, success: bool) {
        if self.copy_progress_prompt.is_none() {
            return;
        }
        if success {
            self.install_progress_set_target(1000, Some("Instalacion completada."));
        } else {
            self.copy_progress_touch("Instalacion finalizada con error.");
        }
        self.paint();
        self.finish_copy_progress_prompt();
    }

    fn pump_ui_while_installing(&mut self, win_id: usize, files_seen: usize) {
        if files_seen == 0 {
            return;
        }

        if (files_seen % INSTALL_UI_PUMP_EVERY_FILES) == 0 {
            self.pump_ui_while_blocked_net();
        }

        if (files_seen % INSTALL_PROGRESS_LOG_EVERY_FILES) == 0 {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.add_output(
                    alloc::format!("Install: progreso {} archivos...", files_seen).as_str(),
                );
                win.render_terminal();
            }
            self.paint();
        }

        // When install prompt is active, keep it moving during long extract stages.
        if self.copy_progress_prompt.is_some() && self.clipboard_paste_job.is_none() {
            self.copy_progress_touch(alloc::format!("Instalando archivos... {}", files_seen).as_str());
            let (done, total) = match self.copy_progress_prompt.as_ref() {
                Some(p) => (p.done_units, p.total_units),
                None => (0usize, 0usize),
            };
            let soft_cap = total.saturating_sub(20); // reserve last 2% for finalization
            if done < soft_cap {
                self.copy_progress_advance_units(1);
            }
        }
    }

    fn pump_ui_while_linux_preflight(&mut self, win_id: usize, items_seen: usize) {
        if items_seen == 0 {
            return;
        }

        if (items_seen % LINUX_UI_PUMP_EVERY_ITEMS) == 0 {
            self.pump_ui_while_blocked_net();
        }

        if (items_seen % LINUX_PROGRESS_LOG_EVERY_ITEMS) == 0 {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.add_output(
                    alloc::format!("Linux run: preflight escaneando {} items...", items_seen)
                        .as_str(),
                );
                win.render_terminal();
            }
            self.paint();
        }
    }

    fn append_terminal_lines(&mut self, win_id: usize, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            for line in lines.iter() {
                win.add_output(line.as_str());
            }
            win.render_terminal();
        }
    }

    fn linux_step_status_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Some(step) = self.linux_step_container.as_ref() else {
            out.push(String::from("Linux proc: sin sesion activa."));
            return out;
        };

        out.push(alloc::format!(
            "Linux proc: stage={} active={} auto={} steps={} tick_start={} tick_last={}",
            LinuxStepContainer::stage_label(step.stage),
            if step.active { "yes" } else { "no" },
            if step.auto { "yes" } else { "no" },
            step.steps_done,
            step.started_tick,
            step.last_step_tick
        ));
        out.push(alloc::format!(
            "Linux proc: target='{}' leaf='{}' issues={} scanned={} runtime={}/{}",
            step.target_request,
            step.target_name,
            step.issues,
            step.items_scanned,
            step.wanted_cursor,
            step.runtime_wants.len()
        ));
        if !step.last_note.is_empty() {
            out.push(alloc::format!("Linux proc: note={}", step.last_note));
        }
        if !step.error.is_empty() {
            out.push(alloc::format!("Linux proc: error={}", step.error));
        }
        out
    }

    fn linux_step_start(&mut self, win_id: usize, target: &str, auto: bool) -> Vec<String> {
        let mut out = Vec::new();
        let trimmed = target.trim();
        if trimmed.is_empty() {
            out.push(String::from("Usage: linux proc start <programa.elf>"));
            return out;
        }
        self.linux_step_container = Some(LinuxStepContainer::new(win_id, trimmed, auto));
        out.push(alloc::format!(
            "Linux proc: inicio '{}' (modo={}).",
            trimmed,
            if auto { "auto" } else { "manual" }
        ));
        out.push(String::from(
            "Linux proc: ejecutando por pasos para no bloquear GUI.",
        ));
        out
    }

    fn linux_step_stop(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        match self.linux_step_container.as_mut() {
            Some(step) => {
                step.active = false;
                step.stage = LinuxProcStage::Stopped;
                step.last_note = String::from("detenido por usuario");
                out.push(String::from("Linux proc: detenido."));
            }
            None => {
                out.push(String::from("Linux proc: no hay sesion para detener."));
            }
        }
        out
    }

    fn discover_linux_runtime_lookup_quick(
        fat: &mut crate::fat32::Fat32,
    ) -> Option<LinuxRuntimeLookup> {
        use crate::fs::FileType;

        let root_entries = fat
            .read_dir_entries_limited(fat.root_cluster, LINUX_RUNTIME_LOOKUP_MAX_CLUSTERS)
            .ok()?;

        let mut runtime_root_cluster: Option<u32> = None;
        for entry in root_entries.iter() {
            if !entry.valid || entry.file_type != FileType::Directory {
                continue;
            }
            if entry.matches_name("LINUXRT") || entry.full_name().eq_ignore_ascii_case("LINUXRT") {
                runtime_root_cluster = Some(if entry.cluster == 0 {
                    fat.root_cluster
                } else {
                    entry.cluster
                });
                break;
            }
        }

        let rt_cluster = runtime_root_cluster?;
        let lib_cluster = Self::find_child_directory_cluster(fat, rt_cluster, "LIB");
        let lib64_cluster = Self::find_child_directory_cluster(fat, rt_cluster, "LIB64");
        let usr_cluster = Self::find_child_directory_cluster(fat, rt_cluster, "USR");
        let usr_lib_cluster = usr_cluster.and_then(|usr| Self::find_child_directory_cluster(fat, usr, "LIB"));
        let usr_lib64_cluster =
            usr_cluster.and_then(|usr| Self::find_child_directory_cluster(fat, usr, "LIB64"));

        Some(LinuxRuntimeLookup {
            root_cluster: rt_cluster,
            lib_cluster,
            lib64_cluster,
            usr_lib_cluster,
            usr_lib64_cluster,
        })
    }

    fn linux_step_find_runtime_dependency(
        fat: &mut crate::fat32::Fat32,
        lookup: LinuxRuntimeLookup,
        runtime_maps: &[(String, String, String)],
        dir_cache: &mut Vec<(u32, Vec<crate::fs::DirEntry>)>,
        wanted_name: &str,
        scanned_items: &mut usize,
    ) -> Option<crate::fs::DirEntry> {
        let mut source_hint: Option<String> = None;
        let mut candidate_names: Vec<String> = Vec::new();
        if let Some((short, source_norm)) =
            Self::resolve_linux_dependency_manifest_short(runtime_maps, wanted_name)
        {
            candidate_names.push(short);
            source_hint = Some(source_norm);
        }

        let wanted_norm = Self::normalize_linux_path(wanted_name);
        let wanted_leaf = Self::ascii_lower(Self::linux_path_leaf(wanted_norm.as_str()));
        if !wanted_leaf.is_empty()
            && !candidate_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(wanted_leaf.as_str()))
        {
            candidate_names.push(wanted_leaf);
        }
        if !candidate_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(wanted_name))
        {
            candidate_names.push(String::from(wanted_name));
        }

        let hint = source_hint.as_deref().unwrap_or(wanted_norm.as_str());
        let cluster_order = Self::runtime_lookup_cluster_order(lookup, Some(hint));
        for cluster in cluster_order.iter() {
            let cache_idx = match dir_cache
                .iter()
                .position(|(cached_cluster, _)| cached_cluster == cluster)
            {
                Some(idx) => idx,
                None => {
                    let entries = match fat.read_dir_entries_limited(
                        *cluster,
                        LINUX_RUNTIME_LOOKUP_MAX_CLUSTERS,
                    ) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    *scanned_items = scanned_items.saturating_add(entries.len().max(1));
                    dir_cache.push((*cluster, entries));
                    dir_cache.len() - 1
                }
            };
            let entries = dir_cache[cache_idx].1.as_slice();
            for candidate in candidate_names.iter() {
                if let Some(entry) = Self::find_dir_file_entry_by_name(entries, candidate) {
                    return Some(entry);
                }
            }
        }

        None
    }

    fn linux_step_advance(&mut self, max_steps: usize) -> Vec<String> {
        use crate::fs::FileType;

        let mut out = Vec::new();
        let mut step = match self.linux_step_container.take() {
            Some(v) => v,
            None => {
                out.push(String::from("Linux proc: sin sesion activa."));
                return out;
            }
        };

        if !step.active {
            self.linux_step_container = Some(step);
            return out;
        }

        let budget = max_steps.max(1).min(64);
        let mut executed = 0usize;
        let start_tick = crate::timer::ticks();
        let mut yielded_by_time_budget = false;

        while executed < budget && step.active {
            match step.stage {
                LinuxProcStage::ResolveTarget => {
                    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    if fat.bytes_per_sector == 0 && !fat.init() {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = String::from("FAT32 no disponible.");
                        step.last_note = step.error.clone();
                        out.push(String::from("Linux proc error: FAT32 no disponible."));
                        break;
                    }

                    let current_cwd_cluster = match self.windows.iter().find(|w| w.id == step.win_id) {
                        Some(win) => {
                            if win.current_dir_cluster == 0 {
                                fat.root_cluster
                            } else {
                                win.current_dir_cluster
                            }
                        }
                        None => fat.root_cluster,
                    };

                    let (target_dir, target_leaf) = match Self::resolve_terminal_parent_and_leaf(
                        fat,
                        current_cwd_cluster,
                        step.target_request.as_str(),
                    ) {
                        Ok(v) => v,
                        Err(err) => {
                            step.active = false;
                            step.stage = LinuxProcStage::Failed;
                            step.error = alloc::format!("ruta invalida ({})", err);
                            step.last_note = step.error.clone();
                            out.push(alloc::format!("Linux proc error: {}", step.error));
                            break;
                        }
                    };

                    let entries = match fat.read_dir_entries(target_dir) {
                        Ok(v) => v,
                        Err(err) => {
                            step.active = false;
                            step.stage = LinuxProcStage::Failed;
                            step.error = alloc::format!("no se pudo leer dir ({})", err);
                            step.last_note = step.error.clone();
                            out.push(alloc::format!("Linux proc error: {}", step.error));
                            break;
                        }
                    };

                    let mut found: Option<crate::fs::DirEntry> = None;
                    let mut files_seen = 0usize;
                    for entry in entries.iter() {
                        if !entry.valid || entry.file_type != FileType::File {
                            continue;
                        }
                        files_seen = files_seen.saturating_add(1);
                        if entry.matches_name(target_leaf.as_str())
                            || entry.full_name().eq_ignore_ascii_case(target_leaf.as_str())
                        {
                            found = Some(*entry);
                        }
                    }

                    let Some(target_entry) = found else {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = String::from("ELF no encontrado.");
                        step.last_note = step.error.clone();
                        out.push(String::from(
                            "Linux proc error: archivo ELF no encontrado en directorio destino.",
                        ));
                        break;
                    };

                    step.target_dir = target_dir;
                    step.target_leaf = target_leaf;
                    step.target_name = target_entry.full_name();
                    step.target_entry = Some(target_entry);
                    step.current_entries = entries;
                    step.items_scanned = files_seen;
                    step.stage = LinuxProcStage::ReadMainElf;
                    step.last_note = alloc::format!(
                        "target resuelto {} ({} bytes)",
                        step.target_name,
                        target_entry.size
                    );
                    out.push(alloc::format!(
                        "Linux proc: target {} listo ({} bytes).",
                        step.target_name,
                        target_entry.size
                    ));
                }
                LinuxProcStage::ReadMainElf => {
                    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    let Some(entry) = step.target_entry else {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = String::from("target_entry ausente.");
                        step.last_note = step.error.clone();
                        out.push(String::from("Linux proc error: target_entry ausente."));
                        break;
                    };
                    if entry.size == 0 {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = String::from("ELF vacio.");
                        step.last_note = step.error.clone();
                        out.push(String::from("Linux proc error: ELF vacio."));
                        break;
                    }
                    if entry.size as usize > crate::linux_compat::ELF_MAX_FILE_BYTES {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = alloc::format!(
                            "ELF demasiado grande (max {} bytes).",
                            crate::linux_compat::ELF_MAX_FILE_BYTES
                        );
                        step.last_note = step.error.clone();
                        out.push(alloc::format!("Linux proc error: {}", step.error));
                        break;
                    }
                    if entry.cluster < 2 {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = String::from("cluster invalido.");
                        step.last_note = step.error.clone();
                        out.push(String::from("Linux proc error: cluster invalido."));
                        break;
                    }

                    let mut raw = Vec::new();
                    raw.resize(entry.size as usize, 0);
                    match fat.read_file_sized(entry.cluster, entry.size as usize, &mut raw) {
                        Ok(len) => raw.truncate(len),
                        Err(err) => {
                            step.active = false;
                            step.stage = LinuxProcStage::Failed;
                            step.error = alloc::format!("read main fallo ({})", err);
                            step.last_note = step.error.clone();
                            out.push(alloc::format!("Linux proc error: {}", step.error));
                            break;
                        }
                    }

                    step.raw = raw;
                    step.stage = LinuxProcStage::InspectMain;
                    step.last_note = alloc::format!("main cargado ({} bytes)", step.raw.len());
                    out.push(alloc::format!(
                        "Linux proc: main ELF cargado ({} bytes).",
                        step.raw.len()
                    ));
                }
                LinuxProcStage::InspectMain => {
                    if Self::is_pe_payload(step.raw.as_slice()) {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = String::from("PE/COFF detectado (.EFI/.EXE), no ELF Linux.");
                        step.last_note = step.error.clone();
                        out.push(alloc::format!("Linux proc error: {}", step.error));
                        break;
                    }

                    let report = match crate::linux_compat::inspect_elf64(step.raw.as_slice()) {
                        Ok(v) => v,
                        Err(err) => {
                            step.active = false;
                            step.stage = LinuxProcStage::Failed;
                            step.error = alloc::format!("inspect fallo ({})", err);
                            step.last_note = step.error.clone();
                            out.push(alloc::format!("Linux proc error: {}", step.error));
                            break;
                        }
                    };

                    out.push(alloc::format!(
                        "Linux proc inspect: Type={} Machine={} Entry={:#x} PT_LOAD={} PT_DYNAMIC={} PT_INTERP={}",
                        crate::linux_compat::elf_type_name(report.e_type),
                        crate::linux_compat::machine_name(report.machine),
                        report.entry,
                        report.load_segments.len(),
                        if report.has_dynamic { "yes" } else { "no" },
                        if report.has_interp { "yes" } else { "no" }
                    ));

                    let phase1 = crate::linux_compat::phase1_static_compatibility(&report);
                    if phase1.is_ok() {
                        step.active = false;
                        step.stage = LinuxProcStage::Complete;
                        step.last_note = String::from("ELF ET_EXEC estatico (fase1).");
                        out.push(String::from(
                            "Linux proc: ET_EXEC estatico detectado; contenedor dinamico no requerido.",
                        ));
                        break;
                    }

                    if !report.has_dynamic {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = String::from("ELF no tiene PT_DYNAMIC.");
                        step.last_note = step.error.clone();
                        out.push(String::from("Linux proc error: ELF no tiene PT_DYNAMIC."));
                        break;
                    }

                    step.stage = LinuxProcStage::InspectDynamic;
                    step.last_note = String::from("inspect main listo");
                }
                LinuxProcStage::InspectDynamic => {
                    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    let report = match crate::linux_compat::inspect_elf64(step.raw.as_slice()) {
                        Ok(v) => v,
                        Err(err) => {
                            step.active = false;
                            step.stage = LinuxProcStage::Failed;
                            step.error = alloc::format!("re-inspect fallo ({})", err);
                            step.last_note = step.error.clone();
                            out.push(alloc::format!("Linux proc error: {}", step.error));
                            break;
                        }
                    };

                    let dynamic = match crate::linux_compat::inspect_dynamic_elf64(step.raw.as_slice()) {
                        Ok(v) => v,
                        Err(err) => {
                            step.active = false;
                            step.stage = LinuxProcStage::Failed;
                            step.error = alloc::format!("inspect dynamic fallo ({})", err);
                            step.last_note = step.error.clone();
                            out.push(alloc::format!("Linux proc error: {}", step.error));
                            break;
                        }
                    };

                    if let Err(err) = crate::linux_compat::phase2_dynamic_compatibility(&report, &dynamic) {
                        step.active = false;
                        step.stage = LinuxProcStage::Failed;
                        step.error = alloc::format!("phase2 no compatible ({})", err);
                        step.last_note = step.error.clone();
                        out.push(alloc::format!("Linux proc error: {}", step.error));
                        break;
                    }

                    let launch_manifest = Self::load_linux_launch_manifest_for_exec(
                        fat,
                        step.current_entries.as_slice(),
                        step.target_name.as_str(),
                    );
                    let mut launch_interp_hint = dynamic.interp_path.clone();
                    let mut launch_needed_hint = dynamic.needed.clone();
                    step.launch_hint_from_manifest = false;
                    step.launch_manifest_file = None;
                    if let Some(metadata) = launch_manifest.as_ref() {
                        let mut used_manifest = false;
                        if let Some(interp) = metadata.interp_path.as_deref() {
                            if !interp.trim().is_empty() {
                                launch_interp_hint = Some(String::from(interp.trim()));
                                used_manifest = true;
                            }
                        }
                        if metadata.needed_declared.is_some() {
                            launch_needed_hint.clear();
                            for needed in metadata.needed.iter() {
                                if needed.trim().is_empty()
                                    || launch_needed_hint
                                        .iter()
                                        .any(|existing| existing.eq_ignore_ascii_case(needed.as_str()))
                                {
                                    continue;
                                }
                                launch_needed_hint.push(needed.clone());
                            }
                            used_manifest = true;
                        } else if !metadata.needed.is_empty() {
                            launch_needed_hint = metadata.needed.clone();
                            used_manifest = true;
                        }
                        if used_manifest {
                            step.launch_hint_from_manifest = true;
                            step.launch_manifest_file = Some(metadata.file_name.clone());
                            out.push(alloc::format!(
                                "Linux proc: metadata launch {} aplicada (interp={} deps={}).",
                                metadata.file_name,
                                if launch_interp_hint.is_some() { "yes" } else { "no" },
                                launch_needed_hint.len()
                            ));
                        }
                    }

                    step.launch_interp_hint = launch_interp_hint.clone();
                    step.launch_needed_hint = launch_needed_hint.clone();
                    step.interp_path = launch_interp_hint.clone();
                    step.needed = launch_needed_hint.clone();
                    step.runtime_wants.clear();
                    if let Some(interp) = launch_interp_hint.as_deref() {
                        step.runtime_wants.push(String::from(interp));
                    }
                    for needed in launch_needed_hint.iter() {
                        if step
                            .runtime_wants
                            .iter()
                            .any(|existing| existing.eq_ignore_ascii_case(needed.as_str()))
                        {
                            continue;
                        }
                        step.runtime_wants.push(needed.clone());
                    }

                    if let Some(map) = Self::load_manifest_for_installed_exec(
                        fat,
                        step.current_entries.as_slice(),
                        step.target_name.as_str(),
                    ) {
                        step.install_manifest_map = map;
                    } else {
                        step.install_manifest_map.clear();
                    }

                    if let Some(lookup) = Self::discover_linux_runtime_lookup_quick(fat) {
                        let root_entries = fat
                            .read_dir_entries_limited(
                                lookup.root_cluster,
                                LINUX_RUNTIME_LOOKUP_MAX_CLUSTERS,
                            )
                            .unwrap_or_default();
                        step.runtime_manifest_map = Self::load_runtime_manifest_mappings_lite(
                            fat,
                            root_entries.as_slice(),
                            LINUX_RUNTIME_LOOKUP_MAX_MANIFESTS,
                        );
                        step.items_scanned = step.items_scanned.saturating_add(root_entries.len());
                        step.runtime_lookup = Some(lookup);
                        step.runtime_dir_cache.clear();
                        if !root_entries.is_empty() {
                            step.runtime_dir_cache
                                .push((lookup.root_cluster, root_entries));
                        }
                        step.wanted_cursor = 0;
                        step.stage = LinuxProcStage::CollectRuntime;
                        if self.linux_runtime_lookup_enabled {
                            step.last_note = alloc::format!(
                                "runtime lookup deep preparado ({} deps)",
                                step.runtime_wants.len()
                            );
                            out.push(alloc::format!(
                                "Linux proc: runtime DEEP por pasos ({} deps).",
                                step.runtime_wants.len()
                            ));
                        } else {
                            step.last_note = alloc::format!(
                                "runtime lookup quick preparado ({} deps)",
                                step.runtime_wants.len()
                            );
                            out.push(alloc::format!(
                                "Linux proc: runtime QUICK por pasos ({} deps, sin escaneo global).",
                                step.runtime_wants.len()
                            ));
                        }
                    } else {
                        step.runtime_lookup = None;
                        step.runtime_dir_cache.clear();
                        step.stage = LinuxProcStage::Summarize;
                        step.last_note = String::from("sin /LINUXRT; resumen local");
                        out.push(String::from(
                            "Linux proc: /LINUXRT no detectado; resumen solo con archivos locales.",
                        ));
                    }
                }
                LinuxProcStage::CollectRuntime => {
                    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    let Some(lookup) = step.runtime_lookup else {
                        step.stage = LinuxProcStage::Summarize;
                        step.last_note = String::from("lookup runtime no disponible");
                        continue;
                    };

                    if step.wanted_cursor >= step.runtime_wants.len() {
                        step.stage = LinuxProcStage::Summarize;
                        step.last_note = String::from("runtime collect completo");
                        out.push(alloc::format!(
                            "Linux proc: runtime collect completo ({} archivos).",
                            step.runtime_entries.len()
                        ));
                        continue;
                    }

                    let wanted = step.runtime_wants[step.wanted_cursor].clone();
                    let found = Self::linux_step_find_runtime_dependency(
                        fat,
                        lookup,
                        step.runtime_manifest_map.as_slice(),
                        &mut step.runtime_dir_cache,
                        wanted.as_str(),
                        &mut step.items_scanned,
                    );
                    if let Some(entry) = found {
                        let name = entry.full_name();
                        let duplicate = step.runtime_entries.iter().any(|existing| {
                            existing.cluster == entry.cluster
                                && existing.size == entry.size
                                && existing.full_name().eq_ignore_ascii_case(name.as_str())
                        });
                        if !duplicate {
                            step.runtime_entries.push(entry);
                        }
                        out.push(alloc::format!("Linux proc runtime: ok {} -> {}", wanted, name));
                    } else {
                        out.push(alloc::format!("Linux proc runtime: missing {}", wanted));
                    }
                    step.wanted_cursor = step.wanted_cursor.saturating_add(1);
                    if step.wanted_cursor >= step.runtime_wants.len() {
                        step.stage = LinuxProcStage::Summarize;
                        step.last_note = String::from("runtime collect completo");
                    }
                }
                LinuxProcStage::Summarize => {
                    let mut dependency_entries = step.current_entries.clone();
                    for entry in step.runtime_entries.iter() {
                        let name = entry.full_name();
                        let duplicate = dependency_entries.iter().any(|existing| {
                            existing.cluster == entry.cluster
                                && existing.size == entry.size
                                && existing.full_name().eq_ignore_ascii_case(name.as_str())
                        });
                        if !duplicate {
                            dependency_entries.push(*entry);
                        }
                    }

                    let mut combined_manifest_map: Vec<(String, String, String)> = Vec::new();
                    for item in step.install_manifest_map.iter() {
                        combined_manifest_map.push((item.0.clone(), item.1.clone(), item.2.clone()));
                    }
                    for item in step.runtime_manifest_map.iter() {
                        let duplicate = combined_manifest_map.iter().any(|(short, source_norm, _)| {
                            short.eq_ignore_ascii_case(item.0.as_str()) && source_norm == &item.1
                        });
                        if !duplicate {
                            combined_manifest_map.push((item.0.clone(), item.1.clone(), item.2.clone()));
                        }
                    }
                    let combined_manifest_ref = if combined_manifest_map.is_empty() {
                        None
                    } else {
                        Some(combined_manifest_map.as_slice())
                    };

                    let mut issues = 0usize;
                    if let Some(interp_src) = step.interp_path.as_deref() {
                        match Self::resolve_linux_dependency_name(
                            dependency_entries.as_slice(),
                            combined_manifest_ref,
                            interp_src,
                        ) {
                            Some(local) => {
                                out.push(alloc::format!("Linux proc interp: {} -> {}", interp_src, local));
                            }
                            None => {
                                out.push(alloc::format!("Linux proc interp missing: {}", interp_src));
                                issues = issues.saturating_add(1);
                            }
                        }
                    }

                    for needed in step.needed.iter() {
                        match Self::resolve_linux_dependency_name(
                            dependency_entries.as_slice(),
                            combined_manifest_ref,
                            needed.as_str(),
                        ) {
                            Some(local) => {
                                out.push(alloc::format!("Linux proc needed ok: {} -> {}", needed, local));
                            }
                            None => {
                                out.push(alloc::format!("Linux proc needed missing: {}", needed));
                                issues = issues.saturating_add(1);
                            }
                        }
                    }

                    step.issues = issues;
                    step.active = false;
                    step.stage = LinuxProcStage::Complete;
                    if issues == 0 {
                        step.last_note = String::from("preflight dinamico completo");
                        out.push(String::from(
                            "Linux proc: preflight dinamico completo (main+interp+deps).",
                        ));
                    } else {
                        step.last_note = alloc::format!("preflight incompleto issues={}", issues);
                        out.push(alloc::format!(
                            "Linux proc: preflight incompleto (issues={}).",
                            issues
                        ));
                    }
                }
                LinuxProcStage::Complete | LinuxProcStage::Failed | LinuxProcStage::Stopped => {
                    step.active = false;
                }
            }

            step.steps_done = step.steps_done.saturating_add(1);
            step.last_step_tick = crate::timer::ticks();
            executed += 1;
            if step.active
                && crate::timer::ticks().saturating_sub(start_tick) >= LINUX_STEP_CMD_TICK_BUDGET
            {
                yielded_by_time_budget = true;
                break;
            }
        }

        if yielded_by_time_budget && !step.auto {
            out.push(String::from(
                "Linux proc: pausa por presupuesto de tiempo; usa status/step para continuar.",
            ));
        }
        if executed == 0 && step.active {
            out.push(String::from("Linux proc: sin avance (reintentar)."));
        }
        self.linux_step_container = Some(step);
        out
    }

    fn service_linux_step_container(&mut self) {
        if self.linux_step_busy {
            return;
        }
        let (active, auto, win_id) = match self.linux_step_container.as_ref() {
            Some(step) => (step.active, step.auto, step.win_id),
            None => return,
        };
        if !active || !auto {
            return;
        }

        self.linux_step_busy = true;
        let lines = self.linux_step_advance(1);
        self.linux_step_busy = false;

        self.append_terminal_lines(win_id, lines.as_slice());
    }

    fn linux_decode_status_ascii(bytes: &[u8], len: usize) -> String {
        let mut out = String::new();
        let max_len = len.min(bytes.len());
        let mut i = 0usize;
        while i < max_len {
            let b = bytes[i];
            if b == 0 {
                break;
            }
            if b == b'\t' || (0x20..=0x7E).contains(&b) {
                out.push(b as char);
            } else {
                out.push('?');
            }
            i += 1;
        }
        if out.is_empty() {
            String::from("sin estado")
        } else {
            out
        }
    }

    fn ensure_linux_bridge_window(&mut self) -> usize {
        if let Some(id) = self.linux_bridge_window_id {
            if self.windows.iter().any(|w| w.id == id) {
                return id;
            }
            self.linux_bridge_window_id = None;
        }
        // Preserve current focus so runloop/bridge updates don't steal keyboard
        // from terminal while user types status/stop commands.
        let previous_active = self.active_window_id;
        let id = self.create_linux_bridge_window("Linux Bridge", 170, 70, 860, 560);
        if let Some(prev_id) = previous_active {
            if self.windows.iter().any(|w| w.id == prev_id) {
                self.active_window_id = Some(prev_id);
            }
        }
        id
    }

    fn service_linux_bridge_window(&mut self) {
        let status = crate::syscall::linux_gfx_bridge_status();
        if !status.active {
            if let Some(id) = self.linux_bridge_window_id {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
                    win.set_linux_bridge_status("Bridge inactivo.");
                }
            }
            return;
        }

        if !status.dirty && status.frame_seq == self.linux_bridge_last_seq {
            return;
        }

        let pixel_count = (status.width as usize).saturating_mul(status.height as usize);
        if pixel_count == 0 {
            return;
        }

        let mut pixels = Vec::new();
        pixels.resize(pixel_count, 0);
        let Some((frame_w, frame_h, frame_seq)) =
            crate::syscall::linux_gfx_bridge_copy_frame(pixels.as_mut_slice())
        else {
            return;
        };
        self.linux_bridge_last_seq = frame_seq;

        let status_text = Self::linux_decode_status_ascii(status.status.as_slice(), status.status_len);
        let win_id = self.ensure_linux_bridge_window();
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.set_linux_bridge_frame(
                "SDL/X11 subset",
                frame_w,
                frame_h,
                pixels,
                status_text.as_str(),
            );
        }
    }

    fn linux_runloop_push_runtime_path(paths: &mut Vec<(String, u64)>, alias: &str, size: u64) {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            return;
        }
        if paths
            .iter()
            .any(|(existing, _)| existing.eq_ignore_ascii_case(trimmed))
        {
            return;
        }
        paths.push((String::from(trimmed), size));
    }

    fn linux_runloop_push_blob_job(
        jobs: &mut Vec<LinuxBlobJob>,
        alias: &str,
        size: u64,
        source: LinuxBlobSource,
    ) {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            return;
        }
        if jobs
            .iter()
            .any(|existing| existing.path_alias.eq_ignore_ascii_case(trimmed))
        {
            return;
        }
        jobs.push(LinuxBlobJob {
            path_alias: String::from(trimmed),
            size,
            source,
        });
    }

    fn linux_runloop_status_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Some(run) = self.linux_runloop_container.as_ref() else {
            out.push(String::from("Linux runloop: sin sesion activa."));
            return out;
        };

        out.push(alloc::format!(
            "Linux runloop: stage={} active={} auto={} steps={} start_tick={} last_tick={}",
            LinuxRunLoopContainer::stage_label(run.stage),
            if run.active { "yes" } else { "no" },
            if run.auto { "yes" } else { "no" },
            run.steps_done,
            run.started_tick,
            run.last_step_tick
        ));
        out.push(alloc::format!(
            "Linux runloop: target='{}' argc={} session={} runtime_paths={}/{} runtime_blobs={}/{} slices={} calls={} irq_preempts={} last_errno={} mode={} stalled={}/{}",
            run.target_request,
            run.argv_items.len(),
            run.session_id,
            run.runtime_paths_registered,
            run.runtime_paths.len(),
            run.runtime_blobs_registered,
            run.runtime_blob_jobs.len(),
            run.run_slices,
            run.run_calls,
            crate::privilege::linux_real_slice_irq_preempts(),
            run.last_slice_errno,
            if run.real_transfer_guarded { "real-slice" } else { "prep" },
            run.stalled_slices,
            LINUX_RUNLOOP_GUARDED_STALL_TIMEOUT_SLICES
        ));
        out.push(alloc::format!(
            "Linux runloop: desktop irq timer active={} irq0_count={}",
            if crate::desktop_irq_timer_active() { "yes" } else { "no" },
            crate::interrupts::irq0_count()
        ));
        let frame_gap = if run.e2e_frame_advances == 0 {
            run.run_slices
        } else {
            run.run_slices.saturating_sub(run.e2e_last_frame_advance_slice)
        };
        out.push(alloc::format!(
            "Linux runloop: e2e validated={} connected_streak={} ready_streak={} frame_advances={} frame_gap={} last_frame_seq={} seen_connected={} seen_ready={} ready_since={} validation_slice={} post_unready_streak={} regressions={}",
            if run.e2e_validated { "yes" } else { "no" },
            run.e2e_connected_streak,
            run.e2e_ready_streak,
            run.e2e_frame_advances,
            frame_gap,
            run.e2e_last_frame_seq,
            if run.e2e_seen_connected { "yes" } else { "no" },
            if run.e2e_seen_ready { "yes" } else { "no" },
            run.e2e_ready_since_slice,
            run.e2e_validation_slice,
            run.e2e_post_validate_unready_streak,
            run.e2e_regressions
        ));
        out.push(alloc::format!(
            "Linux runloop: progress overall={}%% stage={}%%",
            run.progress_overall,
            run.progress_stage
        ));
        let gfx_status = crate::syscall::linux_gfx_bridge_status();
        out.push(alloc::format!(
            "Linux runloop: bridge direct_present={} frame_seq={} dirty={} events={}",
            if gfx_status.direct_present { "yes" } else { "no" },
            gfx_status.frame_seq,
            if gfx_status.dirty { "yes" } else { "no" },
            gfx_status.event_count
        ));
        out.push(alloc::format!(
            "Linux runloop: hw syscall gateway={} priv_phase={}",
            if crate::privilege::syscall_bridge_ready() {
                "ready"
            } else {
                "not-ready"
            },
            crate::privilege::current_phase()
        ));
        if run.session_id != 0 {
            let status = crate::syscall::linux_shim_status();
            let last_sys_name = crate::syscall::linux_syscall_name(status.last_sysno);
            let last_errno_name = crate::syscall::linux_errno_name(status.last_errno);
            out.push(alloc::format!(
                "Linux runloop: shim last={}({}) result={} errno={}({}) watchdog={}",
                last_sys_name,
                status.last_sysno,
                status.last_result,
                status.last_errno,
                last_errno_name,
                if status.watchdog_triggered { "yes" } else { "no" }
            ));
            if let Some(diag) = Self::linux_shim_path_diag_line(&status) {
                out.push(diag);
            }

            let x11 = crate::syscall::linux_x11_socket_status();
            out.push(alloc::format!(
                "Linux runloop: x11 endpoint={} connected={} ready={} handshake={} last_errno={}",
                x11.endpoint_count,
                x11.connected_count,
                x11.ready_count,
                x11.handshake_count,
                x11.last_error
            ));
            if x11.last_path_len > 0 {
                out.push(alloc::format!(
                    "Linux runloop: x11 path={}",
                    Self::linux_decode_status_ascii(x11.last_path.as_slice(), x11.last_path_len)
                ));
            }
            out.push(alloc::format!(
                "Linux runloop: unix connect errno={} path={}",
                x11.last_unix_connect_errno,
                Self::linux_decode_status_ascii(
                    x11.last_unix_connect_path.as_slice(),
                    x11.last_unix_connect_len
                )
            ));
            if x11.endpoint_count == 0
                && x11.connected_count == 0
                && x11.ready_count == 0
                && x11.handshake_count == 0
            {
                out.push(String::from(
                    "Linux runloop: sin intento X11 (cliente no abrio /tmp/.X11-unix/Xn).",
                ));
            }
        }
        if !run.last_note.is_empty() {
            out.push(alloc::format!("Linux runloop: note={}", run.last_note));
        }
        if !run.error.is_empty() {
            out.push(alloc::format!("Linux runloop: error={}", run.error));
        }
        out
    }

    fn linux_runloop_e2e_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Some(run) = self.linux_runloop_container.as_ref() else {
            out.push(String::from("Linux runloop e2e: sin sesion activa."));
            return out;
        };

        let frame_gap = if run.e2e_frame_advances == 0 {
            run.run_slices
        } else {
            run.run_slices.saturating_sub(run.e2e_last_frame_advance_slice)
        };
        let sustain_slices = if run.e2e_validation_slice == 0 {
            0
        } else {
            run.run_slices.saturating_sub(run.e2e_validation_slice)
        };
        let post_validate_frames = run
            .e2e_frame_advances
            .saturating_sub(run.e2e_validation_frame_advances);

        if run.e2e_validated {
            out.push(alloc::format!(
                "Linux runloop e2e: PASS (connected_streak={} ready_streak={} frame_advances={} frame_gap={} post_frames={} sustain_slices={} unready_streak={} regressions={} slices={} calls={}).",
                run.e2e_connected_streak,
                run.e2e_ready_streak,
                run.e2e_frame_advances,
                frame_gap,
                post_validate_frames,
                sustain_slices,
                run.e2e_post_validate_unready_streak,
                run.e2e_regressions,
                run.run_slices,
                run.run_calls
            ));
        } else if run.stage == LinuxRunLoopStage::Failed {
            out.push(alloc::format!(
                "Linux runloop e2e: FAIL (stage=FAILED sustain_slices={} post_frames={} regressions={} slices={} calls={} error='{}').",
                sustain_slices,
                post_validate_frames,
                run.e2e_regressions,
                run.run_slices,
                run.run_calls,
                run.error
            ));
        } else {
            out.push(alloc::format!(
                "Linux runloop e2e: PENDING (stage={} connected_streak={} ready_streak={} frame_advances={} frame_gap={} post_frames={} sustain_slices={} unready_streak={} regressions={} slices={} calls={}).",
                LinuxRunLoopContainer::stage_label(run.stage),
                run.e2e_connected_streak,
                run.e2e_ready_streak,
                run.e2e_frame_advances,
                frame_gap,
                post_validate_frames,
                sustain_slices,
                run.e2e_post_validate_unready_streak,
                run.e2e_regressions,
                run.run_slices,
                run.run_calls
            ));
        }
        out
    }

    fn linux_runloop_start(
        &mut self,
        win_id: usize,
        target: &str,
        auto: bool,
        request_real_transfer: bool,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let trimmed = target.trim();
        if trimmed.is_empty() {
            out.push(String::from("Usage: linux runloop start <programa.elf> [args...]"));
            return out;
        }
        let mut parts = trimmed.split_whitespace();
        let target_program = parts.next().unwrap_or("").trim();
        if target_program.is_empty() {
            out.push(String::from("Usage: linux runloop start <programa.elf> [args...]"));
            return out;
        }
        let irq_active = crate::desktop_irq_timer_active();
        if !irq_active {
            out.push(String::from(
                "Linux runloop: aviso, timer IRQ no activo; ejecutando en modo compatibilidad (polling).",
            ));
            out.push(String::from(
                "Linux runloop: modo robusto IRQ desactivado; puede variar estabilidad/rendimiento.",
            ));
        }
        let mut effective_auto = auto;
        if auto && !irq_active {
            effective_auto = false;
            out.push(String::from(
                "Linux runloop: auto desactivado por seguridad (sin IRQ); usa status/step para continuar.",
            ));
        }
        let mut argv_items: Vec<String> = Vec::new();
        argv_items.push(String::from(target_program));
        for item in parts {
            let token = item.trim();
            if token.is_empty() {
                continue;
            }
            argv_items.push(String::from(token));
        }

        if !self.linux_runtime_lookup_enabled {
            self.linux_runtime_lookup_enabled = true;
            out.push(String::from(
                "Linux runtime lookup: DEEP auto-activado para runloop (dependencias Linux).",
            ));
        }

        self.linux_step_container = Some(LinuxStepContainer::new(win_id, target_program, false));
        self.linux_runloop_container = Some(LinuxRunLoopContainer::new(
            win_id,
            target_program,
            effective_auto,
            true,
            request_real_transfer,
        ));
        crate::syscall::linux_gfx_bridge_open(LINUX_BRIDGE_DEFAULT_WIDTH, LINUX_BRIDGE_DEFAULT_HEIGHT);
        crate::syscall::linux_gfx_bridge_set_direct_present(false);
        if let Some(run) = self.linux_runloop_container.as_mut() {
            run.update_progress(0, 0);
            run.argv_items = argv_items;
            run.execfn = String::from(target_program);
        }
        crate::syscall::linux_gfx_bridge_set_status(
            "Linux runloop: preparando contenedor Linux... (0%)",
        );
        self.linux_bridge_last_seq = 0;
        self.ensure_linux_bridge_window();

        out.push(alloc::format!(
            "Linux runloop: inicio '{}' (modo={}).",
            trimmed,
            if effective_auto { "auto real-slice" } else { "manual real-slice" }
        ));
        out.push(alloc::format!(
            "Linux runloop: argv detectado argc={}.",
            self.linux_runloop_container
                .as_ref()
                .map(|run| run.argv_items.len())
                .unwrap_or(0)
        ));
        out.push(String::from(
            "Linux runloop: preflight incremental activo, GUI protegida por time-slice.",
        ));
        if request_real_transfer {
            out.push(String::from(
                "Linux runloop: modo real-slice solicitado (retorno seguro al GUI).",
            ));
            if auto && LINUX_RUNLOOP_REAL_TRANSFER_AUTO_TIMEOUT_GUARD {
                out.push(alloc::format!(
                    "Linux runloop: timeout-guard activo (watchdog={} slices) para preservar prompt.",
                    LINUX_RUNLOOP_GUARDED_STALL_TIMEOUT_SLICES
                ));
            }
        }
        out.push(String::from(
            "Linux bridge: ventana SDL/X11 subset abierta para salida grafica Linux.",
        ));
        out
    }

    fn linux_runloop_stop(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        match self.linux_runloop_container.as_mut() {
            Some(run) => {
                run.active = false;
                run.stage = LinuxRunLoopStage::Stopped;
                run.update_progress(100, run.progress_overall);
                run.last_note = String::from("detenido por usuario");
                if crate::syscall::linux_shim_active() {
                    let _ = crate::syscall::linux_shim_invoke(231, 0, 0, 0, 0, 0, 0);
                }
                crate::privilege::linux_real_slice_reset();
                crate::syscall::linux_gfx_bridge_set_direct_present(false);
                crate::syscall::linux_gfx_bridge_set_status("Linux runloop detenido por usuario.");
                out.push(String::from("Linux runloop: detenido."));
            }
            None => {
                out.push(String::from("Linux runloop: no hay sesion para detener."));
            }
        }
        out
    }

    fn linux_runloop_advance(&mut self, max_steps: usize) -> Vec<String> {
        use crate::fs::FileType;

        let mut out = Vec::new();
        let mut run = match self.linux_runloop_container.take() {
            Some(v) => v,
            None => {
                out.push(String::from("Linux runloop: sin sesion activa."));
                return out;
            }
        };

        if !run.active {
            self.linux_runloop_container = Some(run);
            return out;
        }

        let budget = max_steps.max(1).min(LINUX_RUNLOOP_MAX_STEPS);
        let mut executed = 0usize;
        let start_tick = crate::timer::ticks();
        let mut yielded_by_time_budget = false;

        while executed < budget && run.active {
            match run.stage {
                LinuxRunLoopStage::Preflight => {
                    let step_lines = self.linux_step_advance(1);
                    out.extend(step_lines.into_iter());

                    let step_state = self.linux_step_container.as_ref();
                    let Some(step) = step_state else {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from("preflight container ausente.");
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    };

                    if step.active {
                        let (stage_pct, overall_pct) = match step.stage {
                            LinuxProcStage::ResolveTarget => (10, 5),
                            LinuxProcStage::ReadMainElf => (25, 10),
                            LinuxProcStage::InspectMain => (45, 16),
                            LinuxProcStage::InspectDynamic => (65, 23),
                            LinuxProcStage::CollectRuntime => {
                                let total = step.runtime_wants.len().max(1);
                                let done = step.wanted_cursor.min(total);
                                let collect_pct = ((done.saturating_mul(100)) / total) as u8;
                                let stage = 65u8.saturating_add((collect_pct as u16 * 30 / 100) as u8);
                                let overall = 23u8.saturating_add((collect_pct as u16 * 12 / 100) as u8);
                                (stage.min(95), overall.min(35))
                            }
                            LinuxProcStage::Summarize => (95, 34),
                            LinuxProcStage::Complete => (100, 35),
                            LinuxProcStage::Failed | LinuxProcStage::Stopped => {
                                (100, run.progress_overall)
                            }
                        };
                        run.update_progress(stage_pct, overall_pct);
                        if step.stage == LinuxProcStage::CollectRuntime {
                            run.last_note = alloc::format!(
                                "preflight runtime {}/{} (scan={})",
                                step.wanted_cursor.min(step.runtime_wants.len()),
                                step.runtime_wants.len(),
                                step.items_scanned
                            );
                            if (run.steps_done % 8) == 0 {
                                out.push(alloc::format!(
                                    "Linux runloop preflight: runtime {}/{} deps (scan={}).",
                                    step.wanted_cursor.min(step.runtime_wants.len()),
                                    step.runtime_wants.len(),
                                    step.items_scanned
                                ));
                            }
                        } else {
                            run.last_note = alloc::format!(
                                "preflight en progreso ({})",
                                LinuxStepContainer::stage_label(step.stage)
                            );
                        }
                    } else if step.stage == LinuxProcStage::Complete {
                        if step.issues > 0 {
                            run.active = false;
                            run.stage = LinuxRunLoopStage::Failed;
                            run.error =
                                alloc::format!("preflight incompleto (issues={}).", step.issues);
                            run.last_note = run.error.clone();
                            out.push(alloc::format!("Linux runloop error: {}", run.error));
                        } else {
                            run.main_raw = step.raw.clone();
                            run.main_name = step.target_name.clone();
                            run.target_leaf = step.target_leaf.clone();
                            run.stage = LinuxRunLoopStage::PrepareLaunch;
                            run.update_progress(100, 35);
                            run.last_note = String::from("preflight completo");
                            out.push(String::from(
                                "Linux runloop: preflight dinamico completo, preparando launch-plan.",
                            ));
                        }
                    } else {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        if step.error.is_empty() {
                            run.error = String::from("preflight fallo.");
                        } else {
                            run.error = step.error.clone();
                        }
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                    }
                }
                LinuxRunLoopStage::PrepareLaunch => {
                    run.update_progress(10, 40);
                    let step_state = self.linux_step_container.as_ref();
                    let Some(step) = step_state else {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from("preflight state perdido.");
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    };

                    let main_report = match crate::linux_compat::inspect_elf64(run.main_raw.as_slice()) {
                        Ok(v) => v,
                        Err(err) => {
                            run.active = false;
                            run.stage = LinuxRunLoopStage::Failed;
                            run.error = alloc::format!("inspect main fallo ({})", err);
                            run.last_note = run.error.clone();
                            out.push(alloc::format!("Linux runloop error: {}", run.error));
                            break;
                        }
                    };
                    if !main_report.has_dynamic || !main_report.has_interp {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from(
                            "ELF estatico detectado: runloop requiere PT_INTERP/PT_DYNAMIC para launch real.",
                        );
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        out.push(String::from(
                            "Tip: usa un ELF dinamico (PIE + PT_INTERP) y ejecuta 'linux runloop startx <elf>'.",
                        ));
                        break;
                    }

                    let dynamic = match crate::linux_compat::inspect_dynamic_elf64(run.main_raw.as_slice()) {
                        Ok(v) => v,
                        Err(err) => {
                            run.active = false;
                            run.stage = LinuxRunLoopStage::Failed;
                            run.error = alloc::format!("inspect dynamic fallo ({})", err);
                            run.last_note = run.error.clone();
                            out.push(alloc::format!("Linux runloop error: {}", run.error));
                            break;
                        }
                    };

                    let mut dependency_entries = step.current_entries.clone();
                    for entry in step.runtime_entries.iter() {
                        let name = entry.full_name();
                        let duplicate = dependency_entries.iter().any(|existing| {
                            existing.cluster == entry.cluster
                                && existing.size == entry.size
                                && existing.full_name().eq_ignore_ascii_case(name.as_str())
                        });
                        if !duplicate {
                            dependency_entries.push(*entry);
                        }
                    }

                    let mut combined_manifest_map: Vec<(String, String, String)> = Vec::new();
                    for item in step.install_manifest_map.iter() {
                        combined_manifest_map.push((item.0.clone(), item.1.clone(), item.2.clone()));
                    }
                    for item in step.runtime_manifest_map.iter() {
                        let duplicate = combined_manifest_map.iter().any(|(short, source_norm, _)| {
                            short.eq_ignore_ascii_case(item.0.as_str()) && source_norm == &item.1
                        });
                        if !duplicate {
                            combined_manifest_map.push((item.0.clone(), item.1.clone(), item.2.clone()));
                        }
                    }
                    let combined_manifest_ref = if combined_manifest_map.is_empty() {
                        None
                    } else {
                        Some(combined_manifest_map.as_slice())
                    };

                    let interp_src = step
                        .launch_interp_hint
                        .as_deref()
                        .map(String::from)
                        .or_else(|| dynamic.interp_path.clone())
                        .unwrap_or_default();
                    if interp_src.is_empty() {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from("PT_INTERP ausente.");
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }
                    if step.launch_hint_from_manifest {
                        if let Some(file_name) = step.launch_manifest_file.as_deref() {
                            out.push(alloc::format!(
                                "Linux runloop: usando metadata {} para resolver runtime.",
                                file_name
                            ));
                        } else {
                            out.push(String::from(
                                "Linux runloop: usando metadata .LNX para resolver runtime.",
                            ));
                        }
                    }
                    run.interp_source = interp_src.clone();

                    let Some(interp_local) = Self::resolve_linux_dependency_name(
                        dependency_entries.as_slice(),
                        combined_manifest_ref,
                        interp_src.as_str(),
                    ) else {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = alloc::format!("loader PT_INTERP no encontrado ({})", interp_src);
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    };
                    run.interp_local = interp_local.clone();

                    let Some(interp_entry) = Self::find_dir_file_entry_by_name(
                        dependency_entries.as_slice(),
                        interp_local.as_str(),
                    ) else {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from("entry local de PT_INTERP no encontrado.");
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    };

                    if interp_entry.file_type != FileType::File
                        || interp_entry.cluster < 2
                        || interp_entry.size == 0
                    {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from("PT_INTERP invalido.");
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }

                    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    let mut interp_raw = Vec::new();
                    interp_raw.resize(interp_entry.size as usize, 0);
                    match fat.read_file_sized(interp_entry.cluster, interp_entry.size as usize, &mut interp_raw) {
                        Ok(len) => interp_raw.truncate(len),
                        Err(err) => {
                            run.active = false;
                            run.stage = LinuxRunLoopStage::Failed;
                            run.error = alloc::format!("read PT_INTERP fallo ({})", err);
                            run.last_note = run.error.clone();
                            out.push(alloc::format!("Linux runloop error: {}", run.error));
                            break;
                        }
                    }
                    run.interp_raw = interp_raw;

                    run.dep_load_jobs.clear();
                    run.dep_load_payloads.clear();
                    run.dep_load_cursor = 0;
                    let needed_for_launch = if step.launch_hint_from_manifest
                        || !step.launch_needed_hint.is_empty()
                    {
                        step.launch_needed_hint.as_slice()
                    } else {
                        dynamic.needed.as_slice()
                    };
                    for needed in needed_for_launch.iter() {
                        if run
                            .dep_load_jobs
                            .iter()
                            .any(|job| job.soname.eq_ignore_ascii_case(needed.as_str()))
                        {
                            continue;
                        }
                        let Some(local) = Self::resolve_linux_dependency_name(
                            dependency_entries.as_slice(),
                            combined_manifest_ref,
                            needed.as_str(),
                        ) else {
                            continue;
                        };
                        let Some(dep_entry) =
                            Self::find_dir_file_entry_by_name(dependency_entries.as_slice(), local.as_str())
                        else {
                            continue;
                        };
                        if dep_entry.file_type != FileType::File
                            || dep_entry.cluster < 2
                            || dep_entry.size == 0
                        {
                            continue;
                        }
                        if dep_entry.size as usize > crate::linux_compat::ELF_MAX_FILE_BYTES {
                            run.active = false;
                            run.stage = LinuxRunLoopStage::Failed;
                            run.error = alloc::format!(
                                "dependencia demasiado grande ({})",
                                needed
                            );
                            run.last_note = run.error.clone();
                            out.push(alloc::format!("Linux runloop error: {}", run.error));
                            break;
                        }
                        run.dep_load_jobs.push(LinuxDepLoadJob {
                            soname: needed.clone(),
                            local_name: local,
                            entry: dep_entry,
                        });
                    }
                    if !run.active {
                        break;
                    }

                    if run.dep_load_jobs.is_empty() {
                        run.stage = LinuxRunLoopStage::FinalizeLaunch;
                        run.update_progress(100, 55);
                        run.last_note = String::from("sin dependencias externas, finalizando launch-plan");
                        out.push(String::from(
                            "Linux runloop: no se detectaron dependencias externas; finalizando launch-plan.",
                        ));
                    } else {
                        run.stage = LinuxRunLoopStage::LoadDependencies;
                        run.update_progress(0, 45);
                        run.last_note = alloc::format!(
                            "deps preparadas ({})",
                            run.dep_load_jobs.len()
                        );
                        out.push(alloc::format!(
                            "Linux runloop: deps preparadas ({}). Carga incremental por chunks.",
                            run.dep_load_jobs.len()
                        ));
                    }
                }
                LinuxRunLoopStage::LoadDependencies => {
                    if run.dep_load_cursor >= run.dep_load_jobs.len() {
                        run.stage = LinuxRunLoopStage::FinalizeLaunch;
                        run.update_progress(100, 55);
                        run.last_note = String::from("dependencias cargadas");
                        out.push(alloc::format!(
                            "Linux runloop: dependencias cargadas {} / {}.",
                            run.dep_load_payloads.len(),
                            run.dep_load_jobs.len()
                        ));
                        continue;
                    }

                    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    let mut processed = 0usize;
                    while run.dep_load_cursor < run.dep_load_jobs.len()
                        && processed < LINUX_RUNLOOP_DEP_FILES_PER_STEP
                    {
                        let job = run.dep_load_jobs[run.dep_load_cursor].clone();
                        let mut dep_raw = Vec::new();
                        dep_raw.resize(job.entry.size as usize, 0);
                        match fat.read_file_sized(
                            job.entry.cluster,
                            job.entry.size as usize,
                            &mut dep_raw,
                        ) {
                            Ok(len) => dep_raw.truncate(len),
                            Err(err) => {
                                run.active = false;
                                run.stage = LinuxRunLoopStage::Failed;
                                run.error = alloc::format!(
                                    "read dependencia fallo ({}: {})",
                                    job.soname, err
                                );
                                run.last_note = run.error.clone();
                                out.push(alloc::format!("Linux runloop error: {}", run.error));
                                break;
                            }
                        }
                        run.dep_load_payloads.push((job.soname.clone(), dep_raw));
                        run.dep_load_cursor = run.dep_load_cursor.saturating_add(1);
                        processed = processed.saturating_add(1);
                        self.pump_ui_while_linux_preflight(run.win_id, run.dep_load_cursor);
                    }
                    if !run.active {
                        break;
                    }

                    let dep_pct = if run.dep_load_jobs.is_empty() {
                        100
                    } else {
                        ((run.dep_load_cursor.saturating_mul(100)) / run.dep_load_jobs.len())
                            .min(100) as u8
                    };
                    let overall_pct = 45u8.saturating_add((dep_pct as u16 * 10 / 100) as u8);
                    run.update_progress(dep_pct, overall_pct);
                    run.last_note = alloc::format!(
                        "dependencias cargadas {}/{}",
                        run.dep_load_cursor,
                        run.dep_load_jobs.len()
                    );

                    if run.dep_load_cursor >= run.dep_load_jobs.len() {
                        run.stage = LinuxRunLoopStage::FinalizeLaunch;
                        run.update_progress(100, 55);
                        out.push(alloc::format!(
                            "Linux runloop: dependencias cargadas {} / {}.",
                            run.dep_load_payloads.len(),
                            run.dep_load_jobs.len()
                        ));
                    }
                }
                LinuxRunLoopStage::FinalizeLaunch => {
                    if run.dep_load_payloads.len() < run.dep_load_jobs.len() {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = alloc::format!(
                            "payloads de dependencias incompletos ({}/{}).",
                            run.dep_load_payloads.len(),
                            run.dep_load_jobs.len()
                        );
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }

                    let mut dep_launch_inputs: Vec<crate::linux_compat::LinuxDynDependencyInput<'_>> =
                        Vec::new();
                    for (soname, dep_raw) in run.dep_load_payloads.iter() {
                        dep_launch_inputs.push(crate::linux_compat::LinuxDynDependencyInput {
                            soname: soname.as_str(),
                            raw: dep_raw.as_slice(),
                        });
                    }

                    let mut launch_argv: Vec<&str> = Vec::new();
                    for item in run.argv_items.iter() {
                        let trimmed = item.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        launch_argv.push(trimmed);
                    }
                    if launch_argv.is_empty() {
                        launch_argv.push(run.main_name.as_str());
                    }
                    let execfn = if run.execfn.trim().is_empty() {
                        run.main_name.as_str()
                    } else {
                        run.execfn.as_str()
                    };

                    let plan = match crate::linux_compat::prepare_phase2_interp_launch_with_deps_and_argv(
                        run.main_raw.as_slice(),
                        run.interp_raw.as_slice(),
                        dep_launch_inputs.as_slice(),
                        launch_argv.as_slice(),
                        execfn,
                        &[],
                    ) {
                        Ok(v) => v,
                        Err(err) => {
                            run.active = false;
                            run.stage = LinuxRunLoopStage::Failed;
                            run.error = alloc::format!("prepare launch-plan fallo ({})", err);
                            run.last_note = run.error.clone();
                            out.push(alloc::format!("Linux runloop error: {}", run.error));
                            break;
                        }
                    };

                    run.runtime_paths.clear();
                    run.runtime_blob_jobs.clear();
                    let main_size = self
                        .linux_step_container
                        .as_ref()
                        .and_then(|step| step.target_entry.map(|entry| entry.size as u64))
                        .unwrap_or(run.main_raw.len() as u64);
                    Self::linux_runloop_push_runtime_path(
                        &mut run.runtime_paths,
                        run.main_name.as_str(),
                        main_size,
                    );
                    if !run.target_leaf.is_empty() && !run.target_leaf.eq_ignore_ascii_case(run.main_name.as_str()) {
                        Self::linux_runloop_push_runtime_path(
                            &mut run.runtime_paths,
                            run.target_leaf.as_str(),
                            main_size,
                        );
                    }
                    Self::linux_runloop_push_runtime_path(
                        &mut run.runtime_paths,
                        alloc::format!("/app/{}", run.main_name).as_str(),
                        main_size,
                    );

                    Self::linux_runloop_push_blob_job(
                        &mut run.runtime_blob_jobs,
                        run.main_name.as_str(),
                        main_size,
                        LinuxBlobSource::Main,
                    );

                    let interp_size = run.interp_raw.len() as u64;
                    Self::linux_runloop_push_runtime_path(
                        &mut run.runtime_paths,
                        run.interp_source.as_str(),
                        interp_size,
                    );
                    Self::linux_runloop_push_blob_job(
                        &mut run.runtime_blob_jobs,
                        run.interp_source.as_str(),
                        interp_size,
                        LinuxBlobSource::Interp,
                    );
                    // Register extra aliases for PT_INTERP to avoid path-form mismatches
                    // between absolute/relative loader opens during runtime.
                    if !run.interp_local.trim().is_empty() {
                        Self::linux_runloop_push_runtime_path(
                            &mut run.runtime_paths,
                            run.interp_local.as_str(),
                            interp_size,
                        );
                        Self::linux_runloop_push_blob_job(
                            &mut run.runtime_blob_jobs,
                            run.interp_local.as_str(),
                            interp_size,
                            LinuxBlobSource::Interp,
                        );
                    }
                    let interp_leaf = Self::linux_path_leaf(run.interp_source.as_str()).trim();
                    if !interp_leaf.is_empty() {
                        Self::linux_runloop_push_runtime_path(
                            &mut run.runtime_paths,
                            interp_leaf,
                            interp_size,
                        );
                        Self::linux_runloop_push_blob_job(
                            &mut run.runtime_blob_jobs,
                            interp_leaf,
                            interp_size,
                            LinuxBlobSource::Interp,
                        );
                        let interp_abs = alloc::format!("/{}", interp_leaf);
                        Self::linux_runloop_push_runtime_path(
                            &mut run.runtime_paths,
                            interp_abs.as_str(),
                            interp_size,
                        );
                        Self::linux_runloop_push_blob_job(
                            &mut run.runtime_blob_jobs,
                            interp_abs.as_str(),
                            interp_size,
                            LinuxBlobSource::Interp,
                        );
                    }

                    for dep in run.dep_load_jobs.iter() {
                        let dep_size = dep.entry.size as u64;
                        Self::linux_runloop_push_runtime_path(
                            &mut run.runtime_paths,
                            dep.soname.as_str(),
                            dep_size,
                        );
                        Self::linux_runloop_push_blob_job(
                            &mut run.runtime_blob_jobs,
                            dep.soname.as_str(),
                            dep_size,
                            LinuxBlobSource::Entry(dep.entry),
                        );
                    }

                    let mut total_blob_bytes = 0u64;
                    for job in run.runtime_blob_jobs.iter() {
                        total_blob_bytes = total_blob_bytes.saturating_add(job.size);
                    }
                    if total_blob_bytes > LINUX_RUNLOOP_BLOB_TOTAL_BUDGET_BYTES {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = alloc::format!(
                            "runtime blobs exceden presupuesto ({} MiB > {} MiB).",
                            (total_blob_bytes / (1024 * 1024)),
                            (LINUX_RUNLOOP_BLOB_TOTAL_BUDGET_BYTES / (1024 * 1024))
                        );
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }

                    run.runtime_path_cursor = 0;
                    run.runtime_paths_registered = 0;
                    run.runtime_blob_cursor = 0;
                    run.runtime_blobs_registered = 0;
                    out.push(alloc::format!(
                        "Linux trace: enlaces PLT/GOT resueltos={}",
                        plan.symbol_traces.len()
                    ));
                    let preview = plan
                        .symbol_traces
                        .len()
                        .min(LINUX_RUNLOOP_SYMBOL_TRACE_PREVIEW_MAX);
                    for trace in plan.symbol_traces.iter().take(preview) {
                        out.push(alloc::format!(
                            "  [{}] {} :: {} -> {} slot={:#x} value={:#x}",
                            trace.reloc_kind,
                            trace.requestor,
                            trace.symbol,
                            trace.provider,
                            trace.slot_addr,
                            trace.value_addr
                        ));
                    }
                    if plan.symbol_traces.len() > preview {
                        out.push(alloc::format!(
                            "  ... {} enlaces adicionales omitidos (preview={} para mantener GUI fluida).",
                            plan.symbol_traces.len().saturating_sub(preview),
                            preview
                        ));
                    }
                    run.plan = Some(plan);
                    run.stage = LinuxRunLoopStage::InitShim;
                    run.update_progress(100, 55);
                    run.last_note = alloc::format!(
                        "launch-plan listo (paths={} blobs={})",
                        run.runtime_paths.len(),
                        run.runtime_blob_jobs.len()
                    );
                    out.push(alloc::format!(
                        "Linux runloop: launch-plan listo (main={} interp={} paths={} blobs={}).",
                        run.main_name,
                        run.interp_local,
                        run.runtime_paths.len(),
                        run.runtime_blob_jobs.len()
                    ));
                }
                LinuxRunLoopStage::InitShim => {
                    let Some(plan) = run.plan.as_ref() else {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from("launch-plan no disponible.");
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    };
                    run.session_id = crate::syscall::linux_shim_begin(
                        plan.main_entry,
                        plan.interp_entry,
                        plan.stack_ptr,
                        plan.tls_tcb_addr,
                    );
                    run.stage = LinuxRunLoopStage::RegisterRuntimePaths;
                    run.update_progress(100, 60);
                    run.last_note = String::from("shim inicializado");
                    out.push(alloc::format!(
                        "Linux runloop: shim iniciado (session={}).",
                        run.session_id
                    ));
                }
                LinuxRunLoopStage::RegisterRuntimePaths => {
                    let mut processed = 0usize;
                    while run.runtime_path_cursor < run.runtime_paths.len()
                        && processed < LINUX_RUNLOOP_PATHS_PER_STEP
                    {
                        let (path, size) = run.runtime_paths[run.runtime_path_cursor].clone();
                        if crate::syscall::linux_shim_register_runtime_path(path.as_str(), size) {
                            run.runtime_paths_registered = run.runtime_paths_registered.saturating_add(1);
                        }
                        run.runtime_path_cursor = run.runtime_path_cursor.saturating_add(1);
                        processed = processed.saturating_add(1);
                    }

                    let path_pct = if run.runtime_paths.is_empty() {
                        100
                    } else {
                        ((run.runtime_path_cursor.saturating_mul(100)) / run.runtime_paths.len())
                            .min(100) as u8
                    };
                    let overall_pct = 60u8.saturating_add((path_pct as u16 * 15 / 100) as u8);
                    run.update_progress(path_pct, overall_pct);

                    if run.runtime_path_cursor >= run.runtime_paths.len() {
                        run.stage = LinuxRunLoopStage::RegisterRuntimeBlobs;
                        run.update_progress(100, 75);
                        run.last_note = String::from("runtime paths registrados");
                        out.push(alloc::format!(
                            "Linux runloop: runtime index {} / {} registrado.",
                            run.runtime_paths_registered,
                            run.runtime_paths.len()
                        ));
                    }
                }
                LinuxRunLoopStage::RegisterRuntimeBlobs => {
                    if run.runtime_blob_cursor >= run.runtime_blob_jobs.len() {
                        run.stage = LinuxRunLoopStage::ProbeShim;
                        run.last_note = String::from("runtime blobs registrados");
                        out.push(alloc::format!(
                            "Linux runloop: runtime blobs {} / {} cargados.",
                            run.runtime_blobs_registered,
                            run.runtime_blob_jobs.len()
                        ));
                        continue;
                    }

                    let job = run.runtime_blob_jobs[run.runtime_blob_cursor].clone();
                    if job.size as usize > LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = alloc::format!(
                            "runtime blob demasiado grande ({}: {} MiB, max {} MiB).",
                            job.path_alias,
                            (job.size / (1024 * 1024)),
                            (LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES as u64 / (1024 * 1024))
                        );
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }
                    let mut payload_temp = Vec::new();
                    let payload: &[u8] = match job.source {
                        LinuxBlobSource::Main => {
                            if run.main_raw.len() > LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES {
                                run.active = false;
                                run.stage = LinuxRunLoopStage::Failed;
                                run.error = alloc::format!(
                                    "main ELF supera limite de blob ({} MiB, max {} MiB).",
                                    (run.main_raw.len() / (1024 * 1024)),
                                    (LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES / (1024 * 1024))
                                );
                                run.last_note = run.error.clone();
                                out.push(alloc::format!("Linux runloop error: {}", run.error));
                                break;
                            }
                            run.main_raw.as_slice()
                        }
                        LinuxBlobSource::Interp => {
                            if run.interp_raw.len() > LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES {
                                run.active = false;
                                run.stage = LinuxRunLoopStage::Failed;
                                run.error = alloc::format!(
                                    "PT_INTERP supera limite de blob ({} MiB, max {} MiB).",
                                    (run.interp_raw.len() / (1024 * 1024)),
                                    (LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES / (1024 * 1024))
                                );
                                run.last_note = run.error.clone();
                                out.push(alloc::format!("Linux runloop error: {}", run.error));
                                break;
                            }
                            run.interp_raw.as_slice()
                        }
                        LinuxBlobSource::Entry(entry) => {
                            if entry.size == 0 || entry.cluster < 2 {
                                &[]
                            } else {
                                if entry.size as usize > LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES {
                                    run.active = false;
                                    run.stage = LinuxRunLoopStage::Failed;
                                    run.error = alloc::format!(
                                        "dependencia supera limite de blob ({}: {} MiB, max {} MiB).",
                                        job.path_alias,
                                        ((entry.size as u64) / (1024 * 1024)),
                                        (LINUX_RUNLOOP_BLOB_SOFT_MAX_BYTES as u64 / (1024 * 1024))
                                    );
                                    run.last_note = run.error.clone();
                                    out.push(alloc::format!("Linux runloop error: {}", run.error));
                                    break;
                                }
                                let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                                payload_temp.resize(entry.size as usize, 0);
                                match fat.read_file_sized(entry.cluster, entry.size as usize, &mut payload_temp) {
                                    Ok(len) => payload_temp.truncate(len),
                                    Err(err) => {
                                        run.active = false;
                                        run.stage = LinuxRunLoopStage::Failed;
                                        run.error = alloc::format!(
                                            "read runtime blob fallo ({}: {}).",
                                            job.path_alias,
                                            err
                                        );
                                        run.last_note = run.error.clone();
                                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                                        break;
                                    }
                                }
                                payload_temp.as_slice()
                            }
                        }
                    };
                    if !run.active {
                        break;
                    }

                    if !payload.is_empty() {
                        if crate::syscall::linux_shim_register_runtime_blob(
                            job.path_alias.as_str(),
                            payload,
                        ) {
                            run.runtime_blobs_registered = run.runtime_blobs_registered.saturating_add(1);
                        } else {
                            run.active = false;
                            run.stage = LinuxRunLoopStage::Failed;
                            run.error = alloc::format!(
                                "registro de runtime blob rechazado ({}: {} bytes).",
                                job.path_alias,
                                payload.len()
                            );
                            run.last_note = run.error.clone();
                            out.push(alloc::format!("Linux runloop error: {}", run.error));
                            break;
                        }
                    }
                    if !run.active {
                        break;
                    }

                    run.runtime_blob_cursor = run.runtime_blob_cursor.saturating_add(1);
                    let blob_pct = if run.runtime_blob_jobs.is_empty() {
                        100
                    } else {
                        ((run.runtime_blob_cursor.saturating_mul(100)) / run.runtime_blob_jobs.len())
                            .min(100) as u8
                    };
                    let overall_pct = 75u8.saturating_add((blob_pct as u16 * 15 / 100) as u8);
                    run.update_progress(blob_pct, overall_pct);
                    if run.runtime_blob_cursor >= run.runtime_blob_jobs.len() {
                        run.stage = LinuxRunLoopStage::ProbeShim;
                        run.update_progress(100, 90);
                        run.last_note = String::from("runtime blobs registrados");
                        out.push(alloc::format!(
                            "Linux runloop: runtime blobs {} / {} cargados.",
                            run.runtime_blobs_registered,
                            run.runtime_blob_jobs.len()
                        ));
                    }
                }
                LinuxRunLoopStage::ProbeShim => {
                    run.update_progress(50, 93);
                    let irq_active = crate::desktop_irq_timer_active();
                    if !irq_active {
                        run.last_note =
                            String::from("modo compatibilidad activo: polling (sin IRQ robusta)");
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: modo compatibilidad polling (sin IRQ robusta).",
                        );
                        out.push(String::from(
                            "Linux runloop: aviso, timer IRQ no activo; continuando en modo compatibilidad.",
                        ));
                    }
                    let probe = crate::syscall::linux_shim_probe_baseline();
                    let status = crate::syscall::linux_shim_status();
                    out.push(alloc::format!(
                        "Linux runloop probe: attempted={} ok={} unsupported={} failed={} watchdog={}",
                        probe.attempted,
                        probe.ok,
                        probe.unsupported,
                        probe.failed,
                        if status.watchdog_triggered { "yes" } else { "no" }
                    ));
                    out.push(alloc::format!(
                        "Linux runloop probe detail: first_errno={} openat={} fstat={} lseek={} read={} close={}",
                        probe.first_errno,
                        probe.openat_res,
                        probe.fstat_res,
                        probe.lseek_res,
                        probe.read_res,
                        probe.close_res
                    ));
                    let probe_soft_nonfatal = probe.failed <= 1;
                    if probe_soft_nonfatal && probe.failed > 0 {
                        out.push(String::from(
                            "Linux runloop: probe reporto 1 falla no critica; continuando.",
                        ));
                    }
                    if probe.failed > 1 || status.watchdog_triggered {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = alloc::format!(
                            "probe fallo (failed={}, watchdog={}).",
                            probe.failed,
                            if status.watchdog_triggered { "yes" } else { "no" }
                        );
                        run.last_note = run.error.clone();
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: probe fallo, revisar diagnostico.",
                        );
                            out.push(alloc::format!("Linux runloop error: {}", run.error));
                            break;
                        }
                    if !crate::privilege::syscall_bridge_ready() {
                        let phase_before = crate::privilege::current_phase();
                        crate::privilege::init_privilege_layers();
                        let phase_after = crate::privilege::current_phase();
                        if phase_after != phase_before {
                            out.push(alloc::format!(
                                "Linux runloop: gateway syscall init on-demand (phase {} -> {}).",
                                phase_before,
                                phase_after
                            ));
                        }
                    }
                    if !crate::privilege::syscall_bridge_ready() {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from(
                            "gateway syscall hardware no listo; avanza privilegios antes de ejecutar real-slice.",
                        );
                        run.last_note = run.error.clone();
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: gateway syscall hardware no listo.",
                        );
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }
                    if LINUX_RUNLOOP_REQUIRE_IRQ_FOR_REAL_SLICE && !irq_active {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from(
                            "timer IRQ no activo; real-slice deshabilitado para evitar congelamiento de GUI.",
                        );
                        run.last_note = run.error.clone();
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: requiere IRQ activo para real-slice seguro.",
                        );
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        out.push(String::from(
                            "Tip: inicia runtime en modo IRQ (boot irq) o usa flujo manual de preflight.",
                        ));
                        break;
                    }
                    run.real_transfer_guarded = true;
                    run.request_real_transfer = false;
                    run.stage = LinuxRunLoopStage::Running;
                    run.e2e_last_frame_seq = crate::syscall::linux_gfx_bridge_status().frame_seq;
                    run.update_progress(100, 95);
                    run.last_note = String::from("ejecucion real por time-slice activa");
                    crate::syscall::linux_gfx_bridge_set_status(
                        "Linux runloop activo: ELF real por time-slice (retorno al desktop).",
                    );
                    out.push(String::from(
                        "Linux runloop: ejecucion real activa (time-slice) con retorno seguro al desktop.",
                    ));
                    if run.auto && LINUX_RUNLOOP_REAL_TRANSFER_AUTO_TIMEOUT_GUARD {
                        out.push(alloc::format!(
                            "Linux runloop: timeout-guard activo (watchdog={} slices).",
                            LINUX_RUNLOOP_GUARDED_STALL_TIMEOUT_SLICES
                        ));
                    }
                }
                LinuxRunLoopStage::Running => {
                    let shim = crate::syscall::linux_shim_status();
                    if !shim.active {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Exited;
                        run.update_progress(100, 100);
                        run.last_note = String::from("proceso Linux finalizado");
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: proceso finalizado, retorno seguro al escritorio.",
                        );
                        out.push(alloc::format!(
                            "Linux runloop: proceso finalizado (exit_code={} watchdog={} calls={}).",
                            shim.exit_code,
                            if shim.watchdog_triggered { "yes" } else { "no" },
                            run.run_calls
                        ));
                        break;
                    }
                    if shim.interp_entry == 0 || shim.stack_ptr == 0 {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = String::from("estado exec invalido: interp_entry/stack_ptr vacios.");
                        run.last_note = run.error.clone();
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }
                    let slice = crate::syscall::linux_shim_run_real_slice(
                        shim.interp_entry,
                        shim.stack_ptr,
                        shim.fs_base,
                        LINUX_RUNLOOP_SLICE_BUDGET,
                    );
                    run.run_slices = run.run_slices.saturating_add(1);
                    run.run_calls = run.run_calls.saturating_add(slice.completed_calls as u64);
                    run.last_slice_errno = slice.first_errno;
                    if slice.completed_calls == 0 {
                        run.stalled_slices = run.stalled_slices.saturating_add(1);
                    } else {
                        run.stalled_slices = 0;
                    }
                    if run.stalled_slices >= LINUX_RUNLOOP_GUARDED_STALL_TIMEOUT_SLICES {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = alloc::format!(
                            "timeout guard: sin progreso en {} slices.",
                            run.stalled_slices
                        );
                        run.last_note = run.error.clone();
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: timeout guard activo, sesion detenida con retorno seguro.",
                        );
                        out.push(alloc::format!(
                            "Linux runloop error: {}",
                            run.error
                        ));
                        break;
                    }
                    let running_stage_pct = (((run.run_calls / 32) % 100) as u8).max(1);
                    let running_overall_pct = 95u8.saturating_add(((run.run_calls / 256) % 5) as u8);
                    run.update_progress(running_stage_pct, running_overall_pct);

                    let bridge_status = crate::syscall::linux_gfx_bridge_status();
                    let x11_status = crate::syscall::linux_x11_socket_status();
                    if x11_status.connected_count > 0 {
                        run.e2e_connected_streak = run.e2e_connected_streak.saturating_add(1);
                        run.e2e_seen_connected = true;
                    } else {
                        run.e2e_connected_streak = 0;
                    }
                    if x11_status.ready_count > 0 {
                        run.e2e_ready_streak = run.e2e_ready_streak.saturating_add(1);
                        if !run.e2e_seen_ready {
                            run.e2e_seen_ready = true;
                            run.e2e_ready_since_slice = run.run_slices;
                        }
                    } else {
                        run.e2e_ready_streak = 0;
                    }
                    let connected_and_ready =
                        x11_status.connected_count > 0 && x11_status.ready_count > 0;
                    if run.e2e_validated {
                        if connected_and_ready {
                            run.e2e_post_validate_unready_streak = 0;
                        } else {
                            run.e2e_post_validate_unready_streak =
                                run.e2e_post_validate_unready_streak.saturating_add(1);
                        }
                    }
                    if bridge_status.frame_seq > run.e2e_last_frame_seq {
                        run.e2e_frame_advances = run
                            .e2e_frame_advances
                            .saturating_add(bridge_status.frame_seq.saturating_sub(run.e2e_last_frame_seq));
                        run.e2e_last_frame_seq = bridge_status.frame_seq;
                        run.e2e_last_frame_advance_slice = run.run_slices;
                    } else if run.e2e_last_frame_seq == 0 {
                        run.e2e_last_frame_seq = bridge_status.frame_seq;
                    }
                    let frame_gap = if run.e2e_frame_advances == 0 {
                        run.run_slices
                    } else {
                        run.run_slices
                            .saturating_sub(run.e2e_last_frame_advance_slice)
                    };
                    if !run.e2e_validated {
                        let e2e_ready = run.e2e_connected_streak >= LINUX_RUNLOOP_E2E_MIN_CONNECTED_STREAK
                            && run.e2e_ready_streak >= LINUX_RUNLOOP_E2E_MIN_READY_STREAK
                            && run.e2e_frame_advances >= LINUX_RUNLOOP_E2E_MIN_FRAME_ADVANCES
                            && frame_gap <= LINUX_RUNLOOP_E2E_MAX_RECENT_FRAME_GAP_SLICES;
                        if e2e_ready {
                            run.e2e_validated = true;
                            run.e2e_validation_slice = run.run_slices;
                            run.e2e_validation_frame_advances = run.e2e_frame_advances;
                            run.e2e_post_validate_unready_streak = 0;
                            run.last_note = alloc::format!(
                                "e2e validado: connected={} ready={} frames={} frame_gap={}",
                                run.e2e_connected_streak,
                                run.e2e_ready_streak,
                                run.e2e_frame_advances,
                                frame_gap
                            );
                            crate::syscall::linux_gfx_bridge_set_status(
                                "Linux runloop: E2E validado (CONNECTED estable + render continuo).",
                            );
                            out.push(alloc::format!(
                                "Linux runloop e2e: PASS (connected_streak={} ready_streak={} frame_advances={} frame_gap={}).",
                                run.e2e_connected_streak,
                                run.e2e_ready_streak,
                                run.e2e_frame_advances,
                                frame_gap
                            ));
                        }
                    }
                    if !run.e2e_validated
                        && run.run_slices >= LINUX_RUNLOOP_E2E_CONNECT_TIMEOUT_SLICES
                        && !run.e2e_seen_ready
                    {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = alloc::format!(
                            "e2e timeout: X11 no llego a READY (connected_seen={} ready_seen={} slices={}).",
                            if run.e2e_seen_connected { "yes" } else { "no" },
                            if run.e2e_seen_ready { "yes" } else { "no" },
                            run.run_slices
                        );
                        run.last_note = run.error.clone();
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: timeout E2E (X11 no llego a READY).",
                        );
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }
                    if !run.e2e_validated
                        && run.e2e_seen_ready
                        && run.e2e_frame_advances < LINUX_RUNLOOP_E2E_MIN_FRAME_ADVANCES
                        && run.run_slices
                            .saturating_sub(run.e2e_ready_since_slice)
                            >= LINUX_RUNLOOP_E2E_FRAME_TIMEOUT_SLICES
                    {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Failed;
                        run.error = alloc::format!(
                            "e2e timeout: X11 READY sin render continuo (frame_advances={} frame_gap={} slices={}).",
                            run.e2e_frame_advances,
                            frame_gap,
                            run.run_slices
                        );
                        run.last_note = run.error.clone();
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: timeout E2E (sin avance de frame).",
                        );
                        out.push(alloc::format!("Linux runloop error: {}", run.error));
                        break;
                    }
                    if run.e2e_validated && slice.active {
                        let sustain_slices = run.run_slices.saturating_sub(run.e2e_validation_slice);
                        if sustain_slices >= LINUX_RUNLOOP_E2E_POST_VALIDATE_GRACE_SLICES {
                            if run.e2e_post_validate_unready_streak
                                >= LINUX_RUNLOOP_E2E_POST_VALIDATE_UNREADY_TIMEOUT_SLICES
                            {
                                run.active = false;
                                run.stage = LinuxRunLoopStage::Failed;
                                run.e2e_validated = false;
                                run.e2e_regressions = run.e2e_regressions.saturating_add(1);
                                run.error = alloc::format!(
                                    "e2e regresion: CONNECTED/READY inestable tras validacion (unready_streak={} sustain_slices={}).",
                                    run.e2e_post_validate_unready_streak,
                                    sustain_slices
                                );
                                run.last_note = run.error.clone();
                                crate::syscall::linux_gfx_bridge_set_status(
                                    "Linux runloop: E2E regresion (CONNECTED/READY inestable).",
                                );
                                out.push(alloc::format!("Linux runloop error: {}", run.error));
                                break;
                            }
                            if frame_gap > LINUX_RUNLOOP_E2E_POST_VALIDATE_FRAME_GAP_TIMEOUT_SLICES {
                                let post_validate_frames = run
                                    .e2e_frame_advances
                                    .saturating_sub(run.e2e_validation_frame_advances);
                                run.active = false;
                                run.stage = LinuxRunLoopStage::Failed;
                                run.e2e_validated = false;
                                run.e2e_regressions = run.e2e_regressions.saturating_add(1);
                                run.error = alloc::format!(
                                    "e2e regresion: render no continuo tras validacion (post_frames={} frame_gap={} sustain_slices={}).",
                                    post_validate_frames,
                                    frame_gap,
                                    sustain_slices
                                );
                                run.last_note = run.error.clone();
                                crate::syscall::linux_gfx_bridge_set_status(
                                    "Linux runloop: E2E regresion (frame_seq detenido).",
                                );
                                out.push(alloc::format!("Linux runloop error: {}", run.error));
                                break;
                            }
                        }
                    }
                    if bridge_status.event_count > 0 {
                        run.last_note = alloc::format!(
                            "ejecucion real activa (input queue={})",
                            bridge_status.event_count
                        );
                    }

                    let should_render_bridge = run.bridge_enabled
                        && (run.run_slices <= 2
                            || bridge_status.event_count > 0
                            || (run.run_slices % LINUX_RUNLOOP_BRIDGE_RENDER_EVERY_SLICES) == 0);
                    if should_render_bridge {
                        crate::syscall::linux_gfx_bridge_render_runtime(run.run_slices);
                    }

                    if !slice.active {
                        run.active = false;
                        run.stage = LinuxRunLoopStage::Exited;
                        run.update_progress(100, 100);
                        run.last_note = String::from("proceso Linux finalizado");
                        crate::syscall::linux_gfx_bridge_set_status(
                            "Linux runloop: proceso finalizado, retorno seguro al escritorio.",
                        );
                        out.push(alloc::format!(
                            "Linux runloop: proceso finalizado (exit_code={} watchdog={} calls={}).",
                            slice.exit_code,
                            if slice.watchdog_triggered { "yes" } else { "no" },
                            run.run_calls
                        ));
                        break;
                    }

                    if (run.run_slices % LINUX_RUNLOOP_PROGRESS_EVERY_SLICES) == 0 {
                        let status = crate::syscall::linux_shim_status();
                        let last_sys_name = crate::syscall::linux_syscall_name(status.last_sysno);
                        let last_errno_name = crate::syscall::linux_errno_name(status.last_errno);
                        out.push(alloc::format!(
                            "Linux runloop: progress={}%% stage={}%% slices={} calls={} pid={} procs={} last_sys={}({}) last_errno={}({}) mmap={} open_fds={} threads={}/{} sigmask={:#x} pending={:#x}",
                            run.progress_overall,
                            run.progress_stage,
                            run.run_slices,
                            run.run_calls,
                            status.current_pid,
                            status.process_count,
                            last_sys_name,
                            status.last_sysno,
                            status.last_errno,
                            last_errno_name,
                            status.mmap_count,
                            status.open_file_count,
                            status.runnable_threads,
                            status.thread_count,
                            status.signal_mask,
                            status.pending_signals
                        ));
                        if let Some(diag) = Self::linux_shim_path_diag_line(&status) {
                            out.push(diag);
                        }
                    }
                }
                LinuxRunLoopStage::Exited
                | LinuxRunLoopStage::Failed
                | LinuxRunLoopStage::Stopped => {
                    run.active = false;
                }
            }

            let bucket = LinuxRunLoopContainer::progress_bucket(run.progress_overall);
            if bucket > run.progress_bucket_reported {
                run.progress_bucket_reported = bucket;
                out.push(alloc::format!(
                    "Linux runloop progress: {}%% (stage={} stage_pct={}%%).",
                    run.progress_overall,
                    LinuxRunLoopContainer::stage_label(run.stage),
                    run.progress_stage
                ));
                let bridge_status = alloc::format!(
                    "Linux runloop: {}% [{} {}%]",
                    run.progress_overall,
                    LinuxRunLoopContainer::stage_label(run.stage),
                    run.progress_stage
                );
                crate::syscall::linux_gfx_bridge_set_status(bridge_status.as_str());
            }

            run.steps_done = run.steps_done.saturating_add(1);
            run.last_step_tick = crate::timer::ticks();
            executed += 1;
            if run.active
                && crate::timer::ticks().saturating_sub(start_tick) >= LINUX_RUNLOOP_CMD_TICK_BUDGET
            {
                yielded_by_time_budget = true;
                break;
            }
        }

        if yielded_by_time_budget && !run.auto {
            out.push(String::from(
                "Linux runloop: pausa por presupuesto de tiempo; usa status/step para continuar.",
            ));
        }
        if executed == 0 && run.active {
            out.push(String::from("Linux runloop: sin avance (reintentar)."));
        }
        self.linux_runloop_container = Some(run);
        out
    }

    fn service_linux_runloop_container(&mut self) {
        if self.linux_runloop_busy {
            return;
        }
        let (active, auto, win_id, stage) = match self.linux_runloop_container.as_ref() {
            Some(run) => (run.active, run.auto, run.win_id, run.stage),
            None => return,
        };
        if !active || !auto {
            return;
        }

        let step_budget = match stage {
            LinuxRunLoopStage::Preflight => 4,
            LinuxRunLoopStage::PrepareLaunch => 2,
            LinuxRunLoopStage::LoadDependencies => 2,
            LinuxRunLoopStage::FinalizeLaunch => 2,
            LinuxRunLoopStage::RegisterRuntimePaths => 4,
            LinuxRunLoopStage::RegisterRuntimeBlobs => 4,
            LinuxRunLoopStage::ProbeShim => 2,
            LinuxRunLoopStage::InitShim
            | LinuxRunLoopStage::Running
            | LinuxRunLoopStage::Exited
            | LinuxRunLoopStage::Failed
            | LinuxRunLoopStage::Stopped => 1,
        };

        self.linux_runloop_busy = true;
        let lines = self.linux_runloop_advance(step_budget);
        self.linux_runloop_busy = false;
        self.append_terminal_lines(win_id, lines.as_slice());
    }

    pub fn new(width: usize, height: usize) -> Self {
        let taskbar_h = 40;
        let taskbar_y = (height - taskbar_h as usize) as i32;
        let taskbar_window = Window::new(9999, "Taskbar", 0, taskbar_y, width as u32, taskbar_h);

        Self {
            windows: Vec::new(),
            closed_windows: Vec::new(),
            active_window_id: None,
            next_id: 1,
            width,
            height,
            mouse_pos: Point {
                x: width as i32 / 2,
                y: height as i32 / 2,
            },
            taskbar: Taskbar::new(width as u32, height as u32),
            minimized_windows: Vec::new(),
            last_mouse_down: false,
            last_mouse_right_down: false,
            taskbar_window,
            start_tools_open: false,
            start_games_open: false,
            start_apps_open: false,
            start_app_shortcuts: Vec::new(),
            last_explorer_click: None,
            explorer_context_menu: None,
            desktop_context_menu: None,
            explorer_clipboard: None,
            pointer_capture: None,
            current_volume_device_index: None,
            desktop_usb_device_index: None,
            desktop_usb_device_label: String::new(),
            desktop_usb_menu_open: false,
            desktop_usb_last_click_tick: 0,
            desktop_usb_last_probe_tick: 0,
            desktop_usb_ejected_device_index: None,
            desktop_surface_status: String::new(),
            explorer_selected_items: Vec::new(),
            desktop_selected_items: Vec::new(),
            desktop_icon_positions: Vec::new(),
            desktop_drag: None,
            desktop_create_folder: None,
            rename_prompt: None,
            copy_progress_prompt: None,
            clipboard_paste_job: None,
            clipboard_paste_job_busy: false,
            notepad_save_prompt: None,
            manual_unmount_lock: false,
            linux_real_transfer_enabled: false,
            linux_runtime_lookup_enabled: true,
            linux_step_container: None,
            linux_step_busy: false,
            linux_runloop_container: None,
            linux_runloop_busy: false,
            linux_bridge_window_id: None,
            linux_bridge_last_seq: 0,
            // Default to local builtin renderer; host bridge stays optional via command.
            web_backend_mode: WebBackendMode::Builtin,
            web_proxy_endpoint_base: String::from(WEB_PROXY_DEFAULT_BASE),
        }
    }

    pub fn create_window(&mut self, title: &str, x: i32, y: i32, width: u32, height: u32) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        id
    }

    pub fn create_explorer_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new_explorer(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        self.refresh_explorer_home(id);
        id
    }

    pub fn create_notepad_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new_notepad(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        id
    }

    pub fn create_browser_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new_browser(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        id
    }

    pub fn create_image_viewer_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new_image_viewer(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        id
    }

    pub fn create_settings_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new_settings(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        id
    }

    pub fn create_doom_launcher_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new_doom_launcher(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        id
    }

    pub fn create_linux_bridge_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new_linux_bridge(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        self.linux_bridge_window_id = Some(id);
        id
    }

    pub fn create_app_runner_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new_app_runner(id, title, x, y, width, height);
        self.windows.push(win);
        self.active_window_id = Some(id);
        id
    }

    fn detect_primary_usb_device() -> Option<(usize, String)> {
        let devices = crate::fat32::Fat32::detect_uefi_block_devices();
        let boot_device_index = crate::fat32::Fat32::boot_block_device_index();
        let mut fallback_non_boot: Option<(usize, String)> = None;
        let mut fallback_boot: Option<(usize, String)> = None;

        for dev in devices.iter() {
            if !dev.removable {
                continue;
            }

            let label = alloc::format!("USB {} ({} MiB)", dev.index, dev.total_mib);
            let is_boot = Some(dev.index) == boot_device_index;
            if dev.logical_partition && !is_boot {
                return Some((dev.index, label));
            }

            if is_boot {
                if fallback_boot.is_none() {
                    fallback_boot = Some((dev.index, label));
                }
            } else if fallback_non_boot.is_none() {
                fallback_non_boot = Some((dev.index, label));
            }
        }

        fallback_non_boot.or(fallback_boot)
    }

    fn refresh_desktop_usb_state_if_needed(&mut self, force: bool) {
        let now = crate::timer::ticks();
        if !force
            && now.saturating_sub(self.desktop_usb_last_probe_tick) < DESKTOP_USB_PROBE_INTERVAL_TICKS
        {
            return;
        }
        self.desktop_usb_last_probe_tick = now;

        if let Some((index, label)) = Self::detect_primary_usb_device() {
            if let Some(ejected_index) = self.desktop_usb_ejected_device_index {
                if ejected_index == index {
                    self.desktop_usb_device_index = None;
                    self.desktop_usb_device_label.clear();
                    self.desktop_usb_menu_open = false;
                    return;
                }
                // Different device connected: clear previous ejected state.
                self.desktop_usb_ejected_device_index = None;
                self.manual_unmount_lock = false;
            }
            self.desktop_usb_device_index = Some(index);
            self.desktop_usb_device_label = label;
        } else {
            self.desktop_usb_device_index = None;
            self.desktop_usb_device_label.clear();
            self.desktop_usb_menu_open = false;
            self.desktop_usb_ejected_device_index = None;
            self.manual_unmount_lock = false;
        }
    }

    fn clear_manual_unmount_lock(&mut self) {
        self.manual_unmount_lock = false;
        self.desktop_usb_ejected_device_index = None;
    }

    fn desktop_usb_icon_rect(&self) -> Rect {
        Rect::new(18, 18, DESKTOP_USB_ICON_W, DESKTOP_USB_ICON_H)
    }

    fn desktop_usb_menu_rect(&self) -> Rect {
        let icon = self.desktop_usb_icon_rect();
        let menu_h = (DESKTOP_USB_MENU_ITEMS as u32) * DESKTOP_USB_MENU_ITEM_H + 8;

        let mut x = icon.x + icon.width as i32 + 6;
        if x + DESKTOP_USB_MENU_W as i32 > self.width as i32 {
            x = (icon.x - DESKTOP_USB_MENU_W as i32 - 6).max(0);
        }

        let mut y = icon.y + 4;
        if y + menu_h as i32 > self.taskbar.rect.y {
            y = (self.taskbar.rect.y - menu_h as i32 - 4).max(0);
        }

        Rect::new(x, y, DESKTOP_USB_MENU_W, menu_h)
    }

    fn desktop_usb_menu_item_rect(&self, index: usize) -> Rect {
        let menu = self.desktop_usb_menu_rect();
        Rect::new(
            menu.x + 4,
            menu.y + 4 + (index as i32 * DESKTOP_USB_MENU_ITEM_H as i32),
            menu.width.saturating_sub(8),
            DESKTOP_USB_MENU_ITEM_H,
        )
    }

    fn draw_desktop_usb_overlay(&mut self) {
        let Some(_index) = self.desktop_usb_device_index else {
            return;
        };

        let icon = self.desktop_usb_icon_rect();
        framebuffer::rect(
            icon.x.max(0) as usize,
            icon.y.max(0) as usize,
            icon.width as usize,
            icon.height as usize,
            0x17304A,
        );
        framebuffer::rect(
            icon.x.max(0) as usize,
            icon.y.max(0) as usize,
            icon.width as usize,
            1,
            0x7EA7CE,
        );
        framebuffer::rect(
            icon.x.max(0) as usize,
            (icon.y + icon.height as i32 - 1).max(0) as usize,
            icon.width as usize,
            1,
            0x0A1622,
        );

        let ux = icon.x + 34;
        let uy = icon.y + 18;
        framebuffer::rect(ux.max(0) as usize, uy.max(0) as usize, 44, 24, 0xA7B6C5);
        framebuffer::rect((ux + 6).max(0) as usize, (uy + 6).max(0) as usize, 32, 10, 0xD8E2EC);
        framebuffer::rect((ux + 36).max(0) as usize, (uy + 18).max(0) as usize, 4, 4, 0x4CE07D);

        framebuffer::draw_text_5x7(
            (icon.x + 10).max(0) as usize,
            (icon.y + 52).max(0) as usize,
            "USB STORAGE",
            0xEAF5FF,
        );

        let label = Self::trim_ascii_line(self.desktop_usb_device_label.as_str(), 18);
        framebuffer::draw_text_5x7(
            (icon.x + 8).max(0) as usize,
            (icon.y + 66).max(0) as usize,
            label.as_str(),
            0xC6DDF3,
        );
        framebuffer::draw_text_5x7(
            (icon.x + 8).max(0) as usize,
            (icon.y + 79).max(0) as usize,
            "L: abrir  R: menu",
            0x9CB9D6,
        );

        if self.desktop_usb_menu_open {
            let menu = self.desktop_usb_menu_rect();
            framebuffer::rect(
                menu.x.max(0) as usize,
                menu.y.max(0) as usize,
                menu.width as usize,
                menu.height as usize,
                0x1E2C3A,
            );
            framebuffer::rect(
                menu.x.max(0) as usize,
                menu.y.max(0) as usize,
                menu.width as usize,
                1,
                0x5F7B96,
            );

            let open_item = self.desktop_usb_menu_item_rect(0);
            framebuffer::rect(
                open_item.x.max(0) as usize,
                open_item.y.max(0) as usize,
                open_item.width as usize,
                open_item.height as usize,
                0x2F475E,
            );
            framebuffer::draw_text_5x7(
                (open_item.x + 8).max(0) as usize,
                (open_item.y + 8).max(0) as usize,
                "Abrir Explorador",
                0xEAF6FF,
            );

            let unmount_item = self.desktop_usb_menu_item_rect(1);
            framebuffer::rect(
                unmount_item.x.max(0) as usize,
                unmount_item.y.max(0) as usize,
                unmount_item.width as usize,
                unmount_item.height as usize,
                0x50313A,
            );
            framebuffer::draw_text_5x7(
                (unmount_item.x + 8).max(0) as usize,
                (unmount_item.y + 8).max(0) as usize,
                "Desmontar",
                0xFFDDE4,
            );
        }
    }

    fn open_desktop_usb_in_explorer(&mut self) {
        let Some(index) = self.desktop_usb_device_index else {
            return;
        };
        let explorer_id = self.create_explorer_window("File Explorer - USB", 140, 80, 920, 580);
        self.open_explorer_volume(explorer_id, index);
    }

    fn unmount_active_volume(&mut self) -> String {
        let ejected_index = self.desktop_usb_device_index;
        let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
        if fat.bytes_per_sector == 0 {
            return String::from("Unmount: no hay volumen montado.");
        }

        fat.unmount();

        let mut explorer_ids = Vec::new();
        for win in self.windows.iter_mut() {
            if win.is_terminal() {
                win.current_dir_cluster = 0;
                win.current_path = String::from("REDUX/");
                win.add_output("Volume unmounted.");
                win.render_terminal();
            } else if win.is_explorer() {
                explorer_ids.push(win.id);
            } else if win.is_notepad() {
                win.notepad_dir_cluster = 0;
                win.notepad_dir_path = String::from("/");
                win.set_notepad_status("Volume unmounted.");
            }
        }

        for id in explorer_ids {
            self.refresh_explorer_home(id);
        }

        self.desktop_usb_menu_open = false;
        self.desktop_usb_device_index = None;
        self.desktop_usb_device_label.clear();
        self.current_volume_device_index = None;
        self.explorer_context_menu = None;
        self.desktop_context_menu = None;
        self.explorer_clipboard = None;
        self.desktop_surface_status = String::from("Unmount: volumen desmontado.");
        self.explorer_selected_items.clear();
        self.desktop_selected_items.clear();
        self.desktop_drag = None;
        self.desktop_create_folder = None;
        self.rename_prompt = None;
        self.notepad_save_prompt = None;
        self.manual_unmount_lock = true;
        self.desktop_usb_ejected_device_index = ejected_index;
        String::from("Unmount: volumen desmontado.")
    }

    fn handle_desktop_usb_left_click(&mut self) -> bool {
        self.refresh_desktop_usb_state_if_needed(true);

        if self.desktop_usb_menu_open {
            let menu = self.desktop_usb_menu_rect();
            if menu.contains(self.mouse_pos) {
                let open_item = self.desktop_usb_menu_item_rect(0);
                if open_item.contains(self.mouse_pos) {
                    self.desktop_usb_menu_open = false;
                    self.open_desktop_usb_in_explorer();
                    return true;
                }

                let unmount_item = self.desktop_usb_menu_item_rect(1);
                if unmount_item.contains(self.mouse_pos) {
                    self.desktop_usb_menu_open = false;
                    let _ = self.unmount_active_volume();
                    return true;
                }

                return true;
            }
        }

        let Some(_index) = self.desktop_usb_device_index else {
            self.desktop_usb_menu_open = false;
            return false;
        };

        let icon = self.desktop_usb_icon_rect();
        if icon.contains(self.mouse_pos) {
            self.desktop_usb_menu_open = false;
            let now = crate::timer::ticks();
            let is_double_click = Self::is_double_click_delta(
                now.saturating_sub(self.desktop_usb_last_click_tick),
            );
            self.desktop_usb_last_click_tick = now;
            if is_double_click {
                self.desktop_usb_last_click_tick = 0;
                self.open_desktop_usb_in_explorer();
            } else {
                self.desktop_surface_status =
                    String::from("Unidad USB seleccionada. Doble clic para abrir.");
            }
            return true;
        }

        if self.desktop_usb_menu_open {
            self.desktop_usb_menu_open = false;
        }
        false
    }

    fn handle_desktop_usb_right_click(&mut self) -> bool {
        self.refresh_desktop_usb_state_if_needed(true);

        let Some(_index) = self.desktop_usb_device_index else {
            self.desktop_usb_menu_open = false;
            return false;
        };

        let icon = self.desktop_usb_icon_rect();
        if icon.contains(self.mouse_pos) {
            self.desktop_usb_menu_open = true;
            return true;
        }

        if self.desktop_usb_menu_open {
            let menu = self.desktop_usb_menu_rect();
            if menu.contains(self.mouse_pos) {
                return true;
            }
            self.desktop_usb_menu_open = false;
            return false;
        }

        false
    }

    fn resolve_named_root_dir_on_best_volume(
        &mut self,
        shortcut_name: &str,
        create_if_missing: bool,
    ) -> Result<(u32, String), String> {
        let candidates = self.auto_mount_candidate_indices();
        if candidates.is_empty() {
            return Err(String::from("No hay volumen FAT32 disponible."));
        }

        for index in candidates.iter().copied() {
            if !self.ensure_volume_index_mounted(index) {
                continue;
            }
            let outcome = {
                let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                let volume_label = Self::volume_label_text(fat).unwrap_or(alloc::format!("VOL{}", index));
                match Self::resolve_named_root_dir_cluster(fat, shortcut_name, false) {
                    Ok((cluster, _created)) => {
                        Ok((cluster, alloc::format!("{}/{}/", volume_label, shortcut_name)))
                    }
                    Err(err) => Err(err),
                }
            };
            if let Ok(found) = outcome {
                return Ok(found);
            }
        }

        if create_if_missing {
            for index in candidates.iter().copied() {
                if !self.ensure_volume_index_mounted(index) {
                    continue;
                }
                let outcome = {
                    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    let volume_label = Self::volume_label_text(fat).unwrap_or(alloc::format!("VOL{}", index));
                    match Self::resolve_named_root_dir_cluster(fat, shortcut_name, true) {
                        Ok((cluster, _created)) => {
                            Ok((cluster, alloc::format!("{}/{}/", volume_label, shortcut_name)))
                        }
                        Err(err) => Err(err),
                    }
                };
                if let Ok(found) = outcome {
                    return Ok(found);
                }
            }
        }

        Err(alloc::format!(
            "No se encontro la carpeta '{}' en los volumenes disponibles.",
            shortcut_name
        ))
    }

    fn resolve_desktop_directory_target(&mut self, create_if_missing: bool) -> Result<(u32, String), String> {
        self.resolve_named_root_dir_on_best_volume("Desktop", create_if_missing)
    }

    fn desktop_surface_items(&mut self) -> Option<(u32, String, Vec<ExplorerItem>)> {
        let (desktop_cluster, desktop_path) = self.resolve_desktop_directory_target(false).ok()?;
        let mut items = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            Self::build_explorer_dir_items(fat, desktop_cluster)
        };
        items.retain(|item| {
            item.kind == ExplorerItemKind::Directory || item.kind == ExplorerItemKind::File
        });
        if items.len() > DESKTOP_ITEMS_MAX {
            items.truncate(DESKTOP_ITEMS_MAX);
        }

        items.push(ExplorerItem::new("Papelera", ExplorerItemKind::ShortcutRecycleBin, 0, 0));

        Some((desktop_cluster, desktop_path, items))
    }

    fn desktop_item_key_eq(cluster: u32, label: &str, item: &ExplorerItem) -> bool {
        cluster == item.cluster && label.eq_ignore_ascii_case(item.label.as_str())
    }

    fn desktop_item_selected(&self, source_dir_cluster: u32, item: &ExplorerItem) -> bool {
        self.desktop_selected_items.iter().any(|entry| {
            entry.source_dir_cluster == source_dir_cluster
                && Self::desktop_item_key_eq(entry.cluster, entry.label.as_str(), item)
        })
    }

    fn desktop_select_single(&mut self, source_dir_cluster: u32, item: &ExplorerItem) {
        self.desktop_selected_items.clear();
        self.desktop_selected_items.push(DesktopSelectionItem {
            source_dir_cluster,
            cluster: item.cluster,
            label: item.label.clone(),
            kind: item.kind,
            size: item.size,
        });
    }

    fn desktop_add_selection(&mut self, source_dir_cluster: u32, item: &ExplorerItem) {
        if self.desktop_item_selected(source_dir_cluster, item) {
            return;
        }
        self.desktop_selected_items.push(DesktopSelectionItem {
            source_dir_cluster,
            cluster: item.cluster,
            label: item.label.clone(),
            kind: item.kind,
            size: item.size,
        });
    }

    fn desktop_prune_selection(&mut self, source_dir_cluster: u32, items: &[ExplorerItem]) {
        self.desktop_selected_items.retain(|entry| {
            if entry.source_dir_cluster != source_dir_cluster {
                return false;
            }
            items
                .iter()
                .any(|item| Self::desktop_item_key_eq(entry.cluster, entry.label.as_str(), item))
        });
    }

    fn desktop_collect_selected_items(
        &mut self,
        source_dir_cluster: u32,
        items: &[ExplorerItem],
    ) -> Vec<ExplorerItem> {
        self.desktop_prune_selection(source_dir_cluster, items);
        if self.desktop_selected_items.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::new();
        for item in items.iter() {
            if self
                .desktop_selected_items
                .iter()
                .any(|sel| {
                    sel.source_dir_cluster == source_dir_cluster
                        && Self::desktop_item_key_eq(sel.cluster, sel.label.as_str(), item)
                })
            {
                out.push(item.clone());
            }
        }
        out
    }

    fn explorer_item_key_eq(cluster: u32, label: &str, item: &ExplorerItem) -> bool {
        cluster == item.cluster && label.eq_ignore_ascii_case(item.label.as_str())
    }

    fn explorer_item_selected(
        &self,
        win_id: usize,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) -> bool {
        self.explorer_selected_items.iter().any(|entry| {
            entry.win_id == win_id
                && entry.source_dir_cluster == source_dir_cluster
                && Self::explorer_item_key_eq(entry.cluster, entry.label.as_str(), item)
        })
    }

    fn explorer_clear_selection_for_window(&mut self, win_id: usize) {
        self.explorer_selected_items
            .retain(|entry| entry.win_id != win_id);
    }

    fn explorer_clear_selection_scope(&mut self, win_id: usize, source_dir_cluster: u32) {
        self.explorer_selected_items.retain(|entry| {
            !(entry.win_id == win_id && entry.source_dir_cluster == source_dir_cluster)
        });
    }

    fn explorer_select_single(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) {
        self.explorer_clear_selection_for_window(win_id);
        self.explorer_selected_items.push(ExplorerSelectionItem {
            win_id,
            source_dir_cluster,
            cluster: item.cluster,
            label: item.label.clone(),
        });
    }

    fn explorer_add_selection(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) {
        if self.explorer_item_selected(win_id, source_dir_cluster, item) {
            return;
        }
        self.explorer_selected_items.retain(|entry| {
            !(entry.win_id == win_id && entry.source_dir_cluster != source_dir_cluster)
        });
        self.explorer_selected_items.push(ExplorerSelectionItem {
            win_id,
            source_dir_cluster,
            cluster: item.cluster,
            label: item.label.clone(),
        });
    }

    fn explorer_prune_selection(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        items: &[ExplorerItem],
    ) {
        self.explorer_selected_items.retain(|entry| {
            if entry.win_id != win_id || entry.source_dir_cluster != source_dir_cluster {
                return true;
            }
            items
                .iter()
                .any(|item| Self::explorer_item_key_eq(entry.cluster, entry.label.as_str(), item))
        });
    }

    fn explorer_collect_selected_items(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        items: &[ExplorerItem],
    ) -> Vec<ExplorerItem> {
        self.explorer_prune_selection(win_id, source_dir_cluster, items);
        if self.explorer_selected_items.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::new();
        for item in items.iter() {
            if self
                .explorer_selected_items
                .iter()
                .any(|sel| {
                    sel.win_id == win_id
                        && sel.source_dir_cluster == source_dir_cluster
                        && Self::explorer_item_key_eq(sel.cluster, sel.label.as_str(), item)
                })
            {
                out.push(item.clone());
            }
        }
        out
    }

    fn desktop_default_item_rect(&self, index: usize) -> Option<Rect> {
        let top = DESKTOP_ITEMS_START_Y;
        let bottom = self.taskbar.rect.y - 8;
        if bottom <= top + DESKTOP_ITEM_H as i32 {
            return None;
        }

        let stride_y = DESKTOP_ITEM_H as i32 + DESKTOP_ITEM_GAP_Y;
        let rows = ((bottom - top + DESKTOP_ITEM_GAP_Y) / stride_y).max(1) as usize;
        let col = index / rows;
        let row = index % rows;

        let x = DESKTOP_ITEMS_START_X + col as i32 * (DESKTOP_ITEM_W as i32 + DESKTOP_ITEM_GAP_X);
        let y = top + row as i32 * stride_y;
        if x + DESKTOP_ITEM_W as i32 > self.width as i32 - 8 || y + DESKTOP_ITEM_H as i32 > bottom {
            return None;
        }

        Some(Rect::new(x, y, DESKTOP_ITEM_W, DESKTOP_ITEM_H))
    }

    fn desktop_item_custom_position(&self, item: &ExplorerItem) -> Option<(i32, i32)> {
        self.desktop_icon_positions
            .iter()
            .find(|entry| Self::desktop_item_key_eq(entry.cluster, entry.label.as_str(), item))
            .map(|entry| (entry.x, entry.y))
    }

    fn set_desktop_item_custom_position(&mut self, item: &ExplorerItem, x: i32, y: i32) {
        self.set_desktop_item_custom_position_by_key(item.cluster, item.label.as_str(), x, y);
    }

    fn desktop_surface_item_rect(&self, index: usize, item: &ExplorerItem) -> Option<Rect> {
        let base = self.desktop_default_item_rect(index)?;
        if let Some((x, y)) = self.desktop_item_custom_position(item) {
            return Some(Rect::new(x, y, base.width, base.height));
        }
        Some(base)
    }

    fn desktop_surface_item_at(
        &mut self,
        mouse_x: i32,
        mouse_y: i32,
    ) -> Option<(u32, String, Vec<ExplorerItem>, ExplorerItem, Rect)> {
        let (desktop_cluster, desktop_path, items) = self.desktop_surface_items()?;
        for (idx, item) in items.iter().enumerate() {
            let Some(slot) = self.desktop_surface_item_rect(idx, item) else {
                break;
            };
            if slot.contains(Point { x: mouse_x, y: mouse_y }) {
                return Some((
                    desktop_cluster,
                    desktop_path.clone(),
                    items.clone(),
                    item.clone(),
                    slot,
                ));
            }
        }
        None
    }

    fn draw_desktop_surface_item_icon(rect: Rect, kind: ExplorerItemKind) {
        if kind == ExplorerItemKind::Directory {
            framebuffer::rect(
                (rect.x + 10).max(0) as usize,
                (rect.y + 18).max(0) as usize,
                rect.width.saturating_sub(20) as usize,
                rect.height.saturating_sub(24) as usize,
                0xE8C56B,
            );
            framebuffer::rect(
                (rect.x + 16).max(0) as usize,
                (rect.y + 10).max(0) as usize,
                22,
                10,
                0xF2D78A,
            );
            framebuffer::rect(
                (rect.x + 10).max(0) as usize,
                (rect.y + 18).max(0) as usize,
                rect.width.saturating_sub(20) as usize,
                1,
                0xB58735,
            );
            framebuffer::rect(
                (rect.x + 10).max(0) as usize,
                (rect.y + rect.height as i32 - 7).max(0) as usize,
                rect.width.saturating_sub(20) as usize,
                1,
                0x8E6525,
            );
            return;
        }

        if kind == ExplorerItemKind::ShortcutRecycleBin {
            framebuffer::rect(
                (rect.x + 14).max(0) as usize,
                (rect.y + 16).max(0) as usize,
                rect.width.saturating_sub(28) as usize,
                rect.height.saturating_sub(20) as usize,
                0x9EACBA,
            );
            framebuffer::rect(
                (rect.x + 12).max(0) as usize,
                (rect.y + 12).max(0) as usize,
                rect.width.saturating_sub(24) as usize,
                4,
                0x76899E,
            );
            framebuffer::rect(
                (rect.x + rect.width as i32 / 2 - 4).max(0) as usize,
                (rect.y + 8).max(0) as usize,
                8,
                4,
                0x76899E,
            );
            return;
        }

        framebuffer::rect(
            (rect.x + 14).max(0) as usize,
            (rect.y + 8).max(0) as usize,
            rect.width.saturating_sub(28) as usize,
            rect.height.saturating_sub(12) as usize,
            0xFFFFFF,
        );
        framebuffer::rect(
            (rect.x + 14).max(0) as usize,
            (rect.y + 8).max(0) as usize,
            rect.width.saturating_sub(28) as usize,
            1,
            0xA1B4C8,
        );
        framebuffer::rect(
            (rect.x + 14).max(0) as usize,
            (rect.y + rect.height as i32 - 5).max(0) as usize,
            rect.width.saturating_sub(28) as usize,
            1,
            0x8A9DAF,
        );
        framebuffer::rect(
            (rect.x + rect.width as i32 - 24).max(0) as usize,
            (rect.y + 8).max(0) as usize,
            10,
            10,
            0xDFE8F0,
        );
    }

    fn draw_desktop_surface_overlay(&mut self) {
        let Some((desktop_cluster, _desktop_path, items)) = self.desktop_surface_items() else {
            self.desktop_selected_items.clear();
            self.desktop_drag = None;
            return;
        };
        self.desktop_prune_selection(desktop_cluster, items.as_slice());

        for (idx, item) in items.iter().enumerate() {
            let Some(slot) = self.desktop_surface_item_rect(idx, item) else {
                break;
            };

            framebuffer::rect(
                slot.x.max(0) as usize,
                slot.y.max(0) as usize,
                slot.width as usize,
                slot.height as usize,
                0x0E2B48,
            );
            framebuffer::rect(
                slot.x.max(0) as usize,
                slot.y.max(0) as usize,
                slot.width as usize,
                1,
                0x3E6488,
            );
            framebuffer::rect(
                slot.x.max(0) as usize,
                (slot.y + slot.height as i32 - 1).max(0) as usize,
                slot.width as usize,
                1,
                0x0A1B2A,
            );

            if self.desktop_item_selected(desktop_cluster, item) {
                if slot.width > 6 && slot.height > 6 {
                    framebuffer::rect(
                        (slot.x + 2).max(0) as usize,
                        (slot.y + 2).max(0) as usize,
                        (slot.width - 4) as usize,
                        (slot.height - 4) as usize,
                        0x17466C,
                    );
                }
                framebuffer::rect(
                    slot.x.max(0) as usize,
                    slot.y.max(0) as usize,
                    slot.width as usize,
                    2,
                    0x79B7EC,
                );
                framebuffer::rect(
                    slot.x.max(0) as usize,
                    (slot.y + slot.height as i32 - 2).max(0) as usize,
                    slot.width as usize,
                    2,
                    0x79B7EC,
                );
                framebuffer::rect(
                    slot.x.max(0) as usize,
                    slot.y.max(0) as usize,
                    2,
                    slot.height as usize,
                    0x79B7EC,
                );
                framebuffer::rect(
                    (slot.x + slot.width as i32 - 2).max(0) as usize,
                    slot.y.max(0) as usize,
                    2,
                    slot.height as usize,
                    0x79B7EC,
                );
            }

            let icon = Rect::new(slot.x + 8, slot.y + 5, slot.width.saturating_sub(16), 52);
            Self::draw_desktop_surface_item_icon(icon, item.kind);

            let name = Self::trim_ascii_line(item.label.as_str(), 14);
            let name_color = if self.desktop_item_selected(desktop_cluster, item) {
                0xFFFFFF
            } else {
                0xEAF6FF
            };
            framebuffer::draw_text_5x7(
                (slot.x + 7).max(0) as usize,
                (slot.y + 62).max(0) as usize,
                name.as_str(),
                name_color,
            );

            let kind = if item.kind == ExplorerItemKind::Directory {
                "Carpeta"
            } else if Self::explorer_item_is_zip(item) {
                "ZIP"
            } else {
                "Archivo"
            };
            let kind_color = if self.desktop_item_selected(desktop_cluster, item) {
                0xD6EBFF
            } else {
                0x9EBDD8
            };
            framebuffer::draw_text_5x7(
                (slot.x + 7).max(0) as usize,
                (slot.y + 75).max(0) as usize,
                kind,
                kind_color,
            );
        }

        if !self.desktop_surface_status.is_empty() {
            let status =
                Self::trim_ascii_line(self.desktop_surface_status.as_str(), DESKTOP_STATUS_MAX_CHARS);
            framebuffer::draw_text_5x7(
                14,
                (self.taskbar.rect.y - 16).max(0) as usize,
                status.as_str(),
                0xC3DAEF,
            );
        }
    }

    fn draw_explorer_selection_overlay_for_window(&self, win: &Window) {
        if !win.is_explorer() {
            return;
        }
        if win.state != WindowState::Normal && win.state != WindowState::Maximized {
            return;
        }

        // Quick Access uses cluster 0; keep drawing selection there as well.
        let source_dir_cluster = win.explorer_current_cluster;

        for (idx, item) in win.explorer_items.iter().enumerate() {
            if !self.explorer_item_selected(win.id, source_dir_cluster, item) {
                continue;
            }

            let Some(slot) = win.explorer_item_global_rect(idx) else {
                continue;
            };
            framebuffer::rect(
                slot.x.max(0) as usize,
                slot.y.max(0) as usize,
                slot.width as usize,
                2,
                0x6BAFEA,
            );
            framebuffer::rect(
                slot.x.max(0) as usize,
                (slot.y + slot.height as i32 - 2).max(0) as usize,
                slot.width as usize,
                2,
                0x6BAFEA,
            );
            framebuffer::rect(
                slot.x.max(0) as usize,
                slot.y.max(0) as usize,
                2,
                slot.height as usize,
                0x6BAFEA,
            );
            framebuffer::rect(
                (slot.x + slot.width as i32 - 2).max(0) as usize,
                slot.y.max(0) as usize,
                2,
                slot.height as usize,
                0x6BAFEA,
            );
        }
    }

    fn handle_desktop_surface_right_click(&mut self, mouse_x: i32, mouse_y: i32) -> bool {
        if mouse_y >= self.taskbar.rect.y {
            self.desktop_context_menu = None;
            return false;
        }

        if self.desktop_usb_icon_rect().contains(self.mouse_pos) {
            return false;
        }

        if let Some((source_dir_cluster, _source_dir_path, items, item, _slot)) =
            self.desktop_surface_item_at(mouse_x, mouse_y)
        {
            let selected = self.desktop_collect_selected_items(source_dir_cluster, items.as_slice());
            if !self.desktop_item_selected(source_dir_cluster, &item) || selected.is_empty() {
                self.desktop_select_single(source_dir_cluster, &item);
            }
            let selected_count = self
                .desktop_collect_selected_items(source_dir_cluster, items.as_slice())
                .len();

            let kind = if item.kind == ExplorerItemKind::Directory {
                ExplorerContextMenuKind::DirectoryItem
            } else {
                ExplorerContextMenuKind::FileItem
            };
            let item_count =
                Self::explorer_context_item_count_for_kind(
                    kind,
                    Some(&item),
                    source_dir_cluster,
                    selected_count.max(1),
                );
            let (menu_x, menu_y) =
                self.clamp_explorer_context_menu_origin(mouse_x, mouse_y, item_count);
            self.desktop_context_menu = Some(ExplorerContextMenuState {
                win_id: 0,
                kind,
                x: menu_x,
                y: menu_y,
                source_dir_cluster,
                target_item: Some(item),
                show_paste: false,
                selection_count: selected_count.max(1),
            });
            return true;
        }

        let (source_dir_cluster, _source_dir_path) =
            match self.resolve_desktop_directory_target(false) {
            Ok(v) => v,
            Err(_) => {
                self.desktop_context_menu = None;
                return false;
            }
        };
        let show_paste = self.explorer_clipboard.is_some();
        let item_count = if show_paste { 3 } else { 2 };
        let (menu_x, menu_y) = self.clamp_explorer_context_menu_origin(mouse_x, mouse_y, item_count);
        self.desktop_context_menu = Some(ExplorerContextMenuState {
            win_id: 0,
            kind: ExplorerContextMenuKind::DesktopArea,
            x: menu_x,
            y: menu_y,
            source_dir_cluster,
            target_item: None,
            show_paste,
            selection_count: 0,
        });
        true
    }

    fn open_desktop_item(&mut self, source_dir_cluster: u32, source_dir_path: String, item: &ExplorerItem) {
        if item.kind == ExplorerItemKind::Directory {
            let explorer_id = self.create_explorer_window("File Explorer - Desktop", 140, 80, 920, 580);
            let source_path_hint = source_dir_path.clone();
            let mut next_path = source_dir_path;
            if !next_path.ends_with('/') {
                next_path.push('/');
            }
            next_path.push_str(item.label.as_str());
            next_path.push('/');
            let device_hint = self.resolve_device_index_for_directory(
                source_dir_cluster,
                Some(source_path_hint.as_str()),
                self.current_volume_device_index,
            );
            self.show_explorer_directory(
                explorer_id,
                item.cluster,
                next_path,
                alloc::format!("Folder: {}", item.label),
                device_hint,
            );
            self.desktop_surface_status = alloc::format!("Abierto: {}", item.label);
            return;
        }

        if item.kind == ExplorerItemKind::ShortcutRecycleBin {
            let mut fat = unsafe { crate::fat32::Fat32::new() };
            fat.ensure_subdirectory(fat.root_cluster, "TRASH");
            let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
            
            let explorer_id = self.create_explorer_window("File Explorer - Papelera", 140, 80, 920, 580);
            self.show_explorer_directory(
                explorer_id,
                trash_cluster,
                String::from("TRASH/"),
                String::from("Papelera de Reciclaje"),
                self.current_volume_device_index,
            );
            self.desktop_surface_status = String::from("Papelera abierta.");
            return;
        }

        if item.kind == ExplorerItemKind::File {
            if Self::is_png_file_name(item.label.as_str()) {
                self.open_png_from_explorer_file(0, item);
            } else {
                self.open_notepad_from_explorer_file(source_dir_cluster, source_dir_path, item);
            }
            self.desktop_surface_status = alloc::format!("Abierto: {}", item.label);
        }
    }

    fn handle_desktop_surface_left_click(&mut self, mouse_x: i32, mouse_y: i32) -> bool {
        if mouse_y >= self.taskbar.rect.y {
            return false;
        }
        if self.desktop_usb_icon_rect().contains(self.mouse_pos) {
            return false;
        }

        let Some((source_dir_cluster, source_dir_path, items, item, slot)) =
            self.desktop_surface_item_at(mouse_x, mouse_y)
        else {
            self.desktop_selected_items.clear();
            self.desktop_drag = None;
            return false;
        };

        let now = crate::timer::ticks();
        let mut is_double_click = false;
        if let Some(prev) = &self.last_explorer_click {
            if prev.win_id == 0
                && prev.kind == item.kind
                && prev.cluster == item.cluster
                && prev.label.eq_ignore_ascii_case(item.label.as_str())
                && Self::is_double_click_delta(now.saturating_sub(prev.tick))
            {
                is_double_click = true;
            }
        }

        self.last_explorer_click = Some(ExplorerClickState {
            win_id: 0,
            kind: item.kind,
            cluster: item.cluster,
            label: item.label.clone(),
            tick: now,
        });

        if is_double_click {
            self.last_explorer_click = None;
        }

        if self.desktop_selected_items.is_empty() {
            self.desktop_select_single(source_dir_cluster, &item);
        } else if !self.desktop_item_selected(source_dir_cluster, &item) {
            self.desktop_add_selection(source_dir_cluster, &item);
        }

        self.desktop_drag = Some(DesktopDragState {
            source_dir_cluster,
            source_dir_path: source_dir_path.clone(),
            item: item.clone(),
            cluster: item.cluster,
            label: item.label.clone(),
            offset_x: mouse_x - slot.x,
            offset_y: mouse_y - slot.y,
            start_mouse_x: mouse_x,
            start_mouse_y: mouse_y,
            moved: false,
            open_on_release: is_double_click,
        });

        let selected_count = self
            .desktop_collect_selected_items(source_dir_cluster, items.as_slice())
            .len();

        if item.kind == ExplorerItemKind::Directory || item.kind == ExplorerItemKind::ShortcutRecycleBin {
            self.desktop_surface_status = if selected_count > 1 {
                alloc::format!("{} elementos seleccionados.", selected_count)
            } else if is_double_click {
                alloc::format!("Carpeta lista para abrir: {}. Suelta para abrir.", item.label)
            } else {
                alloc::format!("Carpeta seleccionada: {}. Doble clic para abrir.", item.label)
            };
            return true;
        }

        if item.kind == ExplorerItemKind::File {
            if selected_count > 1 {
                self.desktop_surface_status = alloc::format!("{} elementos seleccionados.", selected_count);
            } else if is_double_click {
                self.desktop_surface_status =
                    alloc::format!("Archivo listo para abrir: {}. Suelta para abrir.", item.label);
            } else if Self::explorer_item_is_zip(&item) {
                self.desktop_surface_status = alloc::format!(
                    "Archivo ZIP: {}. Doble clic para abrir.",
                    item.label
                );
            } else {
                self.desktop_surface_status = alloc::format!(
                    "Archivo seleccionado: {}. Doble clic para abrir.",
                    item.label
                );
            }
            return true;
        }

        false
    }

    fn begin_explorer_create_folder(&mut self, win_id: usize) {
        let (dir_cluster, mut dir_path) = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => (win.explorer_current_cluster, win.explorer_path.clone()),
            None => return,
        };

        if dir_cluster < 2 {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("Crear carpeta: abre una carpeta destino.");
            }
            return;
        }

        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        if !dir_path.ends_with('/') {
            dir_path.push('/');
        }

        self.desktop_create_folder = Some(DesktopCreateFolderState {
            dir_cluster,
            dir_path,
            input: String::new(),
        });
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.set_explorer_status("Crear carpeta: escribe nombre y presiona Enter.");
        }
    }

    fn begin_desktop_create_folder(&mut self, dir_cluster: u32) {
        let dir_path = self
            .resolve_desktop_directory_target(true)
            .map(|(_, path)| path)
            .unwrap_or_else(|_| String::from("Desktop/"));

        self.desktop_create_folder = Some(DesktopCreateFolderState {
            dir_cluster,
            dir_path,
            input: String::new(),
        });
        self.desktop_surface_status =
            String::from("Crear carpeta: escribe nombre y presiona Enter.");
    }

    fn cancel_desktop_create_folder(&mut self) {
        self.desktop_create_folder = None;
        self.desktop_surface_status = String::from("Crear carpeta cancelado.");
    }

    fn commit_desktop_create_folder(&mut self) {
        let Some(prompt) = self.desktop_create_folder.clone() else {
            return;
        };
        let name = prompt.input.trim();
        if name.is_empty() {
            self.desktop_surface_status = String::from("Nombre vacio. Escribe un nombre.");
            return;
        }
        if name.bytes().any(|b| matches!(b, b'/' | b'\\' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|')) {
            self.desktop_surface_status = String::from("Nombre invalido para carpeta.");
            return;
        }
        if !self.ensure_fat_ready() {
            self.desktop_surface_status = String::from("No se pudo crear carpeta: FAT32 no disponible.");
            return;
        }

        let result = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            fat.ensure_subdirectory(prompt.dir_cluster, name)
        };

        match result {
            Ok(_) => {
                self.desktop_create_folder = None;
                self.desktop_surface_status = alloc::format!("Carpeta creada: {}", name);
                self.refresh_explorer_windows_for_cluster(
                    prompt.dir_cluster,
                    alloc::format!("Carpeta creada: {}", name).as_str(),
                    None,
                );
            }
            Err(err) => {
                self.desktop_surface_status =
                    alloc::format!("No se pudo crear carpeta '{}': {}", name, err);
            }
        }
    }

    fn handle_desktop_create_folder_key(&mut self, key: Option<char>, down: bool) -> bool {
        if self.desktop_create_folder.is_none() {
            return false;
        }
        if !down {
            return true;
        }

        match key {
            Some('\n') | Some('\r') => {
                self.commit_desktop_create_folder();
                true
            }
            Some('\x1b') => {
                self.cancel_desktop_create_folder();
                true
            }
            Some('\x08') | Some('\x7f') => {
                if let Some(prompt) = self.desktop_create_folder.as_mut() {
                    prompt.input.pop();
                }
                true
            }
            Some(ch) => {
                if !ch.is_ascii() || ch.is_control() {
                    return true;
                }
                if let Some(prompt) = self.desktop_create_folder.as_mut() {
                    if prompt.input.len() < 28 {
                        prompt.input.push(ch);
                    }
                }
                true
            }
            None => true,
        }
    }

    fn desktop_create_prompt_rect(&self) -> Rect {
        let x = ((self.width as i32 - DESKTOP_CREATE_PROMPT_W as i32) / 2).max(6);
        let y = ((self.taskbar.rect.y - DESKTOP_CREATE_PROMPT_H as i32) / 2).max(12);
        Rect::new(x, y, DESKTOP_CREATE_PROMPT_W, DESKTOP_CREATE_PROMPT_H)
    }

    fn desktop_create_prompt_cancel_rect(&self) -> Rect {
        let prompt = self.desktop_create_prompt_rect();
        Rect::new(
            prompt.x + prompt.width as i32 - DESKTOP_CREATE_PROMPT_BUTTON_W as i32 - 10,
            prompt.y + prompt.height as i32 - DESKTOP_CREATE_PROMPT_BUTTON_H as i32 - 12,
            DESKTOP_CREATE_PROMPT_BUTTON_W,
            DESKTOP_CREATE_PROMPT_BUTTON_H,
        )
    }

    fn desktop_create_prompt_ok_rect(&self) -> Rect {
        let cancel = self.desktop_create_prompt_cancel_rect();
        Rect::new(
            cancel.x - DESKTOP_CREATE_PROMPT_BUTTON_W as i32 - DESKTOP_CREATE_PROMPT_BUTTON_GAP,
            cancel.y,
            DESKTOP_CREATE_PROMPT_BUTTON_W,
            DESKTOP_CREATE_PROMPT_BUTTON_H,
        )
    }

    fn handle_desktop_create_folder_click(&mut self, mouse_x: i32, mouse_y: i32) -> bool {
        if self.desktop_create_folder.is_none() {
            return false;
        }

        let p = Point {
            x: mouse_x,
            y: mouse_y,
        };
        if self.desktop_create_prompt_ok_rect().contains(p) {
            self.commit_desktop_create_folder();
            return true;
        }
        if self.desktop_create_prompt_cancel_rect().contains(p) {
            self.cancel_desktop_create_folder();
            return true;
        }

        true
    }

    fn draw_desktop_create_folder_prompt(&mut self) {
        let Some(prompt) = self.desktop_create_folder.as_ref() else {
            return;
        };

        let rect = self.desktop_create_prompt_rect();

        framebuffer::rect(
            rect.x.max(0) as usize,
            rect.y.max(0) as usize,
            rect.width as usize,
            rect.height as usize,
            0x13273A,
        );
        framebuffer::rect(
            rect.x.max(0) as usize,
            rect.y.max(0) as usize,
            rect.width as usize,
            1,
            0x6EA2CE,
        );
        framebuffer::rect(
            rect.x.max(0) as usize,
            (rect.y + rect.height as i32 - 1).max(0) as usize,
            rect.width as usize,
            1,
            0x0D1826,
        );

        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + 12).max(0) as usize,
            "Crear carpeta",
            0xEAF6FF,
        );
        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + 28).max(0) as usize,
            Self::trim_ascii_line(prompt.dir_path.as_str(), 42).as_str(),
            0x9FC0DD,
        );

        let input_rect = Rect::new(rect.x + 10, rect.y + 46, rect.width.saturating_sub(20), 28);
        framebuffer::rect(
            input_rect.x.max(0) as usize,
            input_rect.y.max(0) as usize,
            input_rect.width as usize,
            input_rect.height as usize,
            0x0C1B2A,
        );
        framebuffer::rect(
            input_rect.x.max(0) as usize,
            input_rect.y.max(0) as usize,
            input_rect.width as usize,
            1,
            0x4F7A9D,
        );
        let shown = if prompt.input.is_empty() {
            "<nombre de carpeta>"
        } else {
            prompt.input.as_str()
        };
        framebuffer::draw_text_5x7(
            (input_rect.x + 8).max(0) as usize,
            (input_rect.y + 9).max(0) as usize,
            Self::trim_ascii_line(shown, 40).as_str(),
            if prompt.input.is_empty() { 0x6F8DA6 } else { 0xEAF6FF },
        );

        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + 86).max(0) as usize,
            "Enter: crear  Esc: cancelar",
            0xA9C6E1,
        );

        let ok_rect = self.desktop_create_prompt_ok_rect();
        framebuffer::rect(
            ok_rect.x.max(0) as usize,
            ok_rect.y.max(0) as usize,
            ok_rect.width as usize,
            ok_rect.height as usize,
            0x2B5D45,
        );
        framebuffer::rect(
            ok_rect.x.max(0) as usize,
            ok_rect.y.max(0) as usize,
            ok_rect.width as usize,
            1,
            0x7CC2A0,
        );
        framebuffer::draw_text_5x7(
            (ok_rect.x + 26).max(0) as usize,
            (ok_rect.y + 8).max(0) as usize,
            "OK",
            0xF1FFF7,
        );

        let cancel_rect = self.desktop_create_prompt_cancel_rect();
        framebuffer::rect(
            cancel_rect.x.max(0) as usize,
            cancel_rect.y.max(0) as usize,
            cancel_rect.width as usize,
            cancel_rect.height as usize,
            0x4A3441,
        );
        framebuffer::rect(
            cancel_rect.x.max(0) as usize,
            cancel_rect.y.max(0) as usize,
            cancel_rect.width as usize,
            1,
            0x9F6D84,
        );
        framebuffer::draw_text_5x7(
            (cancel_rect.x + 11).max(0) as usize,
            (cancel_rect.y + 8).max(0) as usize,
            "Cancel",
            0xFFEAF3,
        );
    }

    fn begin_rename_prompt_for_explorer_item(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) {
        if item.kind != ExplorerItemKind::File && item.kind != ExplorerItemKind::Directory {
            return;
        }

        let (source_dir_path, source_device_index) = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => (
                win.explorer_path.clone(),
                win.explorer_device_index.or(self.current_volume_device_index),
            ),
            None => return,
        };

        self.explorer_context_menu = None;
        self.desktop_context_menu = None;
        self.rename_prompt = Some(RenamePromptState {
            origin: RenamePromptOrigin::ExplorerWindow(win_id),
            source_dir_cluster,
            source_dir_path,
            source_device_index,
            source_item_cluster: item.cluster,
            source_label: item.label.clone(),
            source_is_directory: item.kind == ExplorerItemKind::Directory,
            input: item.label.clone(),
        });

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.set_explorer_status("Renombrar: escribe nuevo nombre y presiona Enter.");
        }
    }

    fn begin_rename_prompt_for_desktop_item(
        &mut self,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) {
        if item.kind != ExplorerItemKind::File && item.kind != ExplorerItemKind::Directory {
            return;
        }

        let source_dir_path = self
            .resolve_desktop_directory_target(true)
            .map(|(_, path)| path)
            .unwrap_or_else(|_| String::from("Desktop/"));

        self.explorer_context_menu = None;
        self.desktop_context_menu = None;
        self.rename_prompt = Some(RenamePromptState {
            origin: RenamePromptOrigin::Desktop,
            source_dir_cluster,
            source_dir_path,
            source_device_index: self.current_volume_device_index,
            source_item_cluster: item.cluster,
            source_label: item.label.clone(),
            source_is_directory: item.kind == ExplorerItemKind::Directory,
            input: item.label.clone(),
        });
        self.desktop_surface_status =
            String::from("Renombrar: escribe nuevo nombre y presiona Enter.");
    }

    fn cancel_rename_prompt(&mut self) {
        let Some(prompt) = self.rename_prompt.take() else {
            return;
        };

        match prompt.origin {
            RenamePromptOrigin::ExplorerWindow(win_id) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_explorer_status("Renombrar cancelado.");
                }
            }
            RenamePromptOrigin::Desktop => {
                self.desktop_surface_status = String::from("Renombrar cancelado.");
            }
        }
    }

    fn commit_rename_prompt(&mut self) {
        let Some(prompt) = self.rename_prompt.clone() else {
            return;
        };

        let target_name = prompt.input.trim();
        if target_name.is_empty() {
            match prompt.origin {
                RenamePromptOrigin::ExplorerWindow(win_id) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status("Nombre vacio. Escribe un nombre.");
                    }
                }
                RenamePromptOrigin::Desktop => {
                    self.desktop_surface_status = String::from("Nombre vacio. Escribe un nombre.");
                }
            }
            return;
        }

        if target_name.bytes().any(|b| {
            matches!(b, b'/' | b'\\' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|')
        }) {
            match prompt.origin {
                RenamePromptOrigin::ExplorerWindow(win_id) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status("Nombre invalido para renombrar.");
                    }
                }
                RenamePromptOrigin::Desktop => {
                    self.desktop_surface_status = String::from("Nombre invalido para renombrar.");
                }
            }
            return;
        }

        if prompt.source_label.eq_ignore_ascii_case(target_name) {
            self.rename_prompt = None;
            match prompt.origin {
                RenamePromptOrigin::ExplorerWindow(win_id) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status("Renombrar: sin cambios.");
                    }
                }
                RenamePromptOrigin::Desktop => {
                    self.desktop_surface_status = String::from("Renombrar: sin cambios.");
                }
            }
            return;
        }

        if let Some(index) = prompt.source_device_index {
            if !self.ensure_volume_index_mounted(index) {
                match prompt.origin {
                    RenamePromptOrigin::ExplorerWindow(win_id) => {
                        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                            win.set_explorer_status("No se pudo montar la unidad para renombrar.");
                        }
                    }
                    RenamePromptOrigin::Desktop => {
                        self.desktop_surface_status =
                            String::from("No se pudo montar la unidad para renombrar.");
                    }
                }
                return;
            }
        } else if !self.ensure_fat_ready() {
            match prompt.origin {
                RenamePromptOrigin::ExplorerWindow(win_id) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status("FAT32 no disponible para renombrar.");
                    }
                }
                RenamePromptOrigin::Desktop => {
                    self.desktop_surface_status = String::from("FAT32 no disponible para renombrar.");
                }
            }
            return;
        }

        let result = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let source_entry = if prompt.source_is_directory {
                Self::find_directory_entry_by_hint(
                    fat,
                    prompt.source_dir_cluster,
                    prompt.source_label.as_str(),
                    prompt.source_item_cluster,
                )
                .map_err(String::from)
            } else {
                Self::find_file_entry_by_hint(
                    fat,
                    prompt.source_dir_cluster,
                    prompt.source_label.as_str(),
                    prompt.source_item_cluster,
                )
                .map_err(String::from)
            };

            match source_entry {
                Ok(entry) => {
                    let source_name = Self::dir_entry_short_name(&entry);
                    fat.rename_entry_in_dir(
                        prompt.source_dir_cluster,
                        source_name.as_str(),
                        target_name,
                        Some(prompt.source_is_directory),
                    )
                    .map_err(String::from)
                }
                Err(err) => Err(err),
            }
        };

        match result {
            Ok(()) => {
                self.rename_prompt = None;
                let status = if prompt.source_is_directory {
                    alloc::format!("Carpeta renombrada: {}.", target_name)
                } else {
                    alloc::format!("Archivo renombrado: {}.", target_name)
                };

                match prompt.origin {
                    RenamePromptOrigin::ExplorerWindow(win_id) => {
                        if self.windows.iter().any(|w| w.id == win_id) {
                            self.show_explorer_directory(
                                win_id,
                                prompt.source_dir_cluster,
                                prompt.source_dir_path.clone(),
                                status.clone(),
                                prompt.source_device_index.or(self.current_volume_device_index),
                            );
                        }
                        self.refresh_explorer_windows_for_cluster(
                            prompt.source_dir_cluster,
                            status.as_str(),
                            Some(win_id),
                        );
                    }
                    RenamePromptOrigin::Desktop => {
                        self.desktop_selected_items.clear();
                        self.desktop_surface_status = status.clone();
                        self.refresh_explorer_windows_for_cluster(
                            prompt.source_dir_cluster,
                            status.as_str(),
                            None,
                        );
                    }
                }
            }
            Err(err) => {
                let mapped_err = if err.contains("Invalid destination 8.3 filename")
                    || err.contains("Invalid destination filename")
                {
                    String::from(
                        "nombre invalido para FAT32. Intenta evitar caracteres especiales de control.",
                    )
                } else {
                    err
                };
                match prompt.origin {
                    RenamePromptOrigin::ExplorerWindow(win_id) => {
                        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                            win.set_explorer_status(
                                alloc::format!(
                                    "No se pudo renombrar '{}': {}",
                                    prompt.source_label,
                                    mapped_err
                                )
                                .as_str(),
                            );
                        }
                    }
                    RenamePromptOrigin::Desktop => {
                        self.desktop_surface_status = alloc::format!(
                            "No se pudo renombrar '{}': {}",
                            prompt.source_label,
                            mapped_err
                        );
                    }
                }
            }
        }
    }

    fn handle_rename_prompt_key(&mut self, key: Option<char>, down: bool) -> bool {
        if self.rename_prompt.is_none() {
            return false;
        }
        if !down {
            return true;
        }

        match key {
            Some('\n') | Some('\r') => {
                self.commit_rename_prompt();
                true
            }
            Some('\x1b') => {
                self.cancel_rename_prompt();
                true
            }
            Some('\x08') | Some('\x7f') => {
                if let Some(prompt) = self.rename_prompt.as_mut() {
                    prompt.input.pop();
                }
                true
            }
            Some(ch) => {
                if !ch.is_ascii() || ch.is_control() {
                    return true;
                }
                if let Some(prompt) = self.rename_prompt.as_mut() {
                    if prompt.input.len() < RENAME_PROMPT_INPUT_MAX_CHARS {
                        prompt.input.push(ch);
                    }
                }
                true
            }
            None => true,
        }
    }

    fn rename_prompt_rect(&self) -> Rect {
        let x = ((self.width as i32 - DESKTOP_CREATE_PROMPT_W as i32) / 2).max(6);
        let y = ((self.taskbar.rect.y - DESKTOP_CREATE_PROMPT_H as i32) / 2).max(12);
        Rect::new(x, y, DESKTOP_CREATE_PROMPT_W, DESKTOP_CREATE_PROMPT_H)
    }

    fn rename_prompt_cancel_rect(&self) -> Rect {
        let prompt = self.rename_prompt_rect();
        Rect::new(
            prompt.x + prompt.width as i32 - DESKTOP_CREATE_PROMPT_BUTTON_W as i32 - 10,
            prompt.y + prompt.height as i32 - DESKTOP_CREATE_PROMPT_BUTTON_H as i32 - 12,
            DESKTOP_CREATE_PROMPT_BUTTON_W,
            DESKTOP_CREATE_PROMPT_BUTTON_H,
        )
    }

    fn rename_prompt_ok_rect(&self) -> Rect {
        let cancel = self.rename_prompt_cancel_rect();
        Rect::new(
            cancel.x - DESKTOP_CREATE_PROMPT_BUTTON_W as i32 - DESKTOP_CREATE_PROMPT_BUTTON_GAP,
            cancel.y,
            DESKTOP_CREATE_PROMPT_BUTTON_W,
            DESKTOP_CREATE_PROMPT_BUTTON_H,
        )
    }

    fn handle_rename_prompt_click(&mut self, mouse_x: i32, mouse_y: i32) -> bool {
        if self.rename_prompt.is_none() {
            return false;
        }

        let p = Point {
            x: mouse_x,
            y: mouse_y,
        };
        if self.rename_prompt_ok_rect().contains(p) {
            self.commit_rename_prompt();
            return true;
        }
        if self.rename_prompt_cancel_rect().contains(p) {
            self.cancel_rename_prompt();
            return true;
        }

        true
    }

    fn draw_rename_prompt(&mut self) {
        let Some(prompt) = self.rename_prompt.as_ref() else {
            return;
        };

        let rect = self.rename_prompt_rect();
        framebuffer::rect(
            rect.x.max(0) as usize,
            rect.y.max(0) as usize,
            rect.width as usize,
            rect.height as usize,
            0x13273A,
        );
        framebuffer::rect(
            rect.x.max(0) as usize,
            rect.y.max(0) as usize,
            rect.width as usize,
            1,
            0x6EA2CE,
        );
        framebuffer::rect(
            rect.x.max(0) as usize,
            (rect.y + rect.height as i32 - 1).max(0) as usize,
            rect.width as usize,
            1,
            0x0D1826,
        );

        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + 12).max(0) as usize,
            if prompt.source_is_directory {
                "Renombrar carpeta"
            } else {
                "Renombrar archivo"
            },
            0xEAF6FF,
        );

        let mut source_hint = String::from(prompt.source_dir_path.trim());
        while source_hint.ends_with('/') {
            source_hint.pop();
        }
        if source_hint.is_empty() {
            source_hint.push('/');
        }
        if !source_hint.ends_with('/') {
            source_hint.push('/');
        }
        source_hint.push_str(prompt.source_label.as_str());
        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + 28).max(0) as usize,
            Self::trim_ascii_line(source_hint.as_str(), 42).as_str(),
            0x9FC0DD,
        );

        let input_rect = Rect::new(rect.x + 10, rect.y + 46, rect.width.saturating_sub(20), 28);
        framebuffer::rect(
            input_rect.x.max(0) as usize,
            input_rect.y.max(0) as usize,
            input_rect.width as usize,
            input_rect.height as usize,
            0x0C1B2A,
        );
        framebuffer::rect(
            input_rect.x.max(0) as usize,
            input_rect.y.max(0) as usize,
            input_rect.width as usize,
            1,
            0x4F7A9D,
        );

        let shown = if prompt.input.is_empty() {
            "<nuevo nombre>"
        } else {
            prompt.input.as_str()
        };
        framebuffer::draw_text_5x7(
            (input_rect.x + 8).max(0) as usize,
            (input_rect.y + 9).max(0) as usize,
            Self::trim_ascii_line(shown, 40).as_str(),
            if prompt.input.is_empty() { 0x6F8DA6 } else { 0xEAF6FF },
        );

        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + 86).max(0) as usize,
            "Enter: confirmar  Esc: cancelar",
            0xA9C6E1,
        );

        let ok_rect = self.rename_prompt_ok_rect();
        framebuffer::rect(
            ok_rect.x.max(0) as usize,
            ok_rect.y.max(0) as usize,
            ok_rect.width as usize,
            ok_rect.height as usize,
            0x2B5D45,
        );
        framebuffer::rect(
            ok_rect.x.max(0) as usize,
            ok_rect.y.max(0) as usize,
            ok_rect.width as usize,
            1,
            0x7CC2A0,
        );
        framebuffer::draw_text_5x7(
            (ok_rect.x + 26).max(0) as usize,
            (ok_rect.y + 8).max(0) as usize,
            "OK",
            0xF1FFF7,
        );

        let cancel_rect = self.rename_prompt_cancel_rect();
        framebuffer::rect(
            cancel_rect.x.max(0) as usize,
            cancel_rect.y.max(0) as usize,
            cancel_rect.width as usize,
            cancel_rect.height as usize,
            0x4A3441,
        );
        framebuffer::rect(
            cancel_rect.x.max(0) as usize,
            cancel_rect.y.max(0) as usize,
            cancel_rect.width as usize,
            1,
            0x9F6D84,
        );
        framebuffer::draw_text_5x7(
            (cancel_rect.x + 11).max(0) as usize,
            (cancel_rect.y + 8).max(0) as usize,
            "Cancel",
            0xFFEAF3,
        );
    }

    fn copy_progress_prompt_rect(&self) -> Rect {
        let x = ((self.width as i32 - COPY_PROGRESS_PROMPT_W as i32) / 2).max(8);
        let y = ((self.taskbar.rect.y - COPY_PROGRESS_PROMPT_H as i32) / 2).max(16);
        Rect::new(x, y, COPY_PROGRESS_PROMPT_W, COPY_PROGRESS_PROMPT_H)
    }

    fn copy_progress_prompt_cancel_rect(&self) -> Rect {
        let prompt = self.copy_progress_prompt_rect();
        Rect::new(
            prompt.x + prompt.width as i32 - COPY_PROGRESS_PROMPT_BUTTON_W as i32 - 12,
            prompt.y + prompt.height as i32 - COPY_PROGRESS_PROMPT_BUTTON_H as i32 - 12,
            COPY_PROGRESS_PROMPT_BUTTON_W,
            COPY_PROGRESS_PROMPT_BUTTON_H,
        )
    }

    fn copy_progress_prompt_minimize_rect(&self) -> Rect {
        let prompt = self.copy_progress_prompt_rect();
        Rect::new(
            prompt.x + prompt.width as i32 - COPY_PROGRESS_PROMPT_HEADER_BUTTON_W as i32 - 12,
            prompt.y + 10,
            COPY_PROGRESS_PROMPT_HEADER_BUTTON_W,
            COPY_PROGRESS_PROMPT_HEADER_BUTTON_H,
        )
    }

    fn copy_progress_prompt_minimized_rect(&self) -> Rect {
        let available_w = self.width.saturating_sub(24) as u32;
        let width = if available_w >= COPY_PROGRESS_PROMPT_MINI_W {
            COPY_PROGRESS_PROMPT_MINI_W
        } else {
            available_w
        };
        let x = (self.width as i32 - width as i32 - 12).max(4);
        let y = (self.taskbar.rect.y - COPY_PROGRESS_PROMPT_MINI_H as i32 - 10).max(6);
        Rect::new(x, y, width, COPY_PROGRESS_PROMPT_MINI_H)
    }

    fn copy_progress_prompt_restore_rect(&self) -> Rect {
        let mini = self.copy_progress_prompt_minimized_rect();
        Rect::new(
            mini.x + mini.width as i32 - (COPY_PROGRESS_PROMPT_MINI_BUTTON_W as i32 * 2) - 12,
            mini.y + mini.height as i32 - COPY_PROGRESS_PROMPT_MINI_BUTTON_H as i32 - 8,
            COPY_PROGRESS_PROMPT_MINI_BUTTON_W,
            COPY_PROGRESS_PROMPT_MINI_BUTTON_H,
        )
    }

    fn copy_progress_prompt_minimized_cancel_rect(&self) -> Rect {
        let mini = self.copy_progress_prompt_minimized_rect();
        Rect::new(
            mini.x + mini.width as i32 - COPY_PROGRESS_PROMPT_MINI_BUTTON_W as i32 - 8,
            mini.y + mini.height as i32 - COPY_PROGRESS_PROMPT_MINI_BUTTON_H as i32 - 8,
            COPY_PROGRESS_PROMPT_MINI_BUTTON_W,
            COPY_PROGRESS_PROMPT_MINI_BUTTON_H,
        )
    }

    fn begin_copy_progress_prompt(
        &mut self,
        title: &str,
        total_units: usize,
        total_items: usize,
        modal: bool,
    ) {
        let now = crate::timer::ticks();
        self.copy_progress_prompt = Some(CopyProgressPromptState {
            title: String::from(title),
            detail: String::from("Preparando copia..."),
            percent: 0,
            done_units: 0,
            total_units: total_units.max(1),
            done_items: 0,
            total_items: total_items.max(1),
            cancel_requested: false,
            modal,
            minimized: false,
            last_paint_tick: now,
            last_input_tick: now,
        });
        self.copy_progress_sync(None, 0, 0, true);
    }

    fn finish_copy_progress_prompt(&mut self) {
        self.copy_progress_prompt = None;
    }

    fn copy_progress_cancel_requested(&self) -> bool {
        self.copy_progress_prompt
            .as_ref()
            .map(|prompt| prompt.cancel_requested)
            .unwrap_or(false)
    }

    fn copy_progress_prompt_is_modal(&self) -> bool {
        self.copy_progress_prompt
            .as_ref()
            .map(|prompt| prompt.modal)
            .unwrap_or(false)
    }

    fn copy_progress_prompt_is_minimized(&self) -> bool {
        self.copy_progress_prompt
            .as_ref()
            .map(|prompt| prompt.minimized)
            .unwrap_or(false)
    }

    fn request_copy_progress_cancel(&mut self) {
        if let Some(prompt) = self.copy_progress_prompt.as_mut() {
            if !prompt.cancel_requested {
                prompt.cancel_requested = true;
                prompt.detail = String::from("Cancelando copia...");
            }
        }
    }

    fn copy_progress_percent(done_units: usize, total_units: usize) -> u8 {
        if total_units == 0 {
            return 100;
        }
        let pct = done_units.saturating_mul(100) / total_units;
        pct.min(100) as u8
    }

    fn copy_progress_sync(
        &mut self,
        detail: Option<&str>,
        add_units: usize,
        add_items: usize,
        force_paint: bool,
    ) {
        let mut paint_needed = force_paint;
        let mut pump_input = false;
        let now = crate::timer::ticks();
        {
            let Some(prompt) = self.copy_progress_prompt.as_mut() else {
                return;
            };

            if let Some(text) = detail {
                let clipped = Self::trim_ascii_line(text, 56);
                if prompt.detail != clipped {
                    prompt.detail = clipped;
                    paint_needed = true;
                }
            }

            if add_units > 0 {
                let total = prompt.total_units.max(1);
                prompt.done_units = prompt.done_units.saturating_add(add_units).min(total);
            }
            if add_items > 0 {
                let total = prompt.total_items.max(1);
                prompt.done_items = prompt.done_items.saturating_add(add_items).min(total);
            }

            let prev = prompt.percent;
            prompt.percent = Self::copy_progress_percent(prompt.done_units, prompt.total_units);
            let percent_changed = prompt.percent != prev;
            if add_items > 0 {
                paint_needed = true;
            }
            if percent_changed && prompt.percent >= 100 {
                paint_needed = true;
            }

            if add_units > 0
                && now.saturating_sub(prompt.last_paint_tick) >= COPY_PROGRESS_PAINT_INTERVAL_TICKS
            {
                paint_needed = true;
            }
            if paint_needed {
                prompt.last_paint_tick = now;
                prompt.last_input_tick = now;
                pump_input = true;
            } else if now.saturating_sub(prompt.last_input_tick)
                >= COPY_PROGRESS_INPUT_POLL_INTERVAL_TICKS
            {
                prompt.last_input_tick = now;
                pump_input = true;
            }
        }
        if pump_input {
            self.pump_copy_progress_prompt_input(paint_needed);
        }
    }

    fn copy_progress_touch(&mut self, detail: &str) {
        self.copy_progress_sync(Some(detail), 0, 0, false);
    }

    fn copy_progress_advance_units(&mut self, units: usize) {
        if units == 0 {
            self.copy_progress_sync(None, 0, 0, false);
        } else {
            self.copy_progress_sync(None, units, 0, false);
        }
    }

    fn copy_progress_advance_item(&mut self, detail: Option<&str>) {
        self.copy_progress_sync(detail, 0, 1, false);
    }

    fn copy_progress_cancel_error() -> String {
        String::from("Operacion cancelada por usuario.")
    }

    fn is_copy_cancel_error(err: &str) -> bool {
        let lower = Self::ascii_lower(err);
        lower.contains("cancelada por usuario")
            || lower.contains("cancelado por usuario")
            || lower.contains("operation canceled")
    }

    fn copy_progress_abort_if_cancelled(&self) -> Result<(), String> {
        if self.copy_progress_cancel_requested() {
            Err(Self::copy_progress_cancel_error())
        } else {
            Ok(())
        }
    }

    fn handle_copy_progress_prompt_key(&mut self, _key: Option<char>, down: bool) -> bool {
        if self.copy_progress_prompt.is_none() {
            return false;
        }
        if !self.copy_progress_prompt_is_modal() {
            return false;
        }
        if !down {
            return true;
        }
        true
    }

    fn handle_copy_progress_prompt_click(&mut self, mouse_x: i32, mouse_y: i32) -> bool {
        if self.copy_progress_prompt.is_none() {
            return false;
        }
        let p = Point {
            x: mouse_x,
            y: mouse_y,
        };
        if self.copy_progress_prompt_is_minimized() {
            if self.copy_progress_prompt_restore_rect().contains(p) {
                if let Some(prompt) = self.copy_progress_prompt.as_mut() {
                    prompt.minimized = false;
                }
                return true;
            }
            if self.copy_progress_prompt_minimized_cancel_rect().contains(p) {
                self.request_copy_progress_cancel();
                return true;
            }
            if self.copy_progress_prompt_minimized_rect().contains(p) {
                return true;
            }
            return self.copy_progress_prompt_is_modal();
        }
        if self.copy_progress_prompt_minimize_rect().contains(p) {
            if let Some(prompt) = self.copy_progress_prompt.as_mut() {
                prompt.minimized = true;
            }
            return true;
        }
        if self.copy_progress_prompt_cancel_rect().contains(p) {
            self.request_copy_progress_cancel();
            return true;
        }
        self.copy_progress_prompt_is_modal()
    }

    fn pump_copy_progress_prompt_input(&mut self, force_paint: bool) {
        if self.copy_progress_prompt.is_none() {
            return;
        }

        let mut paint_needed = force_paint;
        while crate::input::poll_input_uefi().is_some() {
            // Copy cancellation is intentionally button-only.
        }

        while let Some((dx, dy, _wheel_delta, left_btn, right_btn)) = crate::input::poll_mouse_uefi() {
            let max_x = self.width.saturating_sub(1) as i32;
            let max_y = self.height.saturating_sub(1) as i32;
            self.mouse_pos.x = self.mouse_pos.x.saturating_add(dx).clamp(0, max_x);
            self.mouse_pos.y = self.mouse_pos.y.saturating_add(dy).clamp(0, max_y);
            let was_left_down = self.last_mouse_down;
            self.last_mouse_down = left_btn;
            self.last_mouse_right_down = right_btn;
            if left_btn && !was_left_down {
                self.handle_copy_progress_prompt_click(self.mouse_pos.x, self.mouse_pos.y);
            }
            paint_needed = true;
        }

        if paint_needed {
            self.paint();
        }
    }

    fn draw_copy_progress_prompt(&mut self) {
        let Some(prompt) = self.copy_progress_prompt.as_ref() else {
            return;
        };

        if prompt.minimized {
            let rect = self.copy_progress_prompt_minimized_rect();
            framebuffer::rect(
                rect.x.max(0) as usize,
                rect.y.max(0) as usize,
                rect.width as usize,
                rect.height as usize,
                0x10273D,
            );
            framebuffer::rect(
                rect.x.max(0) as usize,
                rect.y.max(0) as usize,
                rect.width as usize,
                1,
                0x62ACDF,
            );
            framebuffer::draw_text_5x7(
                (rect.x + 8).max(0) as usize,
                (rect.y + 10).max(0) as usize,
                alloc::format!(
                    "{}  {}%",
                    Self::trim_ascii_line(prompt.title.as_str(), 22),
                    prompt.percent
                )
                .as_str(),
                0xE8F6FF,
            );
            let bar = Rect::new(rect.x + 8, rect.y + 24, rect.width.saturating_sub(16), 10);
            framebuffer::rect(
                bar.x.max(0) as usize,
                bar.y.max(0) as usize,
                bar.width as usize,
                bar.height as usize,
                0x0A1824,
            );
            let fill_w = (bar.width as usize)
                .saturating_mul(prompt.percent as usize)
                .saturating_div(100)
                .min(bar.width as usize);
            if fill_w > 0 {
                framebuffer::rect(
                    bar.x.max(0) as usize,
                    bar.y.max(0) as usize,
                    fill_w,
                    bar.height as usize,
                    if prompt.cancel_requested { 0x8E3D47 } else { 0x2A74A5 },
                );
            }

            let restore = self.copy_progress_prompt_restore_rect();
            framebuffer::rect(
                restore.x.max(0) as usize,
                restore.y.max(0) as usize,
                restore.width as usize,
                restore.height as usize,
                0x2D4F6A,
            );
            framebuffer::rect(
                restore.x.max(0) as usize,
                restore.y.max(0) as usize,
                restore.width as usize,
                1,
                0x79BEEA,
            );
            framebuffer::draw_text_5x7(
                (restore.x + 18).max(0) as usize,
                (restore.y + 7).max(0) as usize,
                "Open",
                0xEFF9FF,
            );

            let cancel = self.copy_progress_prompt_minimized_cancel_rect();
            framebuffer::rect(
                cancel.x.max(0) as usize,
                cancel.y.max(0) as usize,
                cancel.width as usize,
                cancel.height as usize,
                if prompt.cancel_requested { 0x5D2E35 } else { 0x4D3342 },
            );
            framebuffer::rect(
                cancel.x.max(0) as usize,
                cancel.y.max(0) as usize,
                cancel.width as usize,
                1,
                if prompt.cancel_requested { 0xD58A95 } else { 0xA8738B },
            );
            framebuffer::draw_text_5x7(
                (cancel.x + 11).max(0) as usize,
                (cancel.y + 7).max(0) as usize,
                if prompt.cancel_requested { "Canceling" } else { "Cancel" },
                0xFFE9F1,
            );
            return;
        }

        let rect = self.copy_progress_prompt_rect();
        framebuffer::rect(
            rect.x.max(0) as usize,
            rect.y.max(0) as usize,
            rect.width as usize,
            rect.height as usize,
            0x12324A,
        );
        framebuffer::rect(
            rect.x.max(0) as usize,
            rect.y.max(0) as usize,
            rect.width as usize,
            1,
            0x7CC4F2,
        );
        framebuffer::rect(
            rect.x.max(0) as usize,
            (rect.y + rect.height as i32 - 1).max(0) as usize,
            rect.width as usize,
            1,
            0x0B1823,
        );

        framebuffer::draw_text_5x7(
            (rect.x + 12).max(0) as usize,
            (rect.y + 12).max(0) as usize,
            Self::trim_ascii_line(prompt.title.as_str(), 48).as_str(),
            0xECF8FF,
        );
        framebuffer::draw_text_5x7(
            (rect.x + 12).max(0) as usize,
            (rect.y + 30).max(0) as usize,
            Self::trim_ascii_line(prompt.detail.as_str(), 56).as_str(),
            if prompt.cancel_requested { 0xFFCFD7 } else { 0xAED2EA },
        );

        let bar_rect = Rect::new(rect.x + 12, rect.y + 56, rect.width.saturating_sub(24), 20);
        framebuffer::rect(
            bar_rect.x.max(0) as usize,
            bar_rect.y.max(0) as usize,
            bar_rect.width as usize,
            bar_rect.height as usize,
            0x081724,
        );
        framebuffer::rect(
            bar_rect.x.max(0) as usize,
            bar_rect.y.max(0) as usize,
            bar_rect.width as usize,
            1,
            0x355A79,
        );
        let fill_w = (bar_rect.width as usize)
            .saturating_mul(prompt.percent as usize)
            .saturating_div(100)
            .min(bar_rect.width as usize);
        if fill_w > 0 {
            framebuffer::rect(
                bar_rect.x.max(0) as usize,
                bar_rect.y.max(0) as usize,
                fill_w,
                bar_rect.height as usize,
                if prompt.cancel_requested { 0x8E3D47 } else { 0x2A74A5 },
            );
        }
        framebuffer::draw_text_5x7(
            (bar_rect.x + 6).max(0) as usize,
            (bar_rect.y + 7).max(0) as usize,
            alloc::format!("{}%", prompt.percent).as_str(),
            0xF2FAFF,
        );

        framebuffer::draw_text_5x7(
            (rect.x + 12).max(0) as usize,
            (rect.y + 86).max(0) as usize,
            {
                let done_mb_x10 = prompt.done_units.saturating_mul(10) / (1024 * 1024);
                let total_mb_x10 = prompt.total_units.saturating_mul(10) / (1024 * 1024);
                alloc::format!(
                    "Items: {}/{}  Data: {}.{} / {}.{} MB",
                    prompt.done_items,
                    prompt.total_items,
                    done_mb_x10 / 10,
                    done_mb_x10 % 10,
                    total_mb_x10 / 10,
                    total_mb_x10 % 10
                )
            }
            .as_str(),
            0xA7CCE7,
        );
        framebuffer::draw_text_5x7(
            (rect.x + 12).max(0) as usize,
            (rect.y + 104).max(0) as usize,
            if prompt.cancel_requested {
                "Cancel requested. Finishing current step..."
            } else if prompt.modal {
                "Use Cancel button to stop."
            } else {
                "Background task running. Use Cancel to stop."
            },
            0xC6DFF0,
        );

        let minimize = self.copy_progress_prompt_minimize_rect();
        framebuffer::rect(
            minimize.x.max(0) as usize,
            minimize.y.max(0) as usize,
            minimize.width as usize,
            minimize.height as usize,
            0x2E4F65,
        );
        framebuffer::rect(
            minimize.x.max(0) as usize,
            minimize.y.max(0) as usize,
            minimize.width as usize,
            1,
            0x78B6D8,
        );
        framebuffer::draw_text_5x7(
            (minimize.x + 8).max(0) as usize,
            (minimize.y + 4).max(0) as usize,
            "_",
            0xEAF8FF,
        );

        let cancel = self.copy_progress_prompt_cancel_rect();
        framebuffer::rect(
            cancel.x.max(0) as usize,
            cancel.y.max(0) as usize,
            cancel.width as usize,
            cancel.height as usize,
            if prompt.cancel_requested { 0x5D2E35 } else { 0x4D3342 },
        );
        framebuffer::rect(
            cancel.x.max(0) as usize,
            cancel.y.max(0) as usize,
            cancel.width as usize,
            1,
            if prompt.cancel_requested { 0xD58A95 } else { 0xA8738B },
        );
        framebuffer::draw_text_5x7(
            (cancel.x + 13).max(0) as usize,
            (cancel.y + 9).max(0) as usize,
            if prompt.cancel_requested { "Canceling" } else { "Cancel" },
            0xFFE9F1,
        );
    }

    fn notepad_save_prompt_rect(&self) -> Rect {
        let x = ((self.width as i32 - NOTEPAD_SAVE_PROMPT_W as i32) / 2).max(6);
        let y = ((self.taskbar.rect.y - NOTEPAD_SAVE_PROMPT_H as i32) / 2).max(12);
        Rect::new(x, y, NOTEPAD_SAVE_PROMPT_W, NOTEPAD_SAVE_PROMPT_H)
    }

    fn notepad_save_prompt_cancel_rect(&self) -> Rect {
        let prompt = self.notepad_save_prompt_rect();
        Rect::new(
            prompt.x + prompt.width as i32 - NOTEPAD_SAVE_PROMPT_BUTTON_W as i32 - 10,
            prompt.y + prompt.height as i32 - NOTEPAD_SAVE_PROMPT_BUTTON_H as i32 - 12,
            NOTEPAD_SAVE_PROMPT_BUTTON_W,
            NOTEPAD_SAVE_PROMPT_BUTTON_H,
        )
    }

    fn notepad_save_prompt_ok_rect(&self) -> Rect {
        let cancel = self.notepad_save_prompt_cancel_rect();
        Rect::new(
            cancel.x - NOTEPAD_SAVE_PROMPT_BUTTON_W as i32 - NOTEPAD_SAVE_PROMPT_BUTTON_GAP,
            cancel.y,
            NOTEPAD_SAVE_PROMPT_BUTTON_W,
            NOTEPAD_SAVE_PROMPT_BUTTON_H,
        )
    }

    fn notepad_save_prompt_list_rect(&self) -> Rect {
        let prompt = self.notepad_save_prompt_rect();
        Rect::new(
            prompt.x + 10,
            prompt.y + 46,
            prompt.width.saturating_sub(20),
            prompt.height.saturating_sub(106),
        )
    }

    fn notepad_save_prompt_item_rect(&self, visible_row: usize) -> Rect {
        let list = self.notepad_save_prompt_list_rect();
        Rect::new(
            list.x + 2,
            list.y + 2 + visible_row as i32 * NOTEPAD_SAVE_PROMPT_ITEM_H as i32,
            list.width.saturating_sub(4),
            NOTEPAD_SAVE_PROMPT_ITEM_H,
        )
    }

    fn notepad_save_prompt_toggle_rect(&self, item_rect: Rect, depth: u8) -> Rect {
        let indent = (depth as i32 * 12).clamp(0, item_rect.width.saturating_sub(20) as i32);
        Rect::new(item_rect.x + 4 + indent, item_rect.y + 6, 8, 8)
    }

    fn notepad_save_prompt_visible_item_count(&self) -> usize {
        let list = self.notepad_save_prompt_list_rect();
        ((list.height.saturating_sub(4) / NOTEPAD_SAVE_PROMPT_ITEM_H).max(1)) as usize
    }

    fn notepad_prompt_node_visible(prompt: &NotepadSavePromptState, index: usize) -> bool {
        if index >= prompt.locations.len() {
            return false;
        }
        let mut parent = prompt.locations[index].parent;
        while let Some(parent_idx) = parent {
            let Some(node) = prompt.locations.get(parent_idx) else {
                return false;
            };
            if !node.expanded {
                return false;
            }
            parent = node.parent;
        }
        true
    }

    fn notepad_prompt_visible_indices(prompt: &NotepadSavePromptState) -> Vec<usize> {
        let mut out = Vec::new();
        for idx in 0..prompt.locations.len() {
            if Self::notepad_prompt_node_visible(prompt, idx) {
                out.push(idx);
            }
        }
        out
    }

    fn notepad_prompt_expand_ancestors(prompt: &mut NotepadSavePromptState, index: usize) {
        if index >= prompt.locations.len() {
            return;
        }
        let mut parent = prompt.locations[index].parent;
        while let Some(parent_idx) = parent {
            let next = prompt.locations.get(parent_idx).and_then(|node| node.parent);
            if let Some(node) = prompt.locations.get_mut(parent_idx) {
                if node.has_children {
                    node.expanded = true;
                }
            }
            parent = next;
        }
    }

    fn ensure_notepad_prompt_scroll(prompt: &mut NotepadSavePromptState, visible_count: usize) {
        let visible_indices = Self::notepad_prompt_visible_indices(prompt);
        if visible_indices.is_empty() {
            prompt.selected_index = 0;
            prompt.scroll_top = 0;
            return;
        }

        if !visible_indices.iter().any(|idx| *idx == prompt.selected_index) {
            prompt.selected_index = visible_indices[0];
        }

        if visible_count == 0 {
            prompt.scroll_top = 0;
            return;
        }

        let selected_row = visible_indices
            .iter()
            .position(|idx| *idx == prompt.selected_index)
            .unwrap_or(0);

        let max_scroll = visible_indices.len().saturating_sub(visible_count);
        if prompt.scroll_top > max_scroll {
            prompt.scroll_top = max_scroll;
        }

        if selected_row < prompt.scroll_top {
            prompt.scroll_top = selected_row;
        }

        let bottom = prompt.scroll_top.saturating_add(visible_count);
        if selected_row >= bottom {
            prompt.scroll_top = selected_row.saturating_sub(visible_count.saturating_sub(1));
        }
    }

    fn push_notepad_save_location(
        locations: &mut Vec<NotepadSaveLocation>,
        device_index: Option<usize>,
        cluster: u32,
        path: &str,
        label: &str,
        is_unit: bool,
        parent: Option<usize>,
        depth: u8,
        expanded: bool,
        has_children: bool,
    ) -> Option<usize> {
        let mut clean_path = String::from(path.trim());
        if clean_path.is_empty() {
            return None;
        }
        if !clean_path.ends_with('/') {
            clean_path.push('/');
        }
        let normalized = Self::ascii_lower(clean_path.trim_end_matches('/'));
        if let Some((idx, _)) = locations.iter().enumerate().find(|(_, loc)| {
            loc.device_index == device_index
                && (loc.cluster == cluster
                    || Self::ascii_lower(loc.path.trim_end_matches('/')) == normalized)
        }) {
            return Some(idx);
        }

        let clean_label = label.trim();
        if clean_label.is_empty() {
            return None;
        }

        let idx = locations.len();
        locations.push(NotepadSaveLocation {
            device_index,
            cluster,
            path: clean_path,
            label: String::from(clean_label),
            is_unit,
            depth,
            parent,
            expanded,
            has_children,
        });
        Some(idx)
    }

    fn notepad_location_has_subdirs(fat: &mut crate::fat32::Fat32, cluster: u32) -> bool {
        use crate::fs::FileType;

        let Ok(entries) = fat.read_dir_entries(cluster) else {
            return false;
        };
        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::Directory {
                continue;
            }
            let name = entry.full_name();
            if name != "." && name != ".." {
                return true;
            }
        }
        false
    }

    fn append_notepad_tree_children(
        fat: &mut crate::fat32::Fat32,
        locations: &mut Vec<NotepadSaveLocation>,
        device_index: Option<usize>,
        parent_index: usize,
        dir_cluster: u32,
        dir_path: &str,
        depth: u8,
    ) {
        use crate::fs::FileType;

        if depth >= 24 {
            return;
        }

        let Ok(entries) = fat.read_dir_entries(dir_cluster) else {
            return;
        };

        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::Directory {
                continue;
            }
            let name = entry.full_name();
            if name == "." || name == ".." {
                continue;
            }

            let child_cluster = if entry.cluster == 0 {
                fat.root_cluster
            } else {
                entry.cluster
            };
            let child_path = alloc::format!("{}{}/", dir_path, name);
            let has_children = Self::notepad_location_has_subdirs(fat, child_cluster);
            let Some(child_idx) = Self::push_notepad_save_location(
                locations,
                device_index,
                child_cluster,
                child_path.as_str(),
                name.as_str(),
                false,
                Some(parent_index),
                depth,
                false,
                has_children,
            ) else {
                continue;
            };

            if has_children {
                Self::append_notepad_tree_children(
                    fat,
                    locations,
                    device_index,
                    child_idx,
                    child_cluster,
                    child_path.as_str(),
                    depth.saturating_add(1),
                );
            }
        }
    }

    fn begin_notepad_save_prompt(&mut self, win_id: usize) {
        let (current_cluster, current_path) = match self.windows.iter().find(|w| w.id == win_id && w.is_notepad()) {
            Some(win) => (win.notepad_dir_cluster, win.notepad_dir_path.clone()),
            None => return,
        };
        let _ = self.ensure_fat_ready();
        let current_device_index = self.current_volume_device_index;

        let (mut locations, mut selected_index) = {
            let mut locations = Vec::new();

            let mut device_targets: Vec<(usize, String)> = Vec::new();
            let devices = crate::fat32::Fat32::detect_uefi_block_devices();
            for dev in devices.iter() {
                if !dev.logical_partition {
                    continue;
                }
                let media = if dev.removable { "USB" } else { "NVME/HDD" };
                let title = alloc::format!("{} {} ({} MiB)", media, dev.index, dev.total_mib);
                device_targets.push((dev.index, title));
            }
            if device_targets.is_empty() {
                for dev in devices.iter() {
                    if !dev.removable || dev.logical_partition {
                        continue;
                    }
                    let title = alloc::format!("USB {} ({} MiB)", dev.index, dev.total_mib);
                    device_targets.push((dev.index, title));
                }
            }

            // Build the tree with a temporary FAT mount so opening the prompt never
            // changes the globally mounted volume used by Desktop/Explorer.
            if device_targets.is_empty() {
                let mut mounted_with_temp = false;
                if let Some(dev_idx) = current_device_index {
                    let mut temp_fat = crate::fat32::Fat32::new();
                    if temp_fat.mount_uefi_block_device(dev_idx).is_ok() {
                        let root_cluster = temp_fat.root_cluster;
                        let volume_label =
                            Self::volume_label_text(&temp_fat).unwrap_or(String::from("USB"));
                        let root_path = alloc::format!("{}/", volume_label);
                        let has_children =
                            Self::notepad_location_has_subdirs(&mut temp_fat, root_cluster);
                        let unit_idx = Self::push_notepad_save_location(
                            &mut locations,
                            Some(dev_idx),
                            root_cluster,
                            root_path.as_str(),
                            root_path.as_str(),
                            true,
                            None,
                            0,
                            true,
                            has_children,
                        );
                        if let Some(root_idx) = unit_idx {
                            if has_children {
                                Self::append_notepad_tree_children(
                                    &mut temp_fat,
                                    &mut locations,
                                    Some(dev_idx),
                                    root_idx,
                                    root_cluster,
                                    root_path.as_str(),
                                    1,
                                );
                            }
                        }
                        mounted_with_temp = true;
                    }
                }

                if !mounted_with_temp {
                    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    let root_cluster = fat.root_cluster;
                    let volume_label = Self::volume_label_text(fat).unwrap_or(String::from("USB"));
                    let root_path = alloc::format!("{}/", volume_label);
                    let has_children = Self::notepad_location_has_subdirs(fat, root_cluster);
                    let unit_idx = Self::push_notepad_save_location(
                        &mut locations,
                        current_device_index,
                        root_cluster,
                        root_path.as_str(),
                        root_path.as_str(),
                        true,
                        None,
                        0,
                        true,
                        has_children,
                    );
                    if let Some(root_idx) = unit_idx {
                        if has_children {
                            Self::append_notepad_tree_children(
                                fat,
                                &mut locations,
                                current_device_index,
                                root_idx,
                                root_cluster,
                                root_path.as_str(),
                                1,
                            );
                        }
                    }
                }
            } else {
                let mut temp_fat = crate::fat32::Fat32::new();
                for (dev_index, title) in device_targets.iter() {
                    let mut unit_cluster = 0u32;
                    let mut unit_path = alloc::format!("{}/", title);
                    let mut has_children = false;
                    let mut mounted_ok = false;

                    if temp_fat.mount_uefi_block_device(*dev_index).is_ok() {
                        mounted_ok = true;
                        unit_cluster = temp_fat.root_cluster;
                        let volume_label = Self::volume_label_text(&temp_fat)
                            .unwrap_or(alloc::format!("VOL{}", dev_index));
                        unit_path = alloc::format!("{}/", volume_label);
                        has_children = Self::notepad_location_has_subdirs(&mut temp_fat, unit_cluster);
                    }

                    let unit_idx = Self::push_notepad_save_location(
                        &mut locations,
                        Some(*dev_index),
                        unit_cluster,
                        unit_path.as_str(),
                        title.as_str(),
                        true,
                        None,
                        0,
                        false,
                        has_children,
                    );

                    if mounted_ok && has_children {
                        if let Some(root_idx) = unit_idx {
                            Self::append_notepad_tree_children(
                                &mut temp_fat,
                                &mut locations,
                                Some(*dev_index),
                                root_idx,
                                unit_cluster,
                                unit_path.as_str(),
                                1,
                            );
                        }
                    }
                }
            }

            let mut selected_index = 0usize;
            if current_cluster >= 2 {
                if let Some(idx) = locations.iter().position(|loc| {
                    loc.device_index == current_device_index && loc.cluster == current_cluster
                }) {
                    selected_index = idx;
                } else {
                    let current_norm = Self::ascii_lower(current_path.trim().trim_end_matches('/'));
                    if let Some(idx) = locations.iter().position(|loc| {
                        loc.device_index == current_device_index
                            && Self::ascii_lower(loc.path.trim().trim_end_matches('/')) == current_norm
                    }) {
                        selected_index = idx;
                    }
                }
            }

            (locations, selected_index)
        };

        if locations.is_empty() {
            locations.push(NotepadSaveLocation {
                device_index: None,
                cluster: 0,
                path: String::from("Sin unidades/"),
                label: String::from("Sin unidades detectadas"),
                is_unit: true,
                depth: 0,
                parent: None,
                expanded: false,
                has_children: false,
            });
            selected_index = 0;
        }
        if selected_index >= locations.len() {
            selected_index = 0;
        }

        let has_real_target = locations.iter().any(|loc| loc.device_index.is_some() || loc.cluster >= 2);
        let visible = self.notepad_save_prompt_visible_item_count();
        self.notepad_save_prompt = Some(NotepadSavePromptState {
            win_id,
            locations,
            selected_index,
            scroll_top: 0,
        });
        if let Some(prompt) = self.notepad_save_prompt.as_mut() {
            Self::notepad_prompt_expand_ancestors(prompt, prompt.selected_index);
            Self::ensure_notepad_prompt_scroll(prompt, visible);
        }
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            if has_real_target {
                win.set_notepad_status("Guardar nota: selecciona ubicacion (arbol) y presiona OK.");
            } else {
                win.set_notepad_status("No hay unidades detectadas. Conecta o monta una unidad para guardar.");
            }
        }
    }

    fn cancel_notepad_save_prompt(&mut self) {
        let Some(prompt) = self.notepad_save_prompt.clone() else {
            return;
        };
        self.notepad_save_prompt = None;
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == prompt.win_id) {
            win.set_notepad_status("Guardado cancelado.");
        }
    }

    fn commit_notepad_save_prompt(&mut self) {
        let Some(prompt) = self.notepad_save_prompt.clone() else {
            return;
        };

        let Some(target_location) = prompt.locations.get(prompt.selected_index).cloned() else {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == prompt.win_id) {
                win.set_notepad_status("Selecciona una ubicacion valida.");
            }
            return;
        };

        let (file_name, text) = match self.windows.iter().find(|w| w.id == prompt.win_id) {
            Some(win) if win.is_notepad() => (win.notepad_file_name.clone(), win.notepad_text.clone()),
            _ => {
                self.notepad_save_prompt = None;
                return;
            }
        };

        let trimmed_name = file_name.trim();
        if trimmed_name.is_empty() {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == prompt.win_id) {
                win.set_notepad_status("Filename is empty.");
            }
            return;
        }

        if trimmed_name
            .bytes()
            .any(|b| matches!(b, b'/' | b'\\' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|'))
        {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == prompt.win_id) {
                win.set_notepad_status("Nombre de archivo invalido.");
            }
            return;
        }

        if text.len() > NOTEPAD_MAX_TEXT_BYTES {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == prompt.win_id) {
                win.set_notepad_status(
                    alloc::format!(
                        "Text too large (max {} bytes). Shorten content.",
                        NOTEPAD_MAX_TEXT_BYTES
                    )
                    .as_str(),
                );
            }
            return;
        }

        if !self.ensure_fat_ready_for_notepad(prompt.win_id) {
            return;
        }

        let selected_device_index = target_location.device_index;
        let mut switched_device = false;
        let save_result = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let mut target_cluster = target_location.cluster;
            let mut target_path = target_location.path.clone();
            let mut mount_error: Option<String> = None;

            if let Some(dev_idx) = selected_device_index {
                if self.current_volume_device_index != Some(dev_idx) {
                    match fat.mount_uefi_block_device(dev_idx) {
                        Ok(_mounted) => {
                            switched_device = true;
                        }
                        Err(e) => {
                            mount_error = Some(alloc::format!(
                                "No se pudo montar unidad {}: {}",
                                dev_idx, e
                            ));
                        }
                    }
                }

                if mount_error.is_none() && target_cluster < 2 {
                    target_cluster = fat.root_cluster;
                    let label = Self::volume_label_text(fat)
                        .unwrap_or(alloc::format!("VOL{}", dev_idx));
                    target_path = alloc::format!("{}/", label);
                }
            } else if target_cluster < 2 {
                target_cluster = fat.root_cluster;
                let label = Self::volume_label_text(fat).unwrap_or(String::from("USB"));
                target_path = alloc::format!("{}/", label);
            }

            if let Some(err) = mount_error {
                Err(err)
            } else {
                match fat.write_text_file_in_dir(target_cluster, trimmed_name, text.as_bytes()) {
                    Ok(()) => Ok((target_cluster, target_path)),
                    Err(e) => Err(alloc::format!("Save failed: {}", e)),
                }
            }
        };

        if switched_device {
            if let Some(dev_idx) = selected_device_index {
                self.current_volume_device_index = Some(dev_idx);
                self.clear_manual_unmount_lock();
            }
        }

        match save_result {
            Ok((dir_cluster, dir_path)) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == prompt.win_id) {
                    win.notepad_dir_cluster = dir_cluster;
                    win.notepad_dir_path = dir_path.clone();
                    win.set_notepad_status(
                        alloc::format!("File saved in {}.", dir_path).as_str(),
                    );
                }
                self.notepad_save_prompt = None;
                self.refresh_explorer_windows_for_cluster(
                    dir_cluster,
                    "Archivo guardado desde Notepad.",
                    None,
                );
            }
            Err(msg) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == prompt.win_id) {
                    win.set_notepad_status(msg.as_str());
                }
            }
        }
    }

    fn handle_notepad_save_prompt_key(
        &mut self,
        key: Option<char>,
        special: Option<SpecialKey>,
        down: bool,
    ) -> bool {
        if self.notepad_save_prompt.is_none() {
            return false;
        }
        if !down {
            return true;
        }

        match key {
            Some('\n') | Some('\r') => {
                self.commit_notepad_save_prompt();
                return true;
            }
            Some('\x1b') => {
                self.cancel_notepad_save_prompt();
                return true;
            }
            _ => {}
        }

        let move_up = matches!(special, Some(SpecialKey::Up)) || matches!(key, Some('w') | Some('W'));
        let move_down =
            matches!(special, Some(SpecialKey::Down)) || matches!(key, Some('s') | Some('S'));
        let collapse =
            matches!(special, Some(SpecialKey::Left)) || matches!(key, Some('a') | Some('A'));
        let expand =
            matches!(special, Some(SpecialKey::Right)) || matches!(key, Some('d') | Some('D'));

        if move_up {
            let visible = self.notepad_save_prompt_visible_item_count();
            if let Some(prompt) = self.notepad_save_prompt.as_mut() {
                let visible_indices = Self::notepad_prompt_visible_indices(prompt);
                if let Some(pos) = visible_indices
                    .iter()
                    .position(|idx| *idx == prompt.selected_index)
                {
                    if pos > 0 {
                        prompt.selected_index = visible_indices[pos - 1];
                    }
                } else if !visible_indices.is_empty() {
                    prompt.selected_index = visible_indices[0];
                }
                Self::ensure_notepad_prompt_scroll(prompt, visible);
            }
            return true;
        }

        if move_down {
            let visible = self.notepad_save_prompt_visible_item_count();
            if let Some(prompt) = self.notepad_save_prompt.as_mut() {
                let visible_indices = Self::notepad_prompt_visible_indices(prompt);
                if let Some(pos) = visible_indices
                    .iter()
                    .position(|idx| *idx == prompt.selected_index)
                {
                    if pos + 1 < visible_indices.len() {
                        prompt.selected_index = visible_indices[pos + 1];
                    }
                } else if !visible_indices.is_empty() {
                    prompt.selected_index = visible_indices[0];
                }
                Self::ensure_notepad_prompt_scroll(prompt, visible);
            }
            return true;
        }

        if collapse {
            let visible = self.notepad_save_prompt_visible_item_count();
            if let Some(prompt) = self.notepad_save_prompt.as_mut() {
                let mut focus_parent: Option<usize> = None;
                let selected = prompt.selected_index;
                if let Some(node) = prompt.locations.get_mut(selected) {
                    if node.has_children && node.expanded {
                        node.expanded = false;
                    } else {
                        focus_parent = node.parent;
                    }
                }
                if let Some(parent) = focus_parent {
                    prompt.selected_index = parent;
                }
                Self::ensure_notepad_prompt_scroll(prompt, visible);
            }
            return true;
        }

        if expand {
            let visible = self.notepad_save_prompt_visible_item_count();
            if let Some(prompt) = self.notepad_save_prompt.as_mut() {
                let selected = prompt.selected_index;
                if let Some(node) = prompt.locations.get_mut(selected) {
                    if node.has_children {
                        node.expanded = true;
                    }
                }
                Self::ensure_notepad_prompt_scroll(prompt, visible);
            }
            return true;
        }

        true
    }

    fn handle_notepad_save_prompt_wheel(
        &mut self,
        mouse_x: i32,
        mouse_y: i32,
        wheel_delta: i32,
    ) -> bool {
        if self.notepad_save_prompt.is_none() || wheel_delta == 0 {
            return false;
        }

        let list_rect = self.notepad_save_prompt_list_rect();
        if !list_rect.contains(Point { x: mouse_x, y: mouse_y }) {
            return false;
        }

        let visible = self.notepad_save_prompt_visible_item_count();
        if let Some(prompt) = self.notepad_save_prompt.as_mut() {
            let visible_indices = Self::notepad_prompt_visible_indices(prompt);
            if visible_indices.is_empty() || visible == 0 {
                prompt.scroll_top = 0;
                return true;
            }

            let max_scroll = visible_indices.len().saturating_sub(visible);
            if max_scroll == 0 {
                prompt.scroll_top = 0;
                return true;
            }

            let magnitude = if wheel_delta < 0 {
                wheel_delta.saturating_neg() as usize
            } else {
                wheel_delta as usize
            };
            let mut rows = magnitude / 120;
            if rows == 0 {
                rows = magnitude.max(1);
            }
            rows = rows.min(visible.max(1));

            if wheel_delta > 0 {
                prompt.scroll_top = prompt.scroll_top.saturating_sub(rows);
            } else {
                prompt.scroll_top = prompt.scroll_top.saturating_add(rows).min(max_scroll);
            }
            return true;
        }

        false
    }

    fn handle_notepad_save_prompt_click(&mut self, mouse_x: i32, mouse_y: i32) -> bool {
        if self.notepad_save_prompt.is_none() {
            return false;
        }

        let p = Point {
            x: mouse_x,
            y: mouse_y,
        };

        let list_rect = self.notepad_save_prompt_list_rect();
        if list_rect.contains(p) {
            let visible = self.notepad_save_prompt_visible_item_count();
            let rel_y = (p.y - list_rect.y - 2).max(0) as u32;
            let row = (rel_y / NOTEPAD_SAVE_PROMPT_ITEM_H) as usize;
            let item_rect = self.notepad_save_prompt_item_rect(row);
            if let Some(prompt) = self.notepad_save_prompt.as_mut() {
                let visible_indices = Self::notepad_prompt_visible_indices(prompt);
                let visible_pos = prompt.scroll_top.saturating_add(row);
                if visible_pos < visible_indices.len() && row < visible {
                    let idx = visible_indices[visible_pos];
                    prompt.selected_index = idx;
                    if let Some(node) = prompt.locations.get(idx) {
                        let indent = (node.depth as i32 * 12)
                            .clamp(0, item_rect.width.saturating_sub(20) as i32);
                        let toggle_rect = Rect::new(item_rect.x + 4 + indent, item_rect.y + 6, 8, 8);
                        if toggle_rect.contains(p) && node.has_children {
                            if let Some(node_mut) = prompt.locations.get_mut(idx) {
                                node_mut.expanded = !node_mut.expanded;
                            }
                        }
                    }
                    Self::ensure_notepad_prompt_scroll(prompt, visible);
                }
            }
            return true;
        }
        if self.notepad_save_prompt_ok_rect().contains(p) {
            self.commit_notepad_save_prompt();
            return true;
        }
        if self.notepad_save_prompt_cancel_rect().contains(p) {
            self.cancel_notepad_save_prompt();
            return true;
        }

        true
    }

    fn draw_notepad_save_prompt(&mut self) {
        let Some(prompt) = self.notepad_save_prompt.as_ref() else {
            return;
        };
        let rect = self.notepad_save_prompt_rect();

        framebuffer::rect(
            rect.x.max(0) as usize,
            rect.y.max(0) as usize,
            rect.width as usize,
            rect.height as usize,
            0x10283A,
        );
        framebuffer::rect(
            rect.x.max(0) as usize,
            rect.y.max(0) as usize,
            rect.width as usize,
            1,
            0x7BB9E3,
        );
        framebuffer::rect(
            rect.x.max(0) as usize,
            (rect.y + rect.height as i32 - 1).max(0) as usize,
            rect.width as usize,
            1,
            0x091521,
        );

        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + 12).max(0) as usize,
            "Guardar nota",
            0xEAF6FF,
        );
        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + 28).max(0) as usize,
            "Arbol de unidades y carpetas:",
            0xA6C7E1,
        );

        let list_rect = self.notepad_save_prompt_list_rect();
        framebuffer::rect(
            list_rect.x.max(0) as usize,
            list_rect.y.max(0) as usize,
            list_rect.width as usize,
            list_rect.height as usize,
            0x0A1C2B,
        );
        framebuffer::rect(
            list_rect.x.max(0) as usize,
            list_rect.y.max(0) as usize,
            list_rect.width as usize,
            1,
            0x4F7A9D,
        );
        framebuffer::rect(
            list_rect.x.max(0) as usize,
            (list_rect.y + list_rect.height as i32 - 1).max(0) as usize,
            list_rect.width as usize,
            1,
            0x0A1520,
        );

        let visible = self.notepad_save_prompt_visible_item_count();
        let visible_indices = Self::notepad_prompt_visible_indices(prompt);
        for row in 0..visible {
            let visible_pos = prompt.scroll_top.saturating_add(row);
            let Some(idx) = visible_indices.get(visible_pos).copied() else {
                break;
            };
            let Some(loc) = prompt.locations.get(idx) else {
                break;
            };
            let item_rect = self.notepad_save_prompt_item_rect(row);
            let selected = idx == prompt.selected_index;
            framebuffer::rect(
                item_rect.x.max(0) as usize,
                item_rect.y.max(0) as usize,
                item_rect.width as usize,
                item_rect.height as usize,
                if selected { 0x1A4F78 } else { 0x10273A },
            );
            framebuffer::rect(
                item_rect.x.max(0) as usize,
                item_rect.y.max(0) as usize,
                item_rect.width as usize,
                1,
                if selected { 0x8CC8F0 } else { 0x27445B },
            );

            let toggle_rect = self.notepad_save_prompt_toggle_rect(item_rect, loc.depth);
            if loc.has_children {
                framebuffer::rect(
                    toggle_rect.x.max(0) as usize,
                    toggle_rect.y.max(0) as usize,
                    toggle_rect.width as usize,
                    toggle_rect.height as usize,
                    if selected { 0x2A628C } else { 0x1A3347 },
                );
                framebuffer::rect(
                    (toggle_rect.x + 1).max(0) as usize,
                    (toggle_rect.y + 3).max(0) as usize,
                    6,
                    1,
                    0xD7ECFF,
                );
                if !loc.expanded {
                    framebuffer::rect(
                        (toggle_rect.x + 3).max(0) as usize,
                        (toggle_rect.y + 1).max(0) as usize,
                        1,
                        6,
                        0xD7ECFF,
                    );
                }
            }

            let icon_x = if loc.has_children {
                toggle_rect.x + toggle_rect.width as i32 + 4
            } else {
                toggle_rect.x + 2
            };
            if loc.is_unit {
                framebuffer::rect(
                    icon_x.max(0) as usize,
                    (item_rect.y + 5).max(0) as usize,
                    12,
                    8,
                    0xB4C5D6,
                );
                framebuffer::rect(
                    (icon_x + 2).max(0) as usize,
                    (item_rect.y + 7).max(0) as usize,
                    8,
                    3,
                    0xE6EEF6,
                );
            } else {
                framebuffer::rect(
                    icon_x.max(0) as usize,
                    (item_rect.y + 7).max(0) as usize,
                    12,
                    8,
                    0xE8C56B,
                );
                framebuffer::rect(
                    (icon_x + 2).max(0) as usize,
                    (item_rect.y + 5).max(0) as usize,
                    5,
                    3,
                    0xF2D78A,
                );
            }

            let item_text = loc.label.as_str();
            framebuffer::draw_text_5x7(
                (icon_x + 16).max(0) as usize,
                (item_rect.y + 7).max(0) as usize,
                Self::trim_ascii_line(item_text, 40).as_str(),
                if selected { 0xF2FAFF } else { 0xD3E8F8 },
            );
        }

        let selected_visible_row = visible_indices
            .iter()
            .position(|idx| *idx == prompt.selected_index)
            .map(|v| v + 1)
            .unwrap_or(0);
        let count_text = alloc::format!(
            "{}/{}",
            selected_visible_row,
            visible_indices.len()
        );
        framebuffer::draw_text_5x7(
            (rect.x + rect.width as i32 - 56).max(0) as usize,
            (rect.y + 28).max(0) as usize,
            count_text.as_str(),
            0xA6C7E1,
        );

        if let Some(selected) = prompt.locations.get(prompt.selected_index) {
            let destination_text = Self::trim_ascii_line(
                alloc::format!("Destino: {}", selected.path).as_str(),
                56,
            );
            framebuffer::draw_text_5x7(
                (rect.x + 10).max(0) as usize,
                (rect.y + rect.height as i32 - 58).max(0) as usize,
                destination_text.as_str(),
                0xA9C6E1,
            );
        }

        framebuffer::draw_text_5x7(
            (rect.x + 10).max(0) as usize,
            (rect.y + rect.height as i32 - 44).max(0) as usize,
            "Click: seleccionar/expandir  A/D: cerrar/abrir  Enter: guardar",
            0xA9C6E1,
        );

        let ok_rect = self.notepad_save_prompt_ok_rect();
        framebuffer::rect(
            ok_rect.x.max(0) as usize,
            ok_rect.y.max(0) as usize,
            ok_rect.width as usize,
            ok_rect.height as usize,
            0x2B5D45,
        );
        framebuffer::rect(
            ok_rect.x.max(0) as usize,
            ok_rect.y.max(0) as usize,
            ok_rect.width as usize,
            1,
            0x7CC2A0,
        );
        framebuffer::draw_text_5x7(
            (ok_rect.x + 26).max(0) as usize,
            (ok_rect.y + 8).max(0) as usize,
            "OK",
            0xF1FFF7,
        );

        let cancel_rect = self.notepad_save_prompt_cancel_rect();
        framebuffer::rect(
            cancel_rect.x.max(0) as usize,
            cancel_rect.y.max(0) as usize,
            cancel_rect.width as usize,
            cancel_rect.height as usize,
            0x4A3441,
        );
        framebuffer::rect(
            cancel_rect.x.max(0) as usize,
            cancel_rect.y.max(0) as usize,
            cancel_rect.width as usize,
            1,
            0x9F6D84,
        );
        framebuffer::draw_text_5x7(
            (cancel_rect.x + 11).max(0) as usize,
            (cancel_rect.y + 8).max(0) as usize,
            "Cancel",
            0xFFEAF3,
        );
    }

    fn set_desktop_item_custom_position_by_key(
        &mut self,
        cluster: u32,
        label: &str,
        x: i32,
        y: i32,
    ) {
        let min_x = 0;
        let min_y = DESKTOP_ITEMS_START_Y.max(0);
        let max_x = (self.width as i32 - DESKTOP_ITEM_W as i32).max(min_x);
        let max_y = (self.taskbar.rect.y - DESKTOP_ITEM_H as i32 - 2).max(min_y);
        let clamped_x = x.clamp(min_x, max_x);
        let clamped_y = y.clamp(min_y, max_y);

        if let Some(entry) = self
            .desktop_icon_positions
            .iter_mut()
            .find(|entry| entry.cluster == cluster && entry.label.eq_ignore_ascii_case(label))
        {
            entry.x = clamped_x;
            entry.y = clamped_y;
            return;
        }

        self.desktop_icon_positions.push(DesktopIconPosition {
            cluster,
            label: String::from(label),
            x: clamped_x,
            y: clamped_y,
        });
    }

    fn update_desktop_drag(&mut self, mouse_x: i32, mouse_y: i32, left_down: bool) -> bool {
        let Some(mut drag) = self.desktop_drag.clone() else {
            return false;
        };

        if !left_down {
            self.desktop_drag = None;
            if drag.open_on_release && !drag.moved {
                let item = drag.item.clone();
                self.open_desktop_item(drag.source_dir_cluster, drag.source_dir_path.clone(), &item);
            }
            return true;
        }

        if !drag.moved {
            let moved_x = (mouse_x - drag.start_mouse_x).abs();
            let moved_y = (mouse_y - drag.start_mouse_y).abs();
            if moved_x >= DESKTOP_DRAG_OPEN_THRESHOLD || moved_y >= DESKTOP_DRAG_OPEN_THRESHOLD {
                drag.moved = true;
            }
        }

        let new_x = mouse_x - drag.offset_x;
        let new_y = mouse_y - drag.offset_y;
        self.set_desktop_item_custom_position_by_key(drag.cluster, drag.label.as_str(), new_x, new_y);
        self.desktop_drag = Some(drag);
        true
    }

    fn explorer_item_is_zip(item: &ExplorerItem) -> bool {
        if item.kind != ExplorerItemKind::File {
            return false;
        }
        Self::ascii_lower(item.label.trim()).ends_with(".zip")
    }

    fn explorer_item_is_deb(item: &ExplorerItem) -> bool {
        if item.kind != ExplorerItemKind::File {
            return false;
        }
        Self::ascii_lower(item.label.trim()).ends_with(".deb")
    }

    fn launch_install_from_context(
        &mut self,
        source_dir_cluster: u32,
        source_dir_path: Option<&str>,
        package_name: &str,
    ) {
        let package = package_name.trim();
        if package.is_empty() {
            return;
        }

        let term_id = self
            .windows
            .iter()
            .find(|w| w.is_terminal())
            .map(|w| w.id)
            .unwrap_or_else(|| self.create_window("Terminal Shell", 100, 100, 800, 500));

        if let Some(term) = self.windows.iter_mut().find(|w| w.id == term_id) {
            if source_dir_cluster >= 2 {
                term.current_dir_cluster = source_dir_cluster;
            }

            let mut path_text = source_dir_path
                .map(|v| String::from(v.trim()))
                .unwrap_or_else(|| String::from("REDUX/"));
            if path_text.is_empty() {
                path_text = String::from("REDUX/");
            }
            if !path_text.ends_with('/') {
                path_text.push('/');
            }
            term.current_path = path_text;
            term.add_output(alloc::format!("GUI: install {}", package).as_str());
            term.render_terminal();
        }

        self.execute_command(term_id, alloc::format!("install {}", package).as_str());
    }

    fn explorer_directory_can_delete(source_dir_cluster: u32, item: &ExplorerItem) -> bool {
        if item.kind != ExplorerItemKind::Directory {
            return false;
        }

        let root_cluster = unsafe { crate::fat32::GLOBAL_FAT.root_cluster };
        if source_dir_cluster == root_cluster
            && Self::is_quick_access_shortcut_name(item.label.as_str())
        {
            return false;
        }

        true
    }

    fn explorer_context_item_count_for_kind(
        kind: ExplorerContextMenuKind,
        target_item: Option<&ExplorerItem>,
        source_dir_cluster: u32,
        selection_count: usize,
    ) -> usize {
        let is_trash_dir = {
            let mut fat = unsafe { crate::fat32::Fat32::new() };
            fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0) == source_dir_cluster && source_dir_cluster >= 2
        };

        if is_trash_dir {
            return 1; // Restaurar
        }

        match kind {
            ExplorerContextMenuKind::FileItem => {
                let mut count = 4; // Copiar, Cortar, Renombrar, Eliminar
                if let Some(item) = target_item {
                    if selection_count <= 1 {
                        if Self::explorer_item_is_zip(item) {
                            count += 1; // Extraer aqui
                        }
                        if Self::explorer_item_is_deb(item) {
                            count += 1; // Instalar .deb
                        }
                    }
                }
                count
            }
            ExplorerContextMenuKind::DirectoryItem => {
                let mut count = 3; // Copiar carpeta, Cortar carpeta, Renombrar carpeta
                if let Some(item) = target_item {
                    if Self::explorer_directory_can_delete(source_dir_cluster, item) {
                        count += 1; // Eliminar carpeta
                    }
                }
                count
            }
            ExplorerContextMenuKind::PasteArea => 1,
            ExplorerContextMenuKind::DesktopArea => 1,
        }
    }

    fn explorer_context_menu_item_count(&self, menu: &ExplorerContextMenuState) -> usize {
        if menu.kind == ExplorerContextMenuKind::DesktopArea {
            return if menu.show_paste { 3 } else { 2 };
        }
        if menu.kind == ExplorerContextMenuKind::PasteArea {
            return if menu.show_paste { 3 } else { 2 };
        }
        Self::explorer_context_item_count_for_kind(
            menu.kind,
            menu.target_item.as_ref(),
            menu.source_dir_cluster,
            menu.selection_count,
        )
    }

    fn explorer_context_menu_height(item_count: usize) -> u32 {
        (item_count as u32).saturating_mul(EXPLORER_CONTEXT_MENU_ITEM_H)
            + (EXPLORER_CONTEXT_MENU_PADDING as u32).saturating_mul(2)
    }

    fn clamp_explorer_context_menu_origin(&self, x: i32, y: i32, item_count: usize) -> (i32, i32) {
        let menu_h = Self::explorer_context_menu_height(item_count) as i32;
        let max_x = (self.width as i32 - EXPLORER_CONTEXT_MENU_W as i32).max(0);
        let max_y = (self.taskbar.rect.y - menu_h).max(0);
        (x.clamp(0, max_x), y.clamp(0, max_y))
    }

    fn explorer_context_menu_rect(&self, menu: &ExplorerContextMenuState) -> Rect {
        let item_count = self.explorer_context_menu_item_count(menu);
        Rect::new(
            menu.x,
            menu.y,
            EXPLORER_CONTEXT_MENU_W,
            Self::explorer_context_menu_height(item_count),
        )
    }

    fn explorer_context_menu_item_rect(&self, menu: &ExplorerContextMenuState, index: usize) -> Rect {
        let menu_rect = self.explorer_context_menu_rect(menu);
        Rect::new(
            menu_rect.x + EXPLORER_CONTEXT_MENU_PADDING,
            menu_rect.y
                + EXPLORER_CONTEXT_MENU_PADDING
                + (index as i32 * EXPLORER_CONTEXT_MENU_ITEM_H as i32),
            menu_rect
                .width
                .saturating_sub((EXPLORER_CONTEXT_MENU_PADDING as u32).saturating_mul(2)),
            EXPLORER_CONTEXT_MENU_ITEM_H,
        )
    }

    fn draw_explorer_context_menu_overlay(&mut self) {
        let Some(menu) = self.explorer_context_menu.as_ref() else {
            return;
        };

        let menu_rect = self.explorer_context_menu_rect(menu);
        framebuffer::rect(
            menu_rect.x.max(0) as usize,
            menu_rect.y.max(0) as usize,
            menu_rect.width as usize,
            menu_rect.height as usize,
            0x1D2D3D,
        );
        framebuffer::rect(
            menu_rect.x.max(0) as usize,
            menu_rect.y.max(0) as usize,
            menu_rect.width as usize,
            1,
            0x5F85A8,
        );
        framebuffer::rect(
            menu_rect.x.max(0) as usize,
            (menu_rect.y + menu_rect.height as i32 - 1).max(0) as usize,
            menu_rect.width as usize,
            1,
            0x0F1A27,
        );

        let item_count = self.explorer_context_menu_item_count(menu);
        for idx in 0..item_count {
            let item_rect = self.explorer_context_menu_item_rect(menu, idx);
            let mut label = "";
            let mut bg = 0x294259u32;
            let mut fg = 0xEAF6FFu32;

            match menu.kind {
                ExplorerContextMenuKind::FileItem => {
                    let is_zip = menu
                        .target_item
                        .as_ref()
                        .map(Self::explorer_item_is_zip)
                        .unwrap_or(false);
                    let is_deb = menu
                        .target_item
                        .as_ref()
                        .map(Self::explorer_item_is_deb)
                        .unwrap_or(false);
                    if idx == 0 {
                        label = "Copiar";
                        bg = 0x2B5D45;
                    } else if idx == 1 {
                        label = "Cortar";
                        bg = 0x6A3A3A;
                    } else if idx == 2 {
                        label = "Renombrar";
                        bg = 0x36596F;
                    } else if idx == 3 {
                        label = "Eliminar";
                        bg = 0x6A2D2D;
                        fg = 0xFFE5E5;
                    } else if menu.selection_count <= 1 {
                        let mut extra_idx = 4usize;
                        if is_zip {
                            if idx == extra_idx {
                                label = "Extraer aqui";
                                bg = 0x36596F;
                            }
                            extra_idx += 1;
                        }
                        if is_deb && idx == extra_idx {
                            label = "Instalar .deb";
                            bg = 0x3F5A2C;
                            fg = 0xF1FFE6;
                        }
                    }
                }
                ExplorerContextMenuKind::DirectoryItem => {
                    let is_trash = menu.target_item.as_ref().map(|i| i.kind == ExplorerItemKind::ShortcutRecycleBin).unwrap_or(false);
                    if is_trash {
                        if idx == 0 {
                            label = "Vaciar Papelera";
                            bg = 0x6A2D2D;
                            fg = 0xFFE5E5;
                        }
                    } else {
                        let can_delete = menu
                            .target_item
                            .as_ref()
                            .map(|item| Self::explorer_directory_can_delete(menu.source_dir_cluster, item))
                            .unwrap_or(false);
                        if idx == 0 {
                            label = "Copiar carpeta";
                            bg = 0x2B5D45;
                        } else if idx == 1 {
                            label = "Cortar carpeta";
                            bg = 0x6A3A3A;
                        } else if idx == 2 {
                            label = "Renombrar carpeta";
                            bg = 0x36596F;
                        } else if idx == 3 && can_delete {
                            label = "Eliminar carpeta";
                            bg = 0x6A2D2D;
                            fg = 0xFFE5E5;
                        }
                    }
                }
                ExplorerContextMenuKind::PasteArea => {
                    if idx == 0 {
                        label = "Crear carpeta";
                        bg = 0x2D4A5F;
                    } else if idx == 1 {
                        label = "Crear nota";
                        bg = 0x3A4D66;
                    } else if idx == 2 && menu.show_paste {
                        if let Some(clip) = self.explorer_clipboard.as_ref() {
                            if clip.mode == ExplorerClipboardMode::Cut {
                                label = "Pegar (mover)";
                                bg = 0x5A4630;
                                fg = 0xFFF2DE;
                            } else {
                                label = "Pegar";
                                bg = 0x2F566F;
                            }
                        } else {
                            label = "Pegar";
                            bg = 0x2B3440;
                            fg = 0x9FB2C4;
                        }
                    }
                }
                ExplorerContextMenuKind::DesktopArea => {
                    if idx == 0 {
                        label = "Crear carpeta";
                        bg = 0x2D4A5F;
                    } else if idx == 1 {
                        label = "Crear nota";
                        bg = 0x3A4D66;
                    } else if idx == 2 && menu.show_paste {
                        if let Some(clip) = self.explorer_clipboard.as_ref() {
                            if clip.mode == ExplorerClipboardMode::Cut {
                                label = "Pegar (mover)";
                                bg = 0x5A4630;
                                fg = 0xFFF2DE;
                            } else {
                                label = "Pegar";
                                bg = 0x2F566F;
                            }
                        } else {
                            label = "Pegar";
                            bg = 0x2B3440;
                            fg = 0x9FB2C4;
                        }
                    }
                }
            }

            framebuffer::rect(
                item_rect.x.max(0) as usize,
                item_rect.y.max(0) as usize,
                item_rect.width as usize,
                item_rect.height as usize,
                bg,
            );
            framebuffer::draw_text_5x7(
                (item_rect.x + 8).max(0) as usize,
                (item_rect.y + 8).max(0) as usize,
                label,
                fg,
            );
        }
    }

    fn draw_desktop_context_menu_overlay(&mut self) {
        let Some(menu) = self.desktop_context_menu.as_ref() else {
            return;
        };

        let menu_rect = self.explorer_context_menu_rect(menu);
        framebuffer::rect(
            menu_rect.x.max(0) as usize,
            menu_rect.y.max(0) as usize,
            menu_rect.width as usize,
            menu_rect.height as usize,
            0x1D2D3D,
        );
        framebuffer::rect(
            menu_rect.x.max(0) as usize,
            menu_rect.y.max(0) as usize,
            menu_rect.width as usize,
            1,
            0x5F85A8,
        );
        framebuffer::rect(
            menu_rect.x.max(0) as usize,
            (menu_rect.y + menu_rect.height as i32 - 1).max(0) as usize,
            menu_rect.width as usize,
            1,
            0x0F1A27,
        );

        let item_count = self.explorer_context_menu_item_count(menu);
        for idx in 0..item_count {
            let item_rect = self.explorer_context_menu_item_rect(menu, idx);
            let mut label = "";
            let mut bg = 0x294259u32;
            let mut fg = 0xEAF6FFu32;

            match menu.kind {
                ExplorerContextMenuKind::FileItem => {
                    let is_zip = menu
                        .target_item
                        .as_ref()
                        .map(Self::explorer_item_is_zip)
                        .unwrap_or(false);
                    let is_deb = menu
                        .target_item
                        .as_ref()
                        .map(Self::explorer_item_is_deb)
                        .unwrap_or(false);
                    if idx == 0 {
                        label = "Copiar";
                        bg = 0x2B5D45;
                    } else if idx == 1 {
                        label = "Cortar";
                        bg = 0x6A3A3A;
                    } else if idx == 2 {
                        label = "Renombrar";
                        bg = 0x36596F;
                    } else if idx == 3 {
                        label = "Eliminar";
                        bg = 0x6A2D2D;
                        fg = 0xFFE5E5;
                    } else if menu.selection_count <= 1 {
                        let mut extra_idx = 4usize;
                        if is_zip {
                            if idx == extra_idx {
                                label = "Extraer aqui";
                                bg = 0x36596F;
                            }
                            extra_idx += 1;
                        }
                        if is_deb && idx == extra_idx {
                            label = "Instalar .deb";
                            bg = 0x3F5A2C;
                            fg = 0xF1FFE6;
                        }
                    }
                }
                ExplorerContextMenuKind::DirectoryItem => {
                    let is_trash = menu.target_item.as_ref().map(|i| i.kind == ExplorerItemKind::ShortcutRecycleBin).unwrap_or(false);
                    if is_trash {
                        if idx == 0 {
                            label = "Vaciar Papelera";
                            bg = 0x6A2D2D;
                            fg = 0xFFE5E5;
                        }
                    } else {
                        let can_delete = menu
                            .target_item
                            .as_ref()
                            .map(|item| Self::explorer_directory_can_delete(menu.source_dir_cluster, item))
                            .unwrap_or(false);
                        if idx == 0 {
                            label = "Copiar carpeta";
                            bg = 0x2B5D45;
                        } else if idx == 1 {
                            label = "Cortar carpeta";
                            bg = 0x6A3A3A;
                        } else if idx == 2 {
                            label = "Renombrar carpeta";
                            bg = 0x36596F;
                        } else if idx == 3 && can_delete {
                            label = "Eliminar carpeta";
                            bg = 0x6A2D2D;
                            fg = 0xFFE5E5;
                        }
                    }
                }
                ExplorerContextMenuKind::PasteArea => {
                    if idx == 0 {
                        label = "Crear carpeta";
                        bg = 0x2D4A5F;
                    } else if idx == 1 {
                        label = "Crear nota";
                        bg = 0x3A4D66;
                    } else if idx == 2 && menu.show_paste {
                        if let Some(clip) = self.explorer_clipboard.as_ref() {
                            if clip.mode == ExplorerClipboardMode::Cut {
                                label = "Pegar (mover)";
                                bg = 0x5A4630;
                                fg = 0xFFF2DE;
                            } else {
                                label = "Pegar";
                                bg = 0x2F566F;
                            }
                        } else {
                            label = "Pegar";
                            bg = 0x2B3440;
                            fg = 0x9FB2C4;
                        }
                    }
                }
                ExplorerContextMenuKind::DesktopArea => {
                    if idx == 0 {
                        label = "Crear carpeta";
                        bg = 0x2D4A5F;
                    } else if idx == 1 {
                        label = "Crear nota";
                        bg = 0x3A4D66;
                    } else if idx == 2 && menu.show_paste {
                        if let Some(clip) = self.explorer_clipboard.as_ref() {
                            if clip.mode == ExplorerClipboardMode::Cut {
                                label = "Pegar (mover)";
                                bg = 0x5A4630;
                                fg = 0xFFF2DE;
                            } else {
                                label = "Pegar";
                                bg = 0x2F566F;
                            }
                        } else {
                            label = "Pegar";
                            bg = 0x2B3440;
                            fg = 0x9FB2C4;
                        }
                    }
                }
            }

            framebuffer::rect(
                item_rect.x.max(0) as usize,
                item_rect.y.max(0) as usize,
                item_rect.width as usize,
                item_rect.height as usize,
                bg,
            );
            framebuffer::draw_text_5x7(
                (item_rect.x + 8).max(0) as usize,
                (item_rect.y + 8).max(0) as usize,
                label,
                fg,
            );
        }
    }

    fn handle_explorer_context_click(&mut self, win_id: usize, mouse_x: i32, mouse_y: i32) -> bool {
        let (clicked, is_canvas, source_dir_cluster, items) = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => (
                win.explorer_item_at(mouse_x, mouse_y),
                win.explorer_canvas_contains(mouse_x, mouse_y),
                win.explorer_current_cluster,
                win.explorer_items.clone(),
            ),
            None => return false,
        };

        if let Some(item) = clicked {
            if item.kind == ExplorerItemKind::File || item.kind == ExplorerItemKind::Directory {
                let selected = self.explorer_collect_selected_items(
                    win_id,
                    source_dir_cluster,
                    items.as_slice(),
                );
                if !self.explorer_item_selected(win_id, source_dir_cluster, &item) || selected.is_empty()
                {
                    self.explorer_select_single(win_id, source_dir_cluster, &item);
                }
                let selection_count = self
                    .explorer_collect_selected_items(win_id, source_dir_cluster, items.as_slice())
                    .len()
                    .max(1);

                let kind = if item.kind == ExplorerItemKind::Directory {
                    ExplorerContextMenuKind::DirectoryItem
                } else {
                    ExplorerContextMenuKind::FileItem
                };
                let item_count = Self::explorer_context_item_count_for_kind(
                    kind,
                    Some(&item),
                    source_dir_cluster,
                    selection_count,
                );
                let (menu_x, menu_y) =
                    self.clamp_explorer_context_menu_origin(mouse_x, mouse_y, item_count);
                self.explorer_context_menu = Some(ExplorerContextMenuState {
                    win_id,
                    kind,
                    x: menu_x,
                    y: menu_y,
                    source_dir_cluster,
                    target_item: Some(item),
                    show_paste: false,
                    selection_count,
                });
                return true;
            }

            self.explorer_clear_selection_scope(win_id, source_dir_cluster);
        }

        if is_canvas {
            self.explorer_clear_selection_scope(win_id, source_dir_cluster);
            let show_paste = self.explorer_clipboard.is_some();
            let item_count = if show_paste { 3 } else { 2 };
            let (menu_x, menu_y) =
                self.clamp_explorer_context_menu_origin(mouse_x, mouse_y, item_count);
            self.explorer_context_menu = Some(ExplorerContextMenuState {
                win_id,
                kind: ExplorerContextMenuKind::PasteArea,
                x: menu_x,
                y: menu_y,
                source_dir_cluster,
                target_item: None,
                show_paste,
                selection_count: 0,
            });
            return true;
        }

        false
    }

    fn handle_right_click(&mut self, mouse_x: i32, mouse_y: i32) {
        if self.notepad_save_prompt.is_some() {
            self.explorer_context_menu = None;
            self.desktop_context_menu = None;
            return;
        }
        if self.rename_prompt.is_some() {
            self.explorer_context_menu = None;
            self.desktop_context_menu = None;
            return;
        }
        if self.desktop_create_folder.is_some() {
            self.explorer_context_menu = None;
            self.desktop_context_menu = None;
            return;
        }
        self.taskbar.start_menu_open = false;
        self.start_tools_open = false;
        self.start_games_open = false;
        self.start_apps_open = false;

        if self.taskbar.rect.contains(self.mouse_pos) {
            self.explorer_context_menu = None;
            self.desktop_context_menu = None;
            return;
        }

        for i in (0..self.windows.len()).rev() {
            if self.windows[i].state != WindowState::Normal
                && self.windows[i].state != WindowState::Maximized
            {
                continue;
            }

            if self.windows[i].rect.contains(self.mouse_pos) {
                let win_id = self.windows[i].id;
                self.active_window_id = Some(win_id);

                let top_idx = if i < self.windows.len() - 1 {
                    let win = self.windows.remove(i);
                    self.windows.push(win);
                    self.windows.len() - 1
                } else {
                    i
                };

                if self.windows[top_idx].is_explorer()
                    && self.handle_explorer_context_click(win_id, mouse_x, mouse_y)
                {
                    self.desktop_context_menu = None;
                    return;
                }

                self.explorer_context_menu = None;
                self.desktop_context_menu = None;
                return;
            }
        }

        if self.handle_desktop_usb_right_click() {
            self.explorer_context_menu = None;
            self.desktop_context_menu = None;
            return;
        }

        if self.handle_desktop_surface_right_click(mouse_x, mouse_y) {
            self.explorer_context_menu = None;
            return;
        }

        self.explorer_context_menu = None;
        self.desktop_context_menu = None;
    }

    fn handle_explorer_context_menu_left_click(&mut self) -> bool {
        let Some(menu) = self.explorer_context_menu.clone() else {
            return false;
        };

        let menu_rect = self.explorer_context_menu_rect(&menu);
        if !menu_rect.contains(self.mouse_pos) {
            self.explorer_context_menu = None;
            return false;
        }

        let item_count = self.explorer_context_menu_item_count(&menu);
        for idx in 0..item_count {
            let item_rect = self.explorer_context_menu_item_rect(&menu, idx);
            if !item_rect.contains(self.mouse_pos) {
                continue;
            }

            self.explorer_context_menu = None;
            match menu.kind {
                ExplorerContextMenuKind::FileItem => {
                    if let Some(item) = menu.target_item.as_ref() {
                        let targets =
                            self.explorer_context_target_items(menu.win_id, menu.source_dir_cluster, item);
                        if idx == 0 {
                            self.set_explorer_clipboard_from_items(
                                Some(menu.win_id),
                                menu.source_dir_cluster,
                                targets.as_slice(),
                                ExplorerClipboardMode::Copy,
                            );
                        } else if idx == 1 {
                            self.set_explorer_clipboard_from_items(
                                Some(menu.win_id),
                                menu.source_dir_cluster,
                                targets.as_slice(),
                                ExplorerClipboardMode::Cut,
                            );
                        } else if idx == 2 {
                            self.begin_rename_prompt_for_explorer_item(
                                menu.win_id,
                                menu.source_dir_cluster,
                                item,
                            );
                        } else if idx == 3 {
                            self.delete_explorer_items(
                                menu.win_id,
                                menu.source_dir_cluster,
                                targets.as_slice(),
                            );
                        } else if menu.selection_count <= 1 && targets.len() == 1 {
                            let target = &targets[0];
                            let mut extra_idx = 4usize;

                            if Self::explorer_item_is_zip(target) {
                                if idx == extra_idx {
                                    self.extract_zip_in_current_directory(
                                        menu.win_id,
                                        menu.source_dir_cluster,
                                        target,
                                    );
                                }
                                extra_idx += 1;
                            }

                            if Self::explorer_item_is_deb(target) && idx == extra_idx {
                                let source_path = self
                                    .windows
                                    .iter()
                                    .find(|w| w.id == menu.win_id)
                                    .map(|w| w.explorer_path.clone());
                                self.launch_install_from_context(
                                    menu.source_dir_cluster,
                                    source_path.as_deref(),
                                    target.label.as_str(),
                                );
                            }
                        }
                    }
                }
                ExplorerContextMenuKind::DirectoryItem => {
                    if let Some(item) = menu.target_item.as_ref() {
                        let is_trash = item.kind == ExplorerItemKind::ShortcutRecycleBin;
                        if is_trash && idx == 0 {
                            let mut fat = unsafe { crate::fat32::Fat32::new() };
                            let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
                            if trash_cluster >= 2 {
                                let _ = fat.empty_directory(trash_cluster);
                            }
                            if let Some(win) = self.windows.iter_mut().find(|w| w.id == menu.win_id) {
                                win.set_explorer_status("Papelera vaciada.");
                            }
                        } else if !is_trash {
                            let targets =
                                self.explorer_context_target_items(menu.win_id, menu.source_dir_cluster, item);
                            if idx == 0 {
                                self.set_explorer_clipboard_from_items(
                                    Some(menu.win_id),
                                    menu.source_dir_cluster,
                                    targets.as_slice(),
                                    ExplorerClipboardMode::Copy,
                                );
                            } else if idx == 1 {
                                self.set_explorer_clipboard_from_items(
                                    Some(menu.win_id),
                                    menu.source_dir_cluster,
                                    targets.as_slice(),
                                    ExplorerClipboardMode::Cut,
                                );
                            } else if idx == 2 {
                                self.begin_rename_prompt_for_explorer_item(
                                    menu.win_id,
                                    menu.source_dir_cluster,
                                    item,
                                );
                            } else if idx == 3 {
                                self.delete_explorer_items(
                                    menu.win_id,
                                    menu.source_dir_cluster,
                                    targets.as_slice(),
                                );
                            }
                        }
                    }
                }
                ExplorerContextMenuKind::PasteArea => {
                    if idx == 0 {
                        self.begin_explorer_create_folder(menu.win_id);
                    } else if idx == 1 {
                        self.open_notepad_blank();
                    } else if idx == 2 && menu.show_paste {
                        self.paste_explorer_clipboard(menu.win_id);
                    }
                }
                ExplorerContextMenuKind::DesktopArea => {}
            }
            return true;
        }

        self.explorer_context_menu = None;
        true
    }

    fn handle_desktop_context_menu_left_click(&mut self) -> bool {
        let Some(menu) = self.desktop_context_menu.clone() else {
            return false;
        };

        let menu_rect = self.explorer_context_menu_rect(&menu);
        if !menu_rect.contains(self.mouse_pos) {
            self.desktop_context_menu = None;
            return false;
        }

        let item_count = self.explorer_context_menu_item_count(&menu);
        for idx in 0..item_count {
            let item_rect = self.explorer_context_menu_item_rect(&menu, idx);
            if !item_rect.contains(self.mouse_pos) {
                continue;
            }

            self.desktop_context_menu = None;
            match menu.kind {
                ExplorerContextMenuKind::FileItem => {
                    if let Some(item) = menu.target_item.as_ref() {
                        let targets =
                            self.desktop_context_target_items(menu.source_dir_cluster, item);
                        if idx == 0 {
                            self.set_desktop_clipboard_from_items(
                                menu.source_dir_cluster,
                                targets.as_slice(),
                                ExplorerClipboardMode::Copy,
                            );
                        } else if idx == 1 {
                            self.set_desktop_clipboard_from_items(
                                menu.source_dir_cluster,
                                targets.as_slice(),
                                ExplorerClipboardMode::Cut,
                            );
                        } else if idx == 2 {
                            self.begin_rename_prompt_for_desktop_item(
                                menu.source_dir_cluster,
                                item,
                            );
                        } else if idx == 3 {
                            self.delete_desktop_items(menu.source_dir_cluster, targets.as_slice());
                        } else if menu.selection_count <= 1 && targets.len() == 1 {
                            let target = &targets[0];
                            let mut extra_idx = 4usize;

                            if Self::explorer_item_is_zip(target) {
                                if idx == extra_idx {
                                    self.extract_zip_on_desktop(menu.source_dir_cluster, target);
                                }
                                extra_idx += 1;
                            }

                            if Self::explorer_item_is_deb(target) && idx == extra_idx {
                                self.launch_install_from_context(
                                    menu.source_dir_cluster,
                                    None,
                                    target.label.as_str(),
                                );
                            }
                        }
                    }
                }
                ExplorerContextMenuKind::DirectoryItem => {
                    if let Some(item) = menu.target_item.as_ref() {
                        let is_trash = item.kind == ExplorerItemKind::ShortcutRecycleBin;
                        if is_trash && idx == 0 {
                            let mut fat = unsafe { crate::fat32::Fat32::new() };
                            let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
                            if trash_cluster >= 2 {
                                let _ = fat.empty_directory(trash_cluster);
                            }
                            self.desktop_surface_status = String::from("Papelera vaciada.");
                        } else if !is_trash {
                            let targets =
                                self.desktop_context_target_items(menu.source_dir_cluster, item);
                            if idx == 0 {
                                self.set_desktop_clipboard_from_items(
                                    menu.source_dir_cluster,
                                    targets.as_slice(),
                                    ExplorerClipboardMode::Copy,
                                );
                            } else if idx == 1 {
                                self.set_desktop_clipboard_from_items(
                                    menu.source_dir_cluster,
                                    targets.as_slice(),
                                    ExplorerClipboardMode::Cut,
                                );
                            } else if idx == 2 {
                                self.begin_rename_prompt_for_desktop_item(
                                    menu.source_dir_cluster,
                                    item,
                                );
                            } else if idx == 3 {
                                self.delete_desktop_items(menu.source_dir_cluster, targets.as_slice());
                            }
                        }
                    }
                }
                ExplorerContextMenuKind::PasteArea => {
                    if idx == 0 {
                        self.begin_desktop_create_folder(menu.source_dir_cluster);
                    } else if idx == 1 {
                        self.open_notepad_blank();
                    } else if idx == 2 && menu.show_paste {
                        self.paste_clipboard_to_desktop();
                    }
                }
                ExplorerContextMenuKind::DesktopArea => {
                    if idx == 0 {
                        self.begin_desktop_create_folder(menu.source_dir_cluster);
                    } else if idx == 1 {
                        self.open_notepad_blank();
                    } else if idx == 2 && menu.show_paste {
                        self.paste_clipboard_to_desktop();
                    }
                }
            }
            return true;
        }

        self.desktop_context_menu = None;
        true
    }

    fn explorer_context_target_items(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        clicked_item: &ExplorerItem,
    ) -> Vec<ExplorerItem> {
        let items = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => win.explorer_items.clone(),
            None => return vec![clicked_item.clone()],
        };

        let selected =
            self.explorer_collect_selected_items(win_id, source_dir_cluster, items.as_slice());
        if selected.len() <= 1 {
            return vec![clicked_item.clone()];
        }

        if selected.iter().any(|entry| {
            Self::explorer_item_key_eq(entry.cluster, entry.label.as_str(), clicked_item)
        }) {
            selected
        } else {
            vec![clicked_item.clone()]
        }
    }

    fn desktop_context_target_items(
        &mut self,
        source_dir_cluster: u32,
        clicked_item: &ExplorerItem,
    ) -> Vec<ExplorerItem> {
        let Some((dir_cluster, _dir_path, items)) = self.desktop_surface_items() else {
            return vec![clicked_item.clone()];
        };
        if dir_cluster != source_dir_cluster {
            return vec![clicked_item.clone()];
        }

        let selected = self.desktop_collect_selected_items(source_dir_cluster, items.as_slice());
        if selected.len() <= 1 {
            return vec![clicked_item.clone()];
        }

        if selected.iter().any(|entry| {
            Self::desktop_item_key_eq(entry.cluster, entry.label.as_str(), clicked_item)
        }) {
            selected
        } else {
            vec![clicked_item.clone()]
        }
    }

    fn set_desktop_clipboard_from_items(
        &mut self,
        source_dir_cluster: u32,
        items: &[ExplorerItem],
        mode: ExplorerClipboardMode,
    ) {
        if items.is_empty() {
            self.desktop_surface_status = String::from("No hay elementos seleccionados.");
            return;
        }

        let mut clip_items = Vec::new();
        let source_dir_path = self
            .desktop_surface_items()
            .and_then(|(cluster, path, _)| {
                if cluster == source_dir_cluster {
                    Some(path)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let source_device_index = self.resolve_device_index_for_directory(
            source_dir_cluster,
            Some(source_dir_path.as_str()),
            self.current_volume_device_index,
        );
        for item in items.iter() {
            if item.kind != ExplorerItemKind::File && item.kind != ExplorerItemKind::Directory {
                continue;
            }
            clip_items.push(ExplorerClipboardItem {
                source_device_index,
                source_dir_cluster,
                source_dir_path: source_dir_path.clone(),
                source_item_cluster: item.cluster,
                source_is_directory: item.kind == ExplorerItemKind::Directory,
                source_label: item.label.clone(),
            });
        }

        let Some(first) = clip_items.first().cloned() else {
            self.desktop_surface_status = String::from("Seleccion invalida.");
            return;
        };

        self.explorer_clipboard = Some(ExplorerClipboardState {
            mode,
            source_device_index: first.source_device_index,
            source_dir_cluster: first.source_dir_cluster,
            source_dir_path: first.source_dir_path.clone(),
            source_item_cluster: first.source_item_cluster,
            source_is_directory: first.source_is_directory,
            source_label: first.source_label.clone(),
            items: clip_items.clone(),
        });

        let verb = if mode == ExplorerClipboardMode::Cut {
            "Cortar"
        } else {
            "Copiar"
        };
        self.desktop_surface_status = alloc::format!(
            "{} {} elemento(s) listo(s). Clic derecho en destino y luego 'Pegar'.",
            verb,
            clip_items.len()
        );
    }

    fn delete_desktop_items(&mut self, source_dir_cluster: u32, items: &[ExplorerItem]) {
        if items.is_empty() {
            self.desktop_surface_status = String::from("No hay elementos para eliminar.");
            return;
        }

        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for item in items.iter() {
            if item.kind == ExplorerItemKind::File {
                files.push(item.clone());
            } else if item.kind == ExplorerItemKind::Directory {
                dirs.push(item.clone());
            }
        }

        let mut done = 0usize;
        for file in files.into_iter() {
            self.delete_desktop_file(source_dir_cluster, &file);
            done += 1;
        }
        for dir in dirs.into_iter() {
            self.delete_desktop_directory(source_dir_cluster, &dir);
            done += 1;
        }

        self.desktop_selected_items.clear();
        self.desktop_surface_status = alloc::format!("Eliminados {} elemento(s).", done);
    }

    fn set_explorer_clipboard_from_items(
        &mut self,
        status_win_id: Option<usize>,
        source_dir_cluster: u32,
        items: &[ExplorerItem],
        mode: ExplorerClipboardMode,
    ) {
        if items.is_empty() {
            let status = String::from("No hay elementos seleccionados.");
            if let Some(win_id) = status_win_id {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_explorer_status(status.as_str());
                }
            } else {
                self.desktop_surface_status = status;
            }
            return;
        }

        let mut clip_items = Vec::new();
        let source_path_hint = status_win_id.and_then(|win_id| {
            self.windows
                .iter()
                .find(|w| w.id == win_id)
                .map(|w| w.explorer_path.clone())
        });
        let source_window_device = status_win_id.and_then(|win_id| {
            self.windows
                .iter()
                .find(|w| w.id == win_id)
                .and_then(|w| w.explorer_device_index)
        });
        let source_device_index = source_window_device.or_else(|| {
            self.resolve_device_index_for_directory(
                source_dir_cluster,
                source_path_hint.as_deref(),
                self.current_volume_device_index,
            )
        });
        for item in items.iter() {
            if item.kind != ExplorerItemKind::File && item.kind != ExplorerItemKind::Directory {
                continue;
            }
            clip_items.push(ExplorerClipboardItem {
                source_device_index,
                source_dir_cluster,
                source_dir_path: source_path_hint.clone().unwrap_or_default(),
                source_item_cluster: item.cluster,
                source_is_directory: item.kind == ExplorerItemKind::Directory,
                source_label: item.label.clone(),
            });
        }

        let Some(first) = clip_items.first().cloned() else {
            let status = String::from("Seleccion invalida.");
            if let Some(win_id) = status_win_id {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_explorer_status(status.as_str());
                }
            } else {
                self.desktop_surface_status = status;
            }
            return;
        };

        self.explorer_clipboard = Some(ExplorerClipboardState {
            mode,
            source_device_index: first.source_device_index,
            source_dir_cluster: first.source_dir_cluster,
            source_dir_path: first.source_dir_path.clone(),
            source_item_cluster: first.source_item_cluster,
            source_is_directory: first.source_is_directory,
            source_label: first.source_label.clone(),
            items: clip_items.clone(),
        });

        let verb = if mode == ExplorerClipboardMode::Cut {
            "Cortar"
        } else {
            "Copiar"
        };
        let status = alloc::format!(
            "{} {} elemento(s) listo(s). Clic derecho en espacio y luego 'Pegar'.",
            verb,
            clip_items.len(),
        );
        if let Some(win_id) = status_win_id {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status(status.as_str());
            }
        } else {
            self.desktop_surface_status = status;
        }
    }

    fn delete_explorer_items(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        items: &[ExplorerItem],
    ) {
        if items.is_empty() {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("No hay elementos para eliminar.");
            }
            return;
        }
        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        let dir_path = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => win.explorer_path.clone(),
            None => String::from("/"),
        };

        let mut deleted = 0usize;
        let mut denied = 0usize;
        let mut failed = 0usize;
        let mut clear_clipboard = false;

        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for item in items.iter() {
            if item.kind == ExplorerItemKind::File {
                files.push(item.clone());
            } else if item.kind == ExplorerItemKind::Directory {
                dirs.push(item.clone());
            }
        }

        for item in files.into_iter() {
            let result = {
                let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                let source_entry = match Self::find_file_entry_by_hint(
                    fat,
                    source_dir_cluster,
                    item.label.as_str(),
                    item.cluster,
                ) {
                    Ok(entry) => entry,
                    Err(_) => {
                        failed += 1;
                        continue;
                    }
                };
                let source_name = Self::dir_entry_short_name(&source_entry);
                fat.ensure_subdirectory(fat.root_cluster, "TRASH");
                let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
                if trash_cluster >= 2 {
                    fat.move_entry(source_dir_cluster, trash_cluster, source_name.as_str())
                } else {
                    fat.delete_file_in_dir(source_dir_cluster, source_name.as_str())
                }
            };
            if result.is_ok() {
                deleted += 1;
                if self
                    .explorer_clipboard
                    .as_ref()
                    .map(|clip| Self::clipboard_contains_item(clip, source_dir_cluster, &item))
                    .unwrap_or(false)
                {
                    clear_clipboard = true;
                }
            } else {
                failed += 1;
            }
        }

        for item in dirs.into_iter() {
            if !Self::explorer_directory_can_delete(source_dir_cluster, &item) {
                denied += 1;
                continue;
            }

            let result = {
                let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                let source_entry = match Self::find_directory_entry_by_hint(
                    fat,
                    source_dir_cluster,
                    item.label.as_str(),
                    item.cluster,
                ) {
                    Ok(entry) => entry,
                    Err(_) => {
                        failed += 1;
                        continue;
                    }
                };
                let source_name = Self::dir_entry_short_name(&source_entry);
                fat.ensure_subdirectory(fat.root_cluster, "TRASH");
                let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
                if trash_cluster >= 2 {
                    fat.move_entry(source_dir_cluster, trash_cluster, source_name.as_str())
                } else {
                    fat.delete_directory_in_dir(source_dir_cluster, source_name.as_str())
                }
            };
            if result.is_ok() {
                deleted += 1;
                if self
                    .explorer_clipboard
                    .as_ref()
                    .map(|clip| Self::clipboard_contains_item(clip, source_dir_cluster, &item))
                    .unwrap_or(false)
                {
                    clear_clipboard = true;
                }
            } else {
                failed += 1;
            }
        }

        if clear_clipboard {
            self.explorer_clipboard = None;
        }

        self.explorer_clear_selection_scope(win_id, source_dir_cluster);

        let mut status = alloc::format!("Eliminados {} elemento(s).", deleted);
        if denied > 0 {
            status.push_str(alloc::format!(" {} protegido(s).", denied).as_str());
        }
        if failed > 0 {
            status.push_str(alloc::format!(" {} con error.", failed).as_str());
        }
        if deleted == 0 && failed > 0 && denied == 0 {
            status = String::from("No se pudo eliminar la seleccion.");
        }

        self.show_explorer_directory(win_id, source_dir_cluster, dir_path, status, None);
    }

    fn clipboard_items(clip: &ExplorerClipboardState) -> Vec<ExplorerClipboardItem> {
        if !clip.items.is_empty() {
            return clip.items.clone();
        }
        vec![ExplorerClipboardItem {
            source_device_index: clip.source_device_index,
            source_dir_cluster: clip.source_dir_cluster,
            source_dir_path: clip.source_dir_path.clone(),
            source_item_cluster: clip.source_item_cluster,
            source_is_directory: clip.source_is_directory,
            source_label: clip.source_label.clone(),
        }]
    }

    fn clipboard_contains_item(
        clip: &ExplorerClipboardState,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) -> bool {
        Self::clipboard_items(clip).iter().any(|entry| {
            entry.source_dir_cluster == source_dir_cluster
                && (entry.source_item_cluster == item.cluster
                    || entry
                        .source_label
                        .eq_ignore_ascii_case(item.label.as_str()))
        })
    }

    fn path_volume_label_hint(path: &str) -> Option<&str> {
        let trimmed = path.trim().trim_start_matches('/');
        if trimmed.is_empty() {
            return None;
        }
        let head = trimmed.split('/').next().unwrap_or("").trim();
        if head.is_empty() {
            None
        } else {
            Some(head)
        }
    }

    fn candidate_device_indices_with_preferred(&self, preferred: Option<usize>) -> Vec<usize> {
        let devices = crate::fat32::Fat32::detect_uefi_block_devices();
        let mut out = Vec::new();

        if let Some(index) = preferred {
            if devices.iter().any(|dev| dev.index == index) {
                Self::push_unique_device_index(&mut out, index);
            }
        }
        if let Some(current) = self.current_volume_device_index {
            if devices.iter().any(|dev| dev.index == current) {
                Self::push_unique_device_index(&mut out, current);
            }
        }

        for index in self.auto_mount_candidate_indices().into_iter() {
            if devices.iter().any(|dev| dev.index == index) {
                Self::push_unique_device_index(&mut out, index);
            }
        }

        for dev in devices.iter() {
            Self::push_unique_device_index(&mut out, dev.index);
        }

        out
    }

    fn clipboard_item_exists_on_device(index: usize, item: &ExplorerClipboardItem) -> bool {
        let mut fat = crate::fat32::Fat32::new();
        if fat.mount_uefi_block_device(index).is_err() {
            return false;
        }

        if let Some(label_hint) = Self::path_volume_label_hint(item.source_dir_path.as_str()) {
            let mounted_label =
                Self::volume_label_text(&fat).unwrap_or(alloc::format!("VOL{}", index));
            if !mounted_label.eq_ignore_ascii_case(label_hint) {
                return false;
            }
        }

        let source_dir_cluster = Self::resolve_directory_cluster_from_explorer_path(
            &mut fat,
            item.source_dir_path.as_str(),
        )
        .unwrap_or(item.source_dir_cluster);

        if item.source_is_directory {
            Self::find_directory_entry_by_hint(
                &mut fat,
                source_dir_cluster,
                item.source_label.as_str(),
                item.source_item_cluster,
            )
            .is_ok()
        } else {
            Self::find_file_entry_by_hint(
                &mut fat,
                source_dir_cluster,
                item.source_label.as_str(),
                item.source_item_cluster,
            )
            .is_ok()
        }
    }

    fn resolve_clipboard_item_device_index(
        &self,
        item: &ExplorerClipboardItem,
        preferred: Option<usize>,
    ) -> Option<usize> {
        for index in self
            .candidate_device_indices_with_preferred(preferred)
            .into_iter()
        {
            if Self::clipboard_item_exists_on_device(index, item) {
                return Some(index);
            }
        }

        preferred
    }

    fn directory_cluster_exists_on_device(
        index: usize,
        cluster: u32,
        path_hint: Option<&str>,
    ) -> bool {
        if cluster < 2 {
            return false;
        }

        let mut fat = crate::fat32::Fat32::new();
        if fat.mount_uefi_block_device(index).is_err() {
            return false;
        }

        if let Some(label_hint) = path_hint.and_then(Self::path_volume_label_hint) {
            let mounted_label =
                Self::volume_label_text(&fat).unwrap_or(alloc::format!("VOL{}", index));
            if !mounted_label.eq_ignore_ascii_case(label_hint) {
                return false;
            }
        }

        fat.read_dir_entries(cluster).is_ok()
    }

    fn resolve_device_index_for_directory(
        &self,
        cluster: u32,
        path_hint: Option<&str>,
        preferred: Option<usize>,
    ) -> Option<usize> {
        for index in self
            .candidate_device_indices_with_preferred(preferred)
            .into_iter()
        {
            if Self::directory_cluster_exists_on_device(index, cluster, path_hint) {
                return Some(index);
            }
        }

        preferred
    }

    fn dir_entry_short_name(entry: &crate::fs::DirEntry) -> String {
        let mut out = String::new();

        let name_len = entry.name[0..8]
            .iter()
            .position(|&c| c == b' ' || c == 0)
            .unwrap_or(8);
        for b in &entry.name[0..name_len] {
            out.push(*b as char);
        }

        let ext_len = entry.name[8..11]
            .iter()
            .position(|&c| c == b' ' || c == 0)
            .unwrap_or(3);
        if ext_len > 0 {
            out.push('.');
            for b in &entry.name[8..8 + ext_len] {
                out.push(*b as char);
            }
        }

        out
    }

    fn dir_entry_target_name(entry: &crate::fs::DirEntry, label_hint: &str) -> String {
        let hint = label_hint.trim();
        if !hint.is_empty() {
            return String::from(hint);
        }

        let full = entry.full_name();
        if !full.trim().is_empty() {
            return full;
        }

        Self::dir_entry_short_name(entry)
    }

    fn find_file_entry_by_hint(
        fat: &mut crate::fat32::Fat32,
        dir_cluster: u32,
        label_hint: &str,
        cluster_hint: u32,
    ) -> Result<crate::fs::DirEntry, &'static str> {
        use crate::fs::FileType;

        let entries = fat
            .read_dir_entries(dir_cluster)
            .map_err(|_| "no se pudo leer directorio origen")?;

        let mut by_name: Option<crate::fs::DirEntry> = None;
        let mut by_cluster: Option<crate::fs::DirEntry> = None;
        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File {
                continue;
            }
            let full_name = entry.full_name();
            let name_match =
                entry.matches_name(label_hint) || full_name.eq_ignore_ascii_case(label_hint);
            let cluster_match = cluster_hint >= 2 && entry.cluster == cluster_hint;

            if name_match && cluster_match {
                return Ok(*entry);
            }
            if name_match && by_name.is_none() {
                by_name = Some(*entry);
            }
            if cluster_match && by_cluster.is_none() {
                by_cluster = Some(*entry);
            }
        }

        if let Some(entry) = by_name {
            return Ok(entry);
        }
        if let Some(entry) = by_cluster {
            return Ok(entry);
        }
        Err("archivo origen no encontrado")
    }

    fn find_directory_entry_by_hint(
        fat: &mut crate::fat32::Fat32,
        dir_cluster: u32,
        label_hint: &str,
        cluster_hint: u32,
    ) -> Result<crate::fs::DirEntry, &'static str> {
        use crate::fs::FileType;

        let entries = fat
            .read_dir_entries(dir_cluster)
            .map_err(|_| "no se pudo leer directorio")?;

        let mut by_name: Option<crate::fs::DirEntry> = None;
        let mut by_cluster: Option<crate::fs::DirEntry> = None;
        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::Directory {
                continue;
            }

            let full_name = entry.full_name();
            let name_match =
                entry.matches_name(label_hint) || full_name.eq_ignore_ascii_case(label_hint);
            let cluster_match = cluster_hint >= 2 && entry.cluster == cluster_hint;

            if name_match && cluster_match {
                return Ok(*entry);
            }
            if name_match && by_name.is_none() {
                by_name = Some(*entry);
            }
            if cluster_match && by_cluster.is_none() {
                by_cluster = Some(*entry);
            }
        }

        if let Some(entry) = by_name {
            return Ok(entry);
        }
        if let Some(entry) = by_cluster {
            return Ok(entry);
        }
        Err("carpeta no encontrada")
    }

    fn parent_cluster_of_directory(
        fat: &mut crate::fat32::Fat32,
        dir_cluster: u32,
    ) -> Option<u32> {
        let root = fat.root_cluster;
        if dir_cluster == root {
            return Some(root);
        }
        if let Ok(entries) = fat.read_dir_entries(dir_cluster) {
            for entry in entries.iter() {
                if entry.matches_name("..") {
                    return Some(if entry.cluster == 0 { root } else { entry.cluster });
                }
            }
        }
        None
    }

    fn resolve_directory_cluster_from_explorer_path(
        fat: &mut crate::fat32::Fat32,
        path: &str,
    ) -> Option<u32> {
        use crate::fs::FileType;

        let mut components: Vec<&str> = path
            .trim()
            .trim_matches('/')
            .split('/')
            .filter_map(|part| {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
            .collect();

        if components.is_empty() {
            return Some(fat.root_cluster);
        }

        if let Some(label) = Self::volume_label_text(fat) {
            if components
                .first()
                .map(|head| Self::path_head_matches_volume_label(head, label.as_str()))
                .unwrap_or(false)
            {
                components.remove(0);
            }
        }

        let mut cluster = fat.root_cluster;
        for part in components.into_iter() {
            if part == "." {
                continue;
            }
            if part == ".." {
                cluster = Self::parent_cluster_of_directory(fat, cluster).unwrap_or(fat.root_cluster);
                continue;
            }

            let entries = fat.read_dir_entries(cluster).ok()?;
            let mut next = None;
            for entry in entries.iter() {
                if !entry.valid || entry.file_type != FileType::Directory {
                    continue;
                }
                if entry.matches_name(part) || entry.full_name().eq_ignore_ascii_case(part) {
                    next = Some(if entry.cluster == 0 {
                        fat.root_cluster
                    } else {
                        entry.cluster
                    });
                    break;
                }
            }
            cluster = next?;
        }

        Some(cluster)
    }

    fn directory_is_descendant_of(
        fat: &mut crate::fat32::Fat32,
        ancestor_cluster: u32,
        candidate_cluster: u32,
    ) -> bool {
        let root = fat.root_cluster;
        if ancestor_cluster < 2 || candidate_cluster < 2 {
            return false;
        }

        let mut current = candidate_cluster;
        let mut guard = 0usize;
        while guard < 2048 {
            if current == ancestor_cluster {
                return true;
            }
            if current == root {
                return false;
            }

            let Some(parent) = Self::parent_cluster_of_directory(fat, current) else {
                return false;
            };
            if parent == current {
                return false;
            }
            current = parent;
            guard += 1;
        }

        false
    }

    fn estimate_directory_tree_work(
        fat: &mut crate::fat32::Fat32,
        src_dir_cluster: u32,
    ) -> Result<(usize, usize), String> {
        use crate::fs::FileType;

        let entries = fat
            .read_dir_entries(src_dir_cluster)
            .map_err(|e| alloc::format!("no se pudo leer carpeta origen: {}", e))?;

        let mut units = 0usize;
        let mut items = 0usize;
        for entry in entries.iter() {
            if !entry.valid {
                continue;
            }
            let short_name = Self::dir_entry_short_name(entry);
            if short_name == "." || short_name == ".." {
                continue;
            }

            match entry.file_type {
                FileType::File => {
                    units = units.saturating_add((entry.size as usize).max(1));
                    items = items.saturating_add(1);
                }
                FileType::Directory => {
                    units = units.saturating_add(1);
                    items = items.saturating_add(1);
                    let (child_units, child_items) =
                        Self::estimate_directory_tree_work(fat, entry.cluster)?;
                    units = units.saturating_add(child_units);
                    items = items.saturating_add(child_items);
                }
            }
        }
        Ok((units, items))
    }

    fn copy_file_with_progress_same_device(
        &mut self,
        fat: &mut crate::fat32::Fat32,
        src_cluster: u32,
        src_size: usize,
        dst_dir_cluster: u32,
        dst_name: &str,
    ) -> Result<usize, String> {
        if src_size > COPY_MAX_FILE_BYTES {
            return Err(alloc::format!(
                "archivo demasiado grande (max {} bytes): {}",
                COPY_MAX_FILE_BYTES,
                dst_name
            ));
        }

        // Prefer the same streaming engine used in cross-device copies by mounting
        // a source FAT handle on the same device. This avoids large full-file RAM
        // buffering and keeps UI/progress responsive on heavy same-partition copies.
        if let Some(src_dev_idx) = self.current_volume_device_index {
            let mut src_fat = crate::fat32::Fat32::new();
            if src_fat.mount_uefi_block_device(src_dev_idx).is_ok() {
                let file_units = src_size.max(1);
                let mut units_reported = 0usize;
                self.copy_progress_touch(alloc::format!("Copiando {}", dst_name).as_str());
                let read_len = fat
                    .copy_file_from_fat_in_dir_with_progress(
                        &mut src_fat,
                        src_cluster,
                        src_size,
                        dst_dir_cluster,
                        dst_name,
                        |copied, total| {
                            if self.copy_progress_cancel_requested() {
                                return false;
                            }
                            let denom = total.max(1);
                            let target_units = copied
                                .saturating_mul(file_units)
                                .saturating_div(denom);
                            let delta = target_units.saturating_sub(units_reported);
                            if delta > 0 {
                                self.copy_progress_advance_units(delta);
                                units_reported = units_reported.saturating_add(delta);
                            }
                            !self.copy_progress_cancel_requested()
                        },
                    )
                    .map_err(|e| {
                        if e == "Operation canceled" {
                            Self::copy_progress_cancel_error()
                        } else {
                            alloc::format!("no se pudo copiar '{}': {}", dst_name, e)
                        }
                    })?;
                if units_reported < file_units {
                    self.copy_progress_advance_units(file_units - units_reported);
                }
                self.copy_progress_abort_if_cancelled()?;
                self.copy_progress_advance_item(None);
                return Ok(read_len);
            }
        }

        // Fallback path if source mirror mount is unavailable.
        let mut raw = Self::try_alloc_zeroed(src_size).map_err(String::from)?;
        let file_units = src_size.max(1);
        let read_units_target = file_units / 2;
        let write_units_target = file_units.saturating_sub(read_units_target);
        let mut read_units_reported = 0usize;

        self.copy_progress_touch(alloc::format!("Leyendo {}", dst_name).as_str());
        let read_len = fat
            .read_file_sized_with_progress(src_cluster, src_size, &mut raw, |copied, total| {
                if self.copy_progress_cancel_requested() {
                    return false;
                }
                let denom = total.max(1);
                let target_units = copied
                    .saturating_mul(read_units_target)
                    .saturating_div(denom);
                let delta = target_units.saturating_sub(read_units_reported);
                if delta > 0 {
                    self.copy_progress_advance_units(delta);
                    read_units_reported = read_units_reported.saturating_add(delta);
                }
                !self.copy_progress_cancel_requested()
            })
            .map_err(|e| {
                if e == "Operation canceled" {
                    Self::copy_progress_cancel_error()
                } else {
                    alloc::format!("no se pudo leer '{}': {}", dst_name, e)
                }
            })?;
        if read_units_reported < read_units_target {
            self.copy_progress_advance_units(read_units_target - read_units_reported);
        }
        raw.truncate(read_len);
        self.copy_progress_abort_if_cancelled()?;

        let mut write_units_reported = 0usize;
        self.copy_progress_touch(alloc::format!("Escribiendo {}", dst_name).as_str());
        fat.write_text_file_in_dir_with_progress(dst_dir_cluster, dst_name, raw.as_slice(), |written, total| {
            if self.copy_progress_cancel_requested() {
                return false;
            }
            let denom = total.max(1);
            let target_units = written
                .saturating_mul(write_units_target)
                .saturating_div(denom);
            let delta = target_units.saturating_sub(write_units_reported);
            if delta > 0 {
                self.copy_progress_advance_units(delta);
                write_units_reported = write_units_reported.saturating_add(delta);
            }
            !self.copy_progress_cancel_requested()
        })
        .map_err(|e| {
            if e == "Operation canceled" {
                Self::copy_progress_cancel_error()
            } else {
                alloc::format!("no se pudo escribir '{}': {}", dst_name, e)
            }
        })?;
        if write_units_reported < write_units_target {
            self.copy_progress_advance_units(write_units_target - write_units_reported);
        }
        self.copy_progress_advance_item(None);
        Ok(read_len)
    }

    fn copy_file_with_progress_cross_device(
        &mut self,
        src_fat: &mut crate::fat32::Fat32,
        src_cluster: u32,
        src_size: usize,
        dst_fat: &mut crate::fat32::Fat32,
        dst_dir_cluster: u32,
        dst_name: &str,
    ) -> Result<usize, String> {
        if src_size > COPY_MAX_FILE_BYTES {
            return Err(alloc::format!(
                "archivo demasiado grande (max {} bytes): {}",
                COPY_MAX_FILE_BYTES,
                dst_name
            ));
        }

        let file_units = src_size.max(1);
        let mut units_reported = 0usize;
        self.copy_progress_touch(alloc::format!("Copiando {}", dst_name).as_str());
        let read_len = dst_fat
            .copy_file_from_fat_in_dir_with_progress(
                src_fat,
                src_cluster,
                src_size,
                dst_dir_cluster,
                dst_name,
                |copied, total| {
                    if self.copy_progress_cancel_requested() {
                        return false;
                    }
                    let denom = total.max(1);
                    let target_units = copied
                        .saturating_mul(file_units)
                        .saturating_div(denom);
                    let delta = target_units.saturating_sub(units_reported);
                    if delta > 0 {
                        self.copy_progress_advance_units(delta);
                        units_reported = units_reported.saturating_add(delta);
                    }
                    !self.copy_progress_cancel_requested()
                },
            )
            .map_err(|e| {
                if e == "Operation canceled" {
                    Self::copy_progress_cancel_error()
                } else {
                    alloc::format!("no se pudo copiar '{}': {}", dst_name, e)
                }
            })?;
        if units_reported < file_units {
            self.copy_progress_advance_units(file_units - units_reported);
        }
        self.copy_progress_abort_if_cancelled()?;
        self.copy_progress_advance_item(None);
        Ok(read_len)
    }

    fn copy_directory_tree_same_device(
        &mut self,
        fat: &mut crate::fat32::Fat32,
        src_dir_cluster: u32,
        dst_dir_cluster: u32,
    ) -> Result<(usize, usize, usize), String> {
        use crate::fs::FileType;

        self.copy_progress_abort_if_cancelled()?;
        let entries = fat
            .read_dir_entries(src_dir_cluster)
            .map_err(|e| alloc::format!("no se pudo leer carpeta origen: {}", e))?;

        let mut files = 0usize;
        let mut dirs = 0usize;
        let mut bytes = 0usize;

        for entry in entries.iter() {
            self.copy_progress_abort_if_cancelled()?;
            if !entry.valid {
                continue;
            }
            let short_name = Self::dir_entry_short_name(entry);
            if short_name == "." || short_name == ".." {
                continue;
            }

            if entry.file_type == FileType::File {
                let read_len = self.copy_file_with_progress_same_device(
                    fat,
                    entry.cluster,
                    entry.size as usize,
                    dst_dir_cluster,
                    short_name.as_str(),
                )?;
                files = files.saturating_add(1);
                bytes = bytes.saturating_add(read_len);
                continue;
            }

            if entry.file_type == FileType::Directory {
                self.copy_progress_touch(alloc::format!("Creando carpeta {}", short_name).as_str());
                let child_dst = fat
                    .ensure_subdirectory(dst_dir_cluster, short_name.as_str())
                    .map_err(|e| alloc::format!("no se pudo crear carpeta '{}': {}", short_name, e))?;
                self.copy_progress_sync(None, 1, 1, false);
                dirs = dirs.saturating_add(1);
                let (f, d, b) = self.copy_directory_tree_same_device(fat, entry.cluster, child_dst)?;
                files = files.saturating_add(f);
                dirs = dirs.saturating_add(d);
                bytes = bytes.saturating_add(b);
            }
        }

        Ok((files, dirs, bytes))
    }

    fn copy_directory_tree_cross_device(
        &mut self,
        src_fat: &mut crate::fat32::Fat32,
        src_dir_cluster: u32,
        dst_fat: &mut crate::fat32::Fat32,
        dst_dir_cluster: u32,
    ) -> Result<(usize, usize, usize), String> {
        use crate::fs::FileType;

        self.copy_progress_abort_if_cancelled()?;
        let entries = src_fat
            .read_dir_entries(src_dir_cluster)
            .map_err(|e| alloc::format!("no se pudo leer carpeta origen: {}", e))?;

        let mut files = 0usize;
        let mut dirs = 0usize;
        let mut bytes = 0usize;

        for entry in entries.iter() {
            self.copy_progress_abort_if_cancelled()?;
            if !entry.valid {
                continue;
            }
            let short_name = Self::dir_entry_short_name(entry);
            if short_name == "." || short_name == ".." {
                continue;
            }

            if entry.file_type == FileType::File {
                let read_len = self.copy_file_with_progress_cross_device(
                    src_fat,
                    entry.cluster,
                    entry.size as usize,
                    dst_fat,
                    dst_dir_cluster,
                    short_name.as_str(),
                )?;
                files = files.saturating_add(1);
                bytes = bytes.saturating_add(read_len);
                continue;
            }

            if entry.file_type == FileType::Directory {
                self.copy_progress_touch(alloc::format!("Creando carpeta {}", short_name).as_str());
                let child_dst = dst_fat
                    .ensure_subdirectory(dst_dir_cluster, short_name.as_str())
                    .map_err(|e| alloc::format!("no se pudo crear carpeta '{}': {}", short_name, e))?;
                self.copy_progress_sync(None, 1, 1, false);
                dirs = dirs.saturating_add(1);
                let (f, d, b) =
                    self.copy_directory_tree_cross_device(src_fat, entry.cluster, dst_fat, child_dst)?;
                files = files.saturating_add(f);
                dirs = dirs.saturating_add(d);
                bytes = bytes.saturating_add(b);
            }
        }

        Ok((files, dirs, bytes))
    }

    fn remove_directory_tree_in_dir_with_progress(
        &mut self,
        fat: &mut crate::fat32::Fat32,
        parent_dir_cluster: u32,
        dir_name: &str,
        dir_cluster: u32,
    ) -> Result<(), String> {
        use crate::fs::FileType;

        let entries = fat
            .read_dir_entries(dir_cluster)
            .map_err(|e| alloc::format!("no se pudo leer carpeta '{}': {}", dir_name, e))?;

        for entry in entries.iter() {
            if !entry.valid {
                continue;
            }
            let child_name = Self::dir_entry_short_name(entry);
            if child_name == "." || child_name == ".." {
                continue;
            }

            match entry.file_type {
                FileType::File => {
                    self.copy_progress_touch(
                        alloc::format!("Eliminando origen {}", child_name).as_str(),
                    );
                    fat.delete_file_in_dir(dir_cluster, child_name.as_str())
                        .map_err(|e| alloc::format!("no se pudo borrar '{}': {}", child_name, e))?;
                    self.copy_progress_sync(None, 1, 1, false);
                }
                FileType::Directory => self.remove_directory_tree_in_dir_with_progress(
                    fat,
                    dir_cluster,
                    child_name.as_str(),
                    entry.cluster,
                )?,
            }
        }

        self.copy_progress_touch(alloc::format!("Eliminando carpeta {}", dir_name).as_str());
        fat.delete_directory_in_dir(parent_dir_cluster, dir_name)
            .map_err(|e| alloc::format!("no se pudo borrar carpeta '{}': {}", dir_name, e))?;
        self.copy_progress_sync(None, 1, 1, false);
        Ok(())
    }

    fn directory_has_entry_name(
        fat: &mut crate::fat32::Fat32,
        dir_cluster: u32,
        name: &str,
        want_directory: Option<bool>,
    ) -> bool {
        use crate::fs::FileType;

        let Ok(entries) = fat.read_dir_entries(dir_cluster) else {
            return false;
        };
        for entry in entries.iter() {
            if !entry.valid {
                continue;
            }
            if let Some(is_dir) = want_directory {
                if is_dir && entry.file_type != FileType::Directory {
                    continue;
                }
                if !is_dir && entry.file_type != FileType::File {
                    continue;
                }
            }

            let full = entry.full_name();
            if entry.matches_name(name) || full.eq_ignore_ascii_case(name) {
                return true;
            }
        }
        false
    }

    fn ensure_copy_name_available_in_dir(
        fat: &mut crate::fat32::Fat32,
        dir_cluster: u32,
        base_name: &str,
        is_directory: bool,
    ) -> Result<String, String> {
        if !Self::directory_has_entry_name(fat, dir_cluster, base_name, Some(is_directory)) {
            return Ok(String::from(base_name));
        }

        Err(alloc::format!(
            "ya existe '{}' en destino; renombra o elimina antes de pegar",
            base_name
        ))
    }

    fn estimate_clipboard_item_work(
        &mut self,
        clip: &ExplorerClipboardState,
        dst_device_index: Option<usize>,
    ) -> Result<(usize, usize), String> {
        let source_item = ExplorerClipboardItem {
            source_device_index: clip.source_device_index,
            source_dir_cluster: clip.source_dir_cluster,
            source_dir_path: clip.source_dir_path.clone(),
            source_item_cluster: clip.source_item_cluster,
            source_is_directory: clip.source_is_directory,
            source_label: clip.source_label.clone(),
        };
        let source_device_index =
            self.resolve_clipboard_item_device_index(&source_item, clip.source_device_index);
        let cross_device = matches!(
            (source_device_index, dst_device_index),
            (Some(src), Some(dst)) if src != dst
        );

        if cross_device {
            let src_dev =
                source_device_index.ok_or_else(|| String::from("origen sin indice de unidad"))?;
            let mut src_fat = crate::fat32::Fat32::new();
            src_fat
                .mount_uefi_block_device(src_dev)
                .map_err(|e| alloc::format!("no se pudo montar unidad origen: {}", e))?;
            let source_dir_cluster = Self::resolve_directory_cluster_from_explorer_path(
                &mut src_fat,
                clip.source_dir_path.as_str(),
            )
            .unwrap_or(clip.source_dir_cluster);

            if clip.source_is_directory {
                let source_entry = Self::find_directory_entry_by_hint(
                    &mut src_fat,
                    source_dir_cluster,
                    clip.source_label.as_str(),
                    clip.source_item_cluster,
                )
                .map_err(String::from)?;
                let (child_units, child_items) =
                    Self::estimate_directory_tree_work(&mut src_fat, source_entry.cluster)?;
                let mut total_units = child_units.saturating_add(1);
                let mut total_items = child_items.saturating_add(1);
                if clip.mode == ExplorerClipboardMode::Cut {
                    let delete_units = total_items.max(1);
                    total_units = total_units.saturating_add(delete_units);
                    total_items = total_items.saturating_add(delete_units);
                }
                return Ok((total_units, total_items));
            }

            let source_entry = Self::find_file_entry_by_hint(
                &mut src_fat,
                source_dir_cluster,
                clip.source_label.as_str(),
                clip.source_item_cluster,
            )
            .map_err(String::from)?;
            let mut total_units = (source_entry.size as usize).max(1);
            let mut total_items = 1usize;
            if clip.mode == ExplorerClipboardMode::Cut {
                total_units = total_units.saturating_add(1);
                total_items = total_items.saturating_add(1);
            }
            return Ok((total_units, total_items));
        }

        let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
        let source_dir_cluster = Self::resolve_directory_cluster_from_explorer_path(
            fat,
            clip.source_dir_path.as_str(),
        )
        .unwrap_or(clip.source_dir_cluster);

        if clip.source_is_directory {
            let source_entry = Self::find_directory_entry_by_hint(
                fat,
                source_dir_cluster,
                clip.source_label.as_str(),
                clip.source_item_cluster,
            )
            .map_err(String::from)?;
            let (child_units, child_items) =
                Self::estimate_directory_tree_work(fat, source_entry.cluster)?;
            let mut total_units = child_units.saturating_add(1);
            let mut total_items = child_items.saturating_add(1);
            if clip.mode == ExplorerClipboardMode::Cut {
                let delete_units = total_items.max(1);
                total_units = total_units.saturating_add(delete_units);
                total_items = total_items.saturating_add(delete_units);
            }
            Ok((total_units, total_items))
        } else {
            let source_entry = Self::find_file_entry_by_hint(
                fat,
                source_dir_cluster,
                clip.source_label.as_str(),
                clip.source_item_cluster,
            )
            .map_err(String::from)?;
            let mut total_units = (source_entry.size as usize).max(1);
            let mut total_items = 1usize;
            if clip.mode == ExplorerClipboardMode::Cut {
                total_units = total_units.saturating_add(1);
                total_items = total_items.saturating_add(1);
            }
            Ok((total_units, total_items))
        }
    }

    fn estimate_clipboard_batch_work(
        &mut self,
        clip: &ExplorerClipboardState,
        dst_device_index: Option<usize>,
    ) -> (usize, usize) {
        let mut total_units = 0usize;
        let mut total_items = 0usize;

        for entry in Self::clipboard_items(clip).iter() {
            let resolved_source_device =
                self.resolve_clipboard_item_device_index(entry, entry.source_device_index.or(clip.source_device_index));
            let item_clip = ExplorerClipboardState {
                mode: clip.mode,
                source_device_index: resolved_source_device,
                source_dir_cluster: entry.source_dir_cluster,
                source_dir_path: entry.source_dir_path.clone(),
                source_item_cluster: entry.source_item_cluster,
                source_is_directory: entry.source_is_directory,
                source_label: entry.source_label.clone(),
                items: vec![entry.clone()],
            };
            match self.estimate_clipboard_item_work(&item_clip, dst_device_index) {
                Ok((units, items)) => {
                    total_units = total_units.saturating_add(units.max(1));
                    total_items = total_items.saturating_add(items.max(1));
                }
                Err(_) => {
                    total_units = total_units.saturating_add(1);
                    total_items = total_items.saturating_add(1);
                }
            }
        }

        (total_units.max(1), total_items.max(1))
    }

    fn refresh_explorer_windows_for_cluster(
        &mut self,
        cluster: u32,
        status: &str,
        skip_win_id: Option<usize>,
    ) {
        let mut refresh = Vec::new();
        for win in self.windows.iter() {
            if !win.is_explorer() {
                continue;
            }
            if skip_win_id == Some(win.id) {
                continue;
            }
            if win.explorer_current_cluster == cluster {
                refresh.push((win.id, win.explorer_path.clone()));
            }
        }

        for (id, path) in refresh.into_iter() {
            self.show_explorer_directory(id, cluster, path, String::from(status), None);
        }
    }

    fn execute_clipboard_paste_to_directory(
        &mut self,
        clip: &ExplorerClipboardState,
        dst_dir_cluster: u32,
    ) -> Result<(String, bool, bool), String> {
        let source_item = ExplorerClipboardItem {
            source_device_index: clip.source_device_index,
            source_dir_cluster: clip.source_dir_cluster,
            source_dir_path: clip.source_dir_path.clone(),
            source_item_cluster: clip.source_item_cluster,
            source_is_directory: clip.source_is_directory,
            source_label: clip.source_label.clone(),
        };
        let source_device_index =
            self.resolve_clipboard_item_device_index(&source_item, clip.source_device_index);
        let dst_device_index = self.current_volume_device_index;
        let cross_device = matches!(
            (source_device_index, dst_device_index),
            (Some(src), Some(dst)) if src != dst
        );

        if !cross_device && clip.source_is_directory && clip.source_item_cluster >= 2 {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            if Self::directory_is_descendant_of(fat, clip.source_item_cluster, dst_dir_cluster) {
                return Err(String::from(
                    "Pegar error: no puedes pegar una carpeta dentro de si misma.",
                ));
            }
        }
        self.copy_progress_abort_if_cancelled()?;

        let outcome = if cross_device {
            (|| -> Result<(String, bool, usize, usize, usize, bool, Option<String>), String> {
                let src_dev =
                    source_device_index.ok_or_else(|| String::from("origen sin indice de unidad"))?;

                let mut src_fat = crate::fat32::Fat32::new();
                src_fat
                    .mount_uefi_block_device(src_dev)
                    .map_err(|e| alloc::format!("no se pudo montar unidad origen: {}", e))?;
                let source_dir_cluster = Self::resolve_directory_cluster_from_explorer_path(
                    &mut src_fat,
                    clip.source_dir_path.as_str(),
                )
                .unwrap_or(clip.source_dir_cluster);

                if clip.source_is_directory {
                    let source_entry = Self::find_directory_entry_by_hint(
                        &mut src_fat,
                        source_dir_cluster,
                        clip.source_label.as_str(),
                        clip.source_item_cluster,
                    )
                    .map_err(String::from)?;

                    let source_short_name = Self::dir_entry_short_name(&source_entry);
                    let source_name_for_target =
                        Self::dir_entry_target_name(&source_entry, clip.source_label.as_str());
                    let dst_fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                    let target_name = if clip.mode == ExplorerClipboardMode::Copy {
                        Self::ensure_copy_name_available_in_dir(
                            dst_fat,
                            dst_dir_cluster,
                            source_name_for_target.as_str(),
                            true,
                        )?
                    } else {
                        source_name_for_target.clone()
                    };
                    let dst_root = dst_fat
                        .ensure_subdirectory(dst_dir_cluster, target_name.as_str())
                        .map_err(|e| alloc::format!("no se pudo preparar carpeta destino: {}", e))?;
                    self.copy_progress_touch(
                        alloc::format!("Preparando carpeta {}", target_name).as_str(),
                    );
                    self.copy_progress_sync(None, 1, 1, false);
                    let (files_copied, dirs_copied, bytes_copied) = self.copy_directory_tree_cross_device(
                        &mut src_fat,
                        source_entry.cluster,
                        dst_fat,
                        dst_root,
                    )?;

                    let mut cut_done = clip.mode != ExplorerClipboardMode::Cut;
                    let mut warning = None;
                    if clip.mode == ExplorerClipboardMode::Cut {
                        self.copy_progress_abort_if_cancelled()?;
                        match self.remove_directory_tree_in_dir_with_progress(
                            &mut src_fat,
                            source_dir_cluster,
                            source_short_name.as_str(),
                            source_entry.cluster,
                        ) {
                            Ok(()) => cut_done = true,
                            Err(e) => {
                                warning = Some(alloc::format!(
                                    "Aviso: pegado, pero no se pudo borrar origen ({}).",
                                    e
                                ));
                                cut_done = false;
                            }
                        }
                    }

                    return Ok((
                        target_name,
                        true,
                        files_copied,
                        dirs_copied.saturating_add(1),
                        bytes_copied,
                        cut_done,
                        warning,
                    ));
                }

                let source_entry = Self::find_file_entry_by_hint(
                    &mut src_fat,
                    source_dir_cluster,
                    clip.source_label.as_str(),
                    clip.source_item_cluster,
                )
                .map_err(String::from)?;

                let source_short_name = Self::dir_entry_short_name(&source_entry);
                let source_name_for_target =
                    Self::dir_entry_target_name(&source_entry, clip.source_label.as_str());
                let dst_fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                let target_name = if clip.mode == ExplorerClipboardMode::Copy {
                    Self::ensure_copy_name_available_in_dir(
                        dst_fat,
                        dst_dir_cluster,
                        source_name_for_target.as_str(),
                        false,
                    )?
                } else {
                    source_name_for_target.clone()
                };
                let read_len = self.copy_file_with_progress_cross_device(
                    &mut src_fat,
                    source_entry.cluster,
                    source_entry.size as usize,
                    dst_fat,
                    dst_dir_cluster,
                    target_name.as_str(),
                )?;
                self.copy_progress_abort_if_cancelled()?;

                let mut cut_done = clip.mode != ExplorerClipboardMode::Cut;
                let mut warning = None;
                if clip.mode == ExplorerClipboardMode::Cut {
                    self.copy_progress_touch(
                        alloc::format!("Eliminando origen {}", source_short_name).as_str(),
                    );
                    match src_fat.delete_file_in_dir(source_dir_cluster, source_short_name.as_str()) {
                        Ok(()) => cut_done = true,
                        Err(e) => {
                            warning = Some(alloc::format!(
                                "Aviso: pegado, pero no se pudo borrar origen ({}).",
                                e
                            ));
                            cut_done = false;
                        }
                    }
                    self.copy_progress_sync(None, 1, 1, false);
                }

                Ok((target_name, false, 1, 0, read_len, cut_done, warning))
            })()
        } else {
            (|| -> Result<(String, bool, usize, usize, usize, bool, Option<String>), String> {
                let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                let source_dir_cluster = Self::resolve_directory_cluster_from_explorer_path(
                    fat,
                    clip.source_dir_path.as_str(),
                )
                .unwrap_or(clip.source_dir_cluster);

                if clip.mode == ExplorerClipboardMode::Cut && source_dir_cluster == dst_dir_cluster {
                    return Err(String::from("Cortar: selecciona otra carpeta para pegar."));
                }

                if clip.source_is_directory {
                    let source_entry = Self::find_directory_entry_by_hint(
                        fat,
                        source_dir_cluster,
                        clip.source_label.as_str(),
                        clip.source_item_cluster,
                    )
                    .map_err(String::from)?;

                    if Self::directory_is_descendant_of(fat, source_entry.cluster, dst_dir_cluster) {
                        return Err(String::from("no puedes pegar una carpeta dentro de si misma."));
                    }

                    let source_short_name = Self::dir_entry_short_name(&source_entry);
                    let source_name_for_target =
                        Self::dir_entry_target_name(&source_entry, clip.source_label.as_str());
                    let target_name = if clip.mode == ExplorerClipboardMode::Copy {
                        Self::ensure_copy_name_available_in_dir(
                            fat,
                            dst_dir_cluster,
                            source_name_for_target.as_str(),
                            true,
                        )?
                    } else {
                        source_name_for_target.clone()
                    };
                    let dst_root = fat
                        .ensure_subdirectory(dst_dir_cluster, target_name.as_str())
                        .map_err(|e| alloc::format!("no se pudo preparar carpeta destino: {}", e))?;
                    if dst_root == source_entry.cluster {
                        return Err(String::from("carpeta destino invalida (misma carpeta)."));
                    }

                    self.copy_progress_touch(
                        alloc::format!("Preparando carpeta {}", target_name).as_str(),
                    );
                    self.copy_progress_sync(None, 1, 1, false);
                    let (files_copied, dirs_copied, bytes_copied) =
                        self.copy_directory_tree_same_device(fat, source_entry.cluster, dst_root)?;

                    let mut cut_done = clip.mode != ExplorerClipboardMode::Cut;
                    let mut warning = None;
                    if clip.mode == ExplorerClipboardMode::Cut {
                        self.copy_progress_abort_if_cancelled()?;
                        match self.remove_directory_tree_in_dir_with_progress(
                            fat,
                            source_dir_cluster,
                            source_short_name.as_str(),
                            source_entry.cluster,
                        ) {
                            Ok(()) => cut_done = true,
                            Err(e) => {
                                warning = Some(alloc::format!(
                                    "Aviso: pegado, pero no se pudo borrar origen ({}).",
                                    e
                                ));
                                cut_done = false;
                            }
                        }
                    }

                    return Ok((
                        target_name,
                        true,
                        files_copied,
                        dirs_copied.saturating_add(1),
                        bytes_copied,
                        cut_done,
                        warning,
                    ));
                }

                let source_entry = Self::find_file_entry_by_hint(
                    fat,
                    source_dir_cluster,
                    clip.source_label.as_str(),
                    clip.source_item_cluster,
                )
                .map_err(String::from)?;

                let source_short_name = Self::dir_entry_short_name(&source_entry);
                let source_name_for_target =
                    Self::dir_entry_target_name(&source_entry, clip.source_label.as_str());
                let target_name = if clip.mode == ExplorerClipboardMode::Copy {
                    Self::ensure_copy_name_available_in_dir(
                        fat,
                        dst_dir_cluster,
                        source_name_for_target.as_str(),
                        false,
                    )?
                } else {
                    source_name_for_target.clone()
                };
                let read_len = self.copy_file_with_progress_same_device(
                    fat,
                    source_entry.cluster,
                    source_entry.size as usize,
                    dst_dir_cluster,
                    target_name.as_str(),
                )?;
                self.copy_progress_abort_if_cancelled()?;

                let mut cut_done = clip.mode != ExplorerClipboardMode::Cut;
                let mut warning = None;
                if clip.mode == ExplorerClipboardMode::Cut {
                    self.copy_progress_touch(
                        alloc::format!("Eliminando origen {}", source_short_name).as_str(),
                    );
                    match fat.delete_file_in_dir(source_dir_cluster, source_short_name.as_str()) {
                        Ok(()) => cut_done = true,
                        Err(e) => {
                            warning = Some(alloc::format!(
                                "Aviso: pegado, pero no se pudo borrar origen ({}).",
                                e
                            ));
                            cut_done = false;
                        }
                    }
                    self.copy_progress_sync(None, 1, 1, false);
                }

                Ok((target_name, false, 1, 0, read_len, cut_done, warning))
            })()
        };

        let (source_name, is_directory, files_copied, dirs_copied, bytes_written, cut_done, warning) =
            outcome?;

        let mut status = if is_directory {
            if clip.mode == ExplorerClipboardMode::Cut && cut_done {
                alloc::format!(
                    "Carpeta movida: {} ({} carpetas, {} archivos, {} bytes).",
                    source_name, dirs_copied, files_copied, bytes_written
                )
            } else {
                alloc::format!(
                    "Carpeta pegada: {} ({} carpetas, {} archivos, {} bytes).",
                    source_name, dirs_copied, files_copied, bytes_written
                )
            }
        } else if clip.mode == ExplorerClipboardMode::Cut && cut_done {
            alloc::format!("Movido: {} ({} bytes).", source_name, bytes_written)
        } else {
            alloc::format!("Pegado: {} ({} bytes).", source_name, bytes_written)
        };
        if let Some(msg) = warning.as_ref() {
            status.push(' ');
            status.push_str(msg.as_str());
        }

        Ok((status, cut_done, cross_device))
    }

    fn finalize_clipboard_paste_job(&mut self, job: ClipboardPasteJob, cancelled: bool) {
        match job.target {
            ClipboardPasteTarget::ExplorerWindow(win_id) => {
                if cancelled {
                    let status = if job.ok_count > 0 {
                        alloc::format!(
                            "Operacion cancelada: {} completado(s), {} error(es).",
                            job.ok_count,
                            job.err_count
                        )
                    } else {
                        String::from("Operacion cancelada por usuario.")
                    };
                    self.show_explorer_directory(
                        win_id,
                        job.dst_dir_cluster,
                        job.dst_path,
                        status,
                        Some(job.dst_device_index),
                    );
                    return;
                }

                if job.ok_count == 0 && job.err_count > 0 {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        let head = job
                            .status_lines
                            .get(0)
                            .cloned()
                            .unwrap_or(String::from("no se pudo completar."));
                        win.set_explorer_status(
                            alloc::format!("Pegar error: {}.", Self::trim_ascii_line(head.as_str(), 72))
                                .as_str(),
                        );
                    }
                    return;
                }

                let status = if job.ok_count == 1 && job.err_count == 0 && !job.status_lines.is_empty() {
                    job.status_lines[0].clone()
                } else {
                    alloc::format!(
                        "Pegado completado: {} correcto(s), {} error(es).",
                        job.ok_count,
                        job.err_count
                    )
                };

                self.show_explorer_directory(
                    win_id,
                    job.dst_dir_cluster,
                    job.dst_path,
                    status,
                    Some(job.dst_device_index),
                );

                if job.clip.mode == ExplorerClipboardMode::Cut && job.cut_all_done {
                    self.explorer_clipboard = None;
                    for cluster in job.moved_sources.into_iter() {
                        self.refresh_explorer_windows_for_cluster(
                            cluster,
                            "Elemento movido a otra carpeta.",
                            Some(win_id),
                        );
                    }
                }
            }
            ClipboardPasteTarget::Desktop => {
                if cancelled {
                    self.desktop_surface_status = if job.ok_count > 0 {
                        alloc::format!(
                            "Operacion cancelada: {} completado(s), {} error(es).",
                            job.ok_count,
                            job.err_count
                        )
                    } else {
                        String::from("Operacion cancelada por usuario.")
                    };
                    self.refresh_explorer_windows_for_cluster(
                        job.dst_dir_cluster,
                        "Desktop parcialmente actualizado.",
                        None,
                    );
                    return;
                }

                if job.ok_count == 0 && job.err_count > 0 {
                    self.desktop_surface_status = String::from("Pegar error: no se pudo completar.");
                    return;
                }

                self.desktop_surface_status =
                    if job.ok_count == 1 && job.err_count == 0 && !job.first_status.is_empty() {
                        job.first_status
                    } else {
                        alloc::format!(
                            "Pegado completado: {} correcto(s), {} error(es).",
                            job.ok_count,
                            job.err_count
                        )
                    };
                self.refresh_explorer_windows_for_cluster(
                    job.dst_dir_cluster,
                    "Desktop actualizado.",
                    None,
                );

                if job.clip.mode == ExplorerClipboardMode::Cut && job.cut_all_done {
                    self.explorer_clipboard = None;
                    for cluster in job.moved_sources.into_iter() {
                        self.refresh_explorer_windows_for_cluster(
                            cluster,
                            "Elemento movido a Desktop.",
                            None,
                        );
                    }
                }
            }
        }
    }

    fn service_clipboard_paste_job(&mut self) {
        if self.clipboard_paste_job_busy {
            return;
        }
        let Some(mut job) = self.clipboard_paste_job.take() else {
            return;
        };

        self.clipboard_paste_job_busy = true;
        let mut cancelled = self.copy_progress_cancel_requested();
        let mut steps = 0usize;
        let start_tick = crate::timer::ticks();

        while !cancelled && steps < COPY_BACKGROUND_MAX_ITEMS_PER_PAINT && job.cursor < job.items.len() {
            let entry = job.items[job.cursor].clone();
            let resolved_source_device = self.resolve_clipboard_item_device_index(
                &entry,
                entry.source_device_index.or(job.clip.source_device_index),
            );
            let item_clip = ExplorerClipboardState {
                mode: job.clip.mode,
                source_device_index: resolved_source_device,
                source_dir_cluster: entry.source_dir_cluster,
                source_dir_path: entry.source_dir_path.clone(),
                source_item_cluster: entry.source_item_cluster,
                source_is_directory: entry.source_is_directory,
                source_label: entry.source_label.clone(),
                items: vec![entry.clone()],
            };

            if item_clip.source_device_index.is_none() {
                job.err_count += 1;
                if job.clip.mode == ExplorerClipboardMode::Cut {
                    job.cut_all_done = false;
                }
                if matches!(job.target, ClipboardPasteTarget::ExplorerWindow(_)) {
                    job.status_lines.push(alloc::format!(
                        "{}: origen no encontrado en volumenes disponibles.",
                        Self::trim_ascii_line(entry.source_label.as_str(), 22),
                    ));
                }
                job.cursor = job.cursor.saturating_add(1);
                steps = steps.saturating_add(1);
                if crate::timer::ticks().saturating_sub(start_tick) >= COPY_BACKGROUND_BUDGET_TICKS {
                    break;
                }
                continue;
            }

            let prev_volume = self.current_volume_device_index;
            self.current_volume_device_index = Some(job.dst_device_index);
            let item_result = self.execute_clipboard_paste_to_directory(&item_clip, job.dst_dir_cluster);
            self.current_volume_device_index = prev_volume;
            match item_result {
                Ok((status, cut_done, cross_device)) => {
                    job.ok_count += 1;
                    if matches!(job.target, ClipboardPasteTarget::ExplorerWindow(_)) {
                        job.status_lines.push(status.clone());
                    }
                    if job.first_status.is_empty() {
                        job.first_status = status;
                    }
                    if job.clip.mode == ExplorerClipboardMode::Cut {
                        if !cut_done {
                            job.cut_all_done = false;
                        }
                        if cut_done
                            && !cross_device
                            && entry.source_dir_cluster != job.dst_dir_cluster
                            && !job.moved_sources.iter().any(|c| *c == entry.source_dir_cluster)
                        {
                            job.moved_sources.push(entry.source_dir_cluster);
                        }
                    }
                }
                Err(err) => {
                    if Self::is_copy_cancel_error(err.as_str()) {
                        cancelled = true;
                        job.status_lines.push(String::from("Operacion cancelada por usuario."));
                        break;
                    }
                    job.err_count += 1;
                    if job.clip.mode == ExplorerClipboardMode::Cut {
                        job.cut_all_done = false;
                    }
                    if matches!(job.target, ClipboardPasteTarget::ExplorerWindow(_)) {
                        job.status_lines.push(alloc::format!(
                            "{}: {}",
                            Self::trim_ascii_line(entry.source_label.as_str(), 22),
                            err
                        ));
                    }
                }
            }

            job.cursor = job.cursor.saturating_add(1);
            steps = steps.saturating_add(1);
            if crate::timer::ticks().saturating_sub(start_tick) >= COPY_BACKGROUND_BUDGET_TICKS {
                break;
            }
        }

        let done = cancelled || job.cursor >= job.items.len();
        self.clipboard_paste_job_busy = false;
        if done {
            self.finish_copy_progress_prompt();
            self.finalize_clipboard_paste_job(job, cancelled);
        } else {
            self.clipboard_paste_job = Some(job);
        }
    }

    fn paste_explorer_clipboard(&mut self, win_id: usize) {
        if self.clipboard_paste_job.is_some() || self.copy_progress_prompt.is_some() {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("Pegar: ya hay una tarea de copia/movimiento en progreso.");
            }
            return;
        }

        let Some(clip) = self.explorer_clipboard.clone() else {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("Pegar: portapapeles vacio.");
            }
            return;
        };

        let (dst_dir_cluster, dst_path) = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => (win.explorer_current_cluster, win.explorer_path.clone()),
            None => return,
        };
        if dst_dir_cluster < 2 {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("Pegar: abre una carpeta de destino.");
            }
            return;
        }
        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        let Some(dst_device_index) = self.resolve_device_index_for_directory(
            dst_dir_cluster,
            Some(dst_path.as_str()),
            self.current_volume_device_index,
        ) else {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("Pegar error: no se pudo ubicar la unidad destino.");
            }
            return;
        };
        if !self.ensure_volume_index_mounted(dst_device_index) {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("Pegar error: no se pudo montar la unidad destino.");
            }
            return;
        }

        let clip_items = Self::clipboard_items(&clip);
        let (total_units, total_items) =
            self.estimate_clipboard_batch_work(&clip, Some(dst_device_index));
        let prompt_title = if clip.mode == ExplorerClipboardMode::Cut {
            "Moviendo elementos..."
        } else {
            "Copiando elementos..."
        };
        self.begin_copy_progress_prompt(prompt_title, total_units, total_items, false);
        self.clipboard_paste_job = Some(ClipboardPasteJob {
            target: ClipboardPasteTarget::ExplorerWindow(win_id),
            clip,
            items: clip_items,
            dst_dir_cluster,
            dst_path,
            dst_device_index,
            cursor: 0,
            ok_count: 0,
            err_count: 0,
            cut_all_done: true,
            moved_sources: Vec::new(),
            status_lines: Vec::new(),
            first_status: String::new(),
        });
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.set_explorer_status("Pegar: tarea iniciada en segundo plano.");
        }
    }

    fn paste_clipboard_to_desktop(&mut self) {
        if self.clipboard_paste_job.is_some() || self.copy_progress_prompt.is_some() {
            self.desktop_surface_status =
                String::from("Pegar: ya hay una tarea de copia/movimiento en progreso.");
            return;
        }

        let Some(clip) = self.explorer_clipboard.clone() else {
            self.desktop_surface_status = String::from("Pegar: portapapeles vacio.");
            return;
        };

        let (desktop_cluster, desktop_path) = match self.resolve_desktop_directory_target(true) {
            Ok(v) => v,
            Err(err) => {
                self.desktop_surface_status = err;
                return;
            }
        };
        let Some(desktop_device_index) = self.resolve_device_index_for_directory(
            desktop_cluster,
            Some(desktop_path.as_str()),
            self.current_volume_device_index,
        ) else {
            self.desktop_surface_status =
                String::from("Pegar error: no se pudo ubicar la unidad destino.");
            return;
        };
        if !self.ensure_volume_index_mounted(desktop_device_index) {
            self.desktop_surface_status =
                String::from("Pegar error: no se pudo montar la unidad destino.");
            return;
        }

        let clip_items = Self::clipboard_items(&clip);
        let (total_units, total_items) =
            self.estimate_clipboard_batch_work(&clip, Some(desktop_device_index));
        let prompt_title = if clip.mode == ExplorerClipboardMode::Cut {
            "Moviendo a Desktop..."
        } else {
            "Copiando a Desktop..."
        };
        self.begin_copy_progress_prompt(prompt_title, total_units, total_items, false);
        self.clipboard_paste_job = Some(ClipboardPasteJob {
            target: ClipboardPasteTarget::Desktop,
            clip,
            items: clip_items,
            dst_dir_cluster: desktop_cluster,
            dst_path: desktop_path,
            dst_device_index: desktop_device_index,
            cursor: 0,
            ok_count: 0,
            err_count: 0,
            cut_all_done: true,
            moved_sources: Vec::new(),
            status_lines: Vec::new(),
            first_status: String::new(),
        });
        self.desktop_surface_status = String::from("Pegar: tarea iniciada en segundo plano.");
    }

    fn delete_explorer_file(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) {
        if item.kind != ExplorerItemKind::File {
            return;
        }
        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        let dir_path = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => win.explorer_path.clone(),
            None => String::from("/"),
        };

        let deleted_name = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let source_entry = match Self::find_file_entry_by_hint(
                fat,
                source_dir_cluster,
                item.label.as_str(),
                item.cluster,
            ) {
                Ok(entry) => entry,
                Err(err) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status(alloc::format!("Eliminar error: {}", err).as_str());
                    }
                    return;
                }
            };

            let source_name = Self::dir_entry_short_name(&source_entry);
            fat.ensure_subdirectory(fat.root_cluster, "TRASH");
            let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
            let res = if trash_cluster >= 2 {
                let r = fat.move_entry(source_dir_cluster, trash_cluster, source_name.as_str());
                if r.is_ok() {
                    let loc_name = alloc::format!("{}.loc", source_name.as_str());
                    let loc_content = alloc::format!("{}", source_dir_cluster);
                    let _ = fat.write_text_file_in_dir_with_progress(trash_cluster, loc_name.as_str(), loc_content.as_bytes(), |_, _| true);
                }
                r
            } else {
                fat.delete_file_in_dir(source_dir_cluster, source_name.as_str())
            };

            if let Err(err) = res {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_explorer_status(alloc::format!("Eliminar error: {}", err).as_str());
                }
                return;
            }
            source_name
        };

        let clear_clipboard = self
            .explorer_clipboard
            .as_ref()
            .map(|clip| Self::clipboard_contains_item(clip, source_dir_cluster, item))
            .unwrap_or(false);
        if clear_clipboard {
            self.explorer_clipboard = None;
        }

        self.show_explorer_directory(
            win_id,
            source_dir_cluster,
            dir_path,
            alloc::format!("Eliminado: {}", deleted_name),
            None,
        );
    }

    fn delete_explorer_directory(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) {
        if item.kind != ExplorerItemKind::Directory {
            return;
        }
        if !Self::explorer_directory_can_delete(source_dir_cluster, item) {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("Esta carpeta de acceso directo no se puede eliminar.");
            }
            return;
        }
        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        let dir_path = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => win.explorer_path.clone(),
            None => String::from("/"),
        };

        let deleted_name = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let source_entry = match Self::find_directory_entry_by_hint(
                fat,
                source_dir_cluster,
                item.label.as_str(),
                item.cluster,
            ) {
                Ok(entry) => entry,
                Err(err) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status(alloc::format!("Eliminar carpeta error: {}", err).as_str());
                    }
                    return;
                }
            };

            let source_name = Self::dir_entry_short_name(&source_entry);
            fat.ensure_subdirectory(fat.root_cluster, "TRASH");
            let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
            let res = if trash_cluster >= 2 {
                let r = fat.move_entry(source_dir_cluster, trash_cluster, source_name.as_str());
                if r.is_ok() {
                    let loc_name = alloc::format!("{}.loc", source_name.as_str());
                    let loc_content = alloc::format!("{}", source_dir_cluster);
                    let _ = fat.write_text_file_in_dir_with_progress(trash_cluster, loc_name.as_str(), loc_content.as_bytes(), |_, _| true);
                }
                r
            } else {
                fat.delete_directory_in_dir(source_dir_cluster, source_name.as_str())
            };

            if let Err(err) = res {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_explorer_status(alloc::format!("Eliminar carpeta error: {}", err).as_str());
                }
                return;
            }
            source_name
        };

        let clear_clipboard = self
            .explorer_clipboard
            .as_ref()
            .map(|clip| {
                Self::clipboard_contains_item(clip, source_dir_cluster, item)
                    && Self::clipboard_items(clip)
                        .iter()
                        .any(|entry| entry.source_is_directory)
            })
            .unwrap_or(false);
        if clear_clipboard {
            self.explorer_clipboard = None;
        }

        self.show_explorer_directory(
            win_id,
            source_dir_cluster,
            dir_path,
            alloc::format!("Carpeta eliminada: {}", deleted_name),
            None,
        );
    }

    fn delete_desktop_file(&mut self, source_dir_cluster: u32, item: &ExplorerItem) {
        if item.kind != ExplorerItemKind::File {
            return;
        }
        if !self.ensure_fat_ready() {
            self.desktop_surface_status = if self.manual_unmount_lock {
                String::from("Volume desmontado. No se puede eliminar.")
            } else {
                String::from("FAT32 no disponible para eliminar.")
            };
            return;
        }

        let deleted_name = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let source_entry = match Self::find_file_entry_by_hint(
                fat,
                source_dir_cluster,
                item.label.as_str(),
                item.cluster,
            ) {
                Ok(entry) => entry,
                Err(err) => {
                    self.desktop_surface_status = alloc::format!("Eliminar error: {}", err);
                    return;
                }
            };

            let source_name = Self::dir_entry_short_name(&source_entry);
            fat.ensure_subdirectory(fat.root_cluster, "TRASH");
            let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
            let res = if trash_cluster >= 2 {
                let r = fat.move_entry(source_dir_cluster, trash_cluster, source_name.as_str());
                if r.is_ok() {
                    let loc_name = alloc::format!("{}.loc", source_name.as_str());
                    let loc_content = alloc::format!("{}", source_dir_cluster);
                    let _ = fat.write_text_file_in_dir_with_progress(trash_cluster, loc_name.as_str(), loc_content.as_bytes(), |_, _| true);
                }
                r
            } else {
                fat.delete_file_in_dir(source_dir_cluster, source_name.as_str())
            };

            if let Err(err) = res {
                self.desktop_surface_status = alloc::format!("Eliminar error: {}", err);
                return;
            }
            source_name
        };

        let clear_clipboard = self
            .explorer_clipboard
            .as_ref()
            .map(|clip| Self::clipboard_contains_item(clip, source_dir_cluster, item))
            .unwrap_or(false);
        if clear_clipboard {
            self.explorer_clipboard = None;
        }

        self.desktop_surface_status = alloc::format!("Eliminado: {}", deleted_name);
        let refresh_note = alloc::format!("Elemento eliminado: {}", deleted_name);
        self.refresh_explorer_windows_for_cluster(source_dir_cluster, refresh_note.as_str(), None);
    }

    fn delete_desktop_directory(&mut self, source_dir_cluster: u32, item: &ExplorerItem) {
        if item.kind != ExplorerItemKind::Directory {
            return;
        }
        if !Self::explorer_directory_can_delete(source_dir_cluster, item) {
            self.desktop_surface_status =
                String::from("Esta carpeta de acceso directo no se puede eliminar.");
            return;
        }
        if !self.ensure_fat_ready() {
            self.desktop_surface_status = if self.manual_unmount_lock {
                String::from("Volume desmontado. No se puede eliminar carpeta.")
            } else {
                String::from("FAT32 no disponible para eliminar carpeta.")
            };
            return;
        }

        let deleted_name = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let source_entry = match Self::find_directory_entry_by_hint(
                fat,
                source_dir_cluster,
                item.label.as_str(),
                item.cluster,
            ) {
                Ok(entry) => entry,
                Err(err) => {
                    self.desktop_surface_status =
                        alloc::format!("Eliminar carpeta error: {}", err);
                    return;
                }
            };

            let source_name = Self::dir_entry_short_name(&source_entry);
            fat.ensure_subdirectory(fat.root_cluster, "TRASH");
            let trash_cluster = fat.resolve_path(fat.root_cluster, "TRASH/").map(|(_, c)| c).unwrap_or(0);
            let res = if trash_cluster >= 2 {
                let r = fat.move_entry(source_dir_cluster, trash_cluster, source_name.as_str());
                if r.is_ok() {
                    let loc_name = alloc::format!("{}.loc", source_name.as_str());
                    let loc_content = alloc::format!("{}", source_dir_cluster);
                    let _ = fat.write_text_file_in_dir_with_progress(trash_cluster, loc_name.as_str(), loc_content.as_bytes(), |_, _| true);
                }
                r
            } else {
                fat.delete_directory_in_dir(source_dir_cluster, source_name.as_str())
            };

            if let Err(err) = res {
                self.desktop_surface_status = alloc::format!("Eliminar carpeta error: {}", err);
                return;
            }
            source_name
        };

        let clear_clipboard = self
            .explorer_clipboard
            .as_ref()
            .map(|clip| {
                Self::clipboard_contains_item(clip, source_dir_cluster, item)
                    && Self::clipboard_items(clip)
                        .iter()
                        .any(|entry| entry.source_is_directory)
            })
            .unwrap_or(false);
        if clear_clipboard {
            self.explorer_clipboard = None;
        }

        self.desktop_surface_status = alloc::format!("Carpeta eliminada: {}", deleted_name);
        let refresh_note = alloc::format!("Carpeta eliminada: {}", deleted_name);
        self.refresh_explorer_windows_for_cluster(source_dir_cluster, refresh_note.as_str(), None);
    }

    fn extract_zip_from_directory(
        &mut self,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) -> Result<(String, usize, usize, usize), String> {
        let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
        let source_entry = Self::find_file_entry_by_hint(
            fat,
            source_dir_cluster,
            item.label.as_str(),
            item.cluster,
        )
        .map_err(String::from)?;

        if source_entry.size == 0 {
            return Err(String::from("ZIP vacio."));
        }
        if source_entry.size as usize > COPY_MAX_FILE_BYTES {
            return Err(alloc::format!(
                "ZIP demasiado grande (max {} bytes).",
                COPY_MAX_FILE_BYTES
            ));
        }

        let source_name = Self::dir_entry_short_name(&source_entry);
        let mut zip_raw = Self::try_alloc_zeroed(source_entry.size as usize).map_err(String::from)?;
        let read_len = fat
            .read_file_sized(source_entry.cluster, source_entry.size as usize, &mut zip_raw)
            .map_err(|e| alloc::format!("no se pudo leer ZIP: {}", e))?;
        zip_raw.truncate(read_len);

        if zip_raw.len() < 4 || &zip_raw[0..2] != b"PK" {
            return Err(String::from("ZIP invalido (firma)."));
        }

        let (mut central_entries, central_offset) = Self::parse_zip_central_directory(zip_raw.as_slice())
            .ok_or_else(|| String::from("ZIP corrupto (central directory)."))?;
        central_entries.sort_by_key(|e| e.0);

        let archive_tag4 =
            Self::sanitize_short_component(Self::filename_stem(source_name.as_str()), 4, "ZIP");

        let mut cursor = 0usize;
        let mut parsed_headers = 0usize;
        let mut output_index = 0usize;
        let mut extracted = 0usize;
        let mut skipped = 0usize;
        let mut errors = 0usize;

        while cursor + 4 <= zip_raw.len() {
            let local_offset = cursor;
            let sig = u32::from_le_bytes([
                zip_raw[cursor],
                zip_raw[cursor + 1],
                zip_raw[cursor + 2],
                zip_raw[cursor + 3],
            ]);

            if sig == 0x0201_4B50 || sig == 0x0605_4B50 {
                break;
            }
            if sig != 0x0403_4B50 {
                return Err(String::from("ZIP corrupto (local header)."));
            }
            parsed_headers += 1;
            cursor += 4;

            let _version = Self::read_u16_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (version)."))?;
            let flags = Self::read_u16_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (flags)."))?;
            let mut method = Self::read_u16_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (method)."))?;
            let _mod_time = Self::read_u16_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (mod time)."))?;
            let _mod_date = Self::read_u16_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (mod date)."))?;
            let _crc32 = Self::read_u32_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (crc32)."))?;
            let mut comp_size = Self::read_u32_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (comp size)."))?
                as usize;
            let mut uncomp_size = Self::read_u32_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (uncomp size)."))?
                as usize;
            let name_len = Self::read_u16_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (name len)."))?
                as usize;
            let extra_len = Self::read_u16_le(zip_raw.as_slice(), &mut cursor)
                .ok_or_else(|| String::from("ZIP corrupto (extra len)."))?
                as usize;

            if (flags & 0x0008) != 0 {
                let Some((_, cd_comp, cd_uncomp, cd_method)) =
                    central_entries.iter().find(|entry| entry.0 == local_offset)
                else {
                    return Err(String::from("ZIP descriptor sin entrada central."));
                };
                comp_size = *cd_comp;
                uncomp_size = *cd_uncomp;
                method = *cd_method;
            }

            if cursor + name_len > zip_raw.len() {
                return Err(String::from("ZIP corrupto (file name)."));
            }
            let name_bytes = &zip_raw[cursor..cursor + name_len];
            cursor += name_len;

            if cursor + extra_len > zip_raw.len() {
                return Err(String::from("ZIP corrupto (extra data)."));
            }
            cursor += extra_len;

            if cursor + comp_size > zip_raw.len() {
                return Err(String::from("ZIP corrupto (file payload)."));
            }
            let payload = &zip_raw[cursor..cursor + comp_size];
            cursor += comp_size;

            if (flags & 0x0008) != 0 {
                let mut next_local_offset = central_offset;
                for entry in central_entries.iter() {
                    if entry.0 > local_offset
                        && (next_local_offset == central_offset || entry.0 < next_local_offset)
                    {
                        next_local_offset = entry.0;
                    }
                }
                if next_local_offset < cursor || next_local_offset > zip_raw.len() {
                    return Err(String::from("ZIP descriptor fuera de rango."));
                }
                cursor = next_local_offset;
            }

            let path_text = String::from_utf8_lossy(name_bytes).into_owned();
            if path_text.ends_with('/') || path_text.ends_with('\\') {
                skipped += 1;
                continue;
            }
            if !Self::is_installable_zip_path(path_text.as_str()) {
                skipped += 1;
                continue;
            }
            if Self::linux_path_leaf(path_text.as_str()).trim().is_empty() {
                skipped += 1;
                continue;
            }

            let payload_buf: Option<Vec<u8>> = match method {
                0 => {
                    if comp_size != uncomp_size {
                        errors += 1;
                        continue;
                    }
                    None
                }
                8 => {
                    if uncomp_size > INSTALL_MAX_EXPANDED_FILE_BYTES {
                        errors += 1;
                        continue;
                    }
                    let inflate_limit = if uncomp_size == 0 {
                        INSTALL_MAX_EXPANDED_FILE_BYTES
                    } else {
                        uncomp_size
                    };
                    match decompress_to_vec_with_limit(payload, inflate_limit) {
                        Ok(raw) => {
                            if uncomp_size != 0 && raw.len() != uncomp_size {
                                errors += 1;
                                continue;
                            }
                            Some(raw)
                        }
                        Err(_) => {
                            errors += 1;
                            continue;
                        }
                    }
                }
                _ => {
                    errors += 1;
                    continue;
                }
            };

            output_index += 1;
            let out_name =
                Self::short_install_name(archive_tag4.as_str(), path_text.as_str(), output_index);
            let payload_out: &[u8] = match payload_buf.as_ref() {
                Some(v) => v.as_slice(),
                None => payload,
            };

            match fat.write_text_file_in_dir(source_dir_cluster, out_name.as_str(), payload_out) {
                Ok(()) => extracted += 1,
                Err(_) => errors += 1,
            }
        }

        if parsed_headers == 0 {
            return Err(String::from("ZIP sin entradas de archivo."));
        }

        Ok((source_name, extracted, skipped, errors))
    }

    fn extract_zip_in_current_directory(
        &mut self,
        win_id: usize,
        source_dir_cluster: u32,
        item: &ExplorerItem,
    ) {
        if item.kind != ExplorerItemKind::File || !Self::explorer_item_is_zip(item) {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("Extraer: selecciona un archivo .zip.");
            }
            return;
        }
        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        let dir_path = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => win.explorer_path.clone(),
            None => String::from("/"),
        };

        let (source_name, extracted, skipped, errors) =
            match self.extract_zip_from_directory(source_dir_cluster, item) {
                Ok(v) => v,
                Err(err) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status(alloc::format!("Extraer error: {}", err).as_str());
                    }
                    return;
                }
            };

        self.show_explorer_directory(
            win_id,
            source_dir_cluster,
            dir_path,
            alloc::format!(
                "ZIP {}: {} extraidos, {} omitidos, {} con error.",
                source_name,
                extracted,
                skipped,
                errors
            ),
            None,
        );
    }

    fn extract_zip_on_desktop(&mut self, source_dir_cluster: u32, item: &ExplorerItem) {
        if item.kind != ExplorerItemKind::File || !Self::explorer_item_is_zip(item) {
            self.desktop_surface_status = String::from("Extraer: selecciona un archivo .zip.");
            return;
        }
        if !self.ensure_fat_ready() {
            self.desktop_surface_status = if self.manual_unmount_lock {
                String::from("Volume desmontado. No se puede extraer ZIP.")
            } else {
                String::from("FAT32 no disponible para extraer ZIP.")
            };
            return;
        }

        let (source_name, extracted, skipped, errors) =
            match self.extract_zip_from_directory(source_dir_cluster, item) {
                Ok(v) => v,
                Err(err) => {
                    self.desktop_surface_status = alloc::format!("Extraer error: {}", err);
                    return;
                }
            };

        self.desktop_surface_status = alloc::format!(
            "ZIP {}: {} extraidos, {} omitidos, {} con error.",
            source_name,
            extracted,
            skipped,
            errors
        );
        self.refresh_explorer_windows_for_cluster(source_dir_cluster, "ZIP extraido.", None);
    }

    fn begin_move_capture(&mut self, win_id: usize, mouse_x: i32, mouse_y: i32) {
        if let Some(win) = self.windows.iter().find(|w| w.id == win_id) {
            self.pointer_capture = Some(WindowPointerCapture::Move(WindowMoveCapture {
                win_id,
                grab_offset_x: mouse_x - win.rect.x,
                grab_offset_y: mouse_y - win.rect.y,
            }));
        }
    }

    fn begin_resize_capture(&mut self, win_id: usize, mouse_x: i32, mouse_y: i32) {
        if let Some(win) = self.windows.iter().find(|w| w.id == win_id) {
            self.pointer_capture = Some(WindowPointerCapture::Resize(WindowResizeCapture {
                win_id,
                start_mouse_x: mouse_x,
                start_mouse_y: mouse_y,
                start_width: win.rect.width,
                start_height: win.rect.height,
            }));
        }
    }

    fn update_pointer_capture(&mut self, mouse_x: i32, mouse_y: i32, left_down: bool) -> bool {
        if !left_down {
            self.pointer_capture = None;
            return false;
        }

        let capture = match self.pointer_capture {
            Some(c) => c,
            None => return false,
        };

        let screen_w = self.width as i32;
        let taskbar_top = self.taskbar.rect.y;

        match capture {
            WindowPointerCapture::Move(c) => {
                let Some(win) = self.windows.iter_mut().find(|w| w.id == c.win_id) else {
                    self.pointer_capture = None;
                    return false;
                };

                if win.state != WindowState::Normal {
                    self.pointer_capture = None;
                    return false;
                }

                let max_x = (screen_w - win.rect.width as i32).max(0);
                let max_y = (taskbar_top - win.rect.height as i32).max(0);
                let new_x = (mouse_x - c.grab_offset_x).clamp(0, max_x);
                let new_y = (mouse_y - c.grab_offset_y).clamp(0, max_y);
                win.move_to(new_x, new_y);
                true
            }
            WindowPointerCapture::Resize(c) => {
                let Some(win) = self.windows.iter_mut().find(|w| w.id == c.win_id) else {
                    self.pointer_capture = None;
                    return false;
                };

                if win.state != WindowState::Normal {
                    self.pointer_capture = None;
                    return false;
                }

                let (mut min_w, mut min_h) = win.min_dimensions();
                if min_w == 0 {
                    min_w = WINDOW_MIN_FALLBACK_W;
                }
                if min_h == 0 {
                    min_h = WINDOW_MIN_FALLBACK_H;
                }

                let max_w = (screen_w - win.rect.x).max(min_w as i32);
                let max_h = (taskbar_top - win.rect.y).max(min_h as i32);

                let target_w = (c.start_width as i32 + (mouse_x - c.start_mouse_x))
                    .clamp(min_w as i32, max_w) as u32;
                let target_h = (c.start_height as i32 + (mouse_y - c.start_mouse_y))
                    .clamp(min_h as i32, max_h) as u32;

                win.resize_to(target_w, target_h);
                true
            }
        }
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Mouse(m) => {
                let _ = crate::syscall::linux_gfx_bridge_push_pointer_event(
                    m.x,
                    m.y,
                    m.left_down,
                    m.right_down,
                );
                self.mouse_pos = Point { x: m.x, y: m.y };

                let was_left_down = self.last_mouse_down;
                let was_right_down = self.last_mouse_right_down;
                self.last_mouse_down = m.left_down;
                self.last_mouse_right_down = m.right_down;
                let is_new_left_click = m.left_down && !was_left_down;
                let is_new_right_click = m.right_down && !was_right_down;

                if self.copy_progress_prompt.is_some() {
                    if is_new_left_click
                        && self.handle_copy_progress_prompt_click(m.x, m.y)
                    {
                        return;
                    }
                    if self.copy_progress_prompt_is_modal() {
                        return;
                    }
                }

                if self.rename_prompt.is_some() {
                    if is_new_left_click {
                        self.handle_rename_prompt_click(m.x, m.y);
                    }
                    return;
                }

                if self.update_pointer_capture(m.x, m.y, m.left_down) {
                    return;
                }

                if self.desktop_create_folder.is_none()
                    && self.rename_prompt.is_none()
                    && self.update_desktop_drag(m.x, m.y, m.left_down)
                {
                    return;
                }

                if self.handle_notepad_save_prompt_wheel(m.x, m.y, m.wheel_delta) {
                    return;
                }

                if is_new_right_click {
                    self.handle_right_click(m.x, m.y);
                    return;
                }

                if !is_new_left_click {
                    return;
                }

                if self.desktop_create_folder.is_some() {
                    self.handle_desktop_create_folder_click(m.x, m.y);
                    return;
                }

                if self.notepad_save_prompt.is_some() {
                    self.handle_notepad_save_prompt_click(m.x, m.y);
                    return;
                }

                if self.handle_explorer_context_menu_left_click() {
                    return;
                }

                if self.handle_desktop_context_menu_left_click() {
                    return;
                }

                if self.taskbar.rect.contains(self.mouse_pos) {
                    let rel_x = self.mouse_pos.x - self.taskbar.rect.x;
                    let rel_y = self.mouse_pos.y - self.taskbar.rect.y;

                    if self
                        .taskbar
                        .start_button
                        .rect
                        .contains(Point { x: rel_x, y: rel_y })
                    {
                        self.taskbar.start_menu_open = !self.taskbar.start_menu_open;
                        if self.taskbar.start_menu_open {
                            self.refresh_start_app_shortcuts();
                        } else {
                            self.start_tools_open = false;
                            self.start_games_open = false;
                            self.start_apps_open = false;
                        }
                        return;
                    }

                    let tabs_start_x = 90;
                    if rel_x >= tabs_start_x {
                        let btn_index = (rel_x - tabs_start_x) / 120;
                        if btn_index >= 0 && (btn_index as usize) < self.minimized_windows.len() {
                            let win_id = self.minimized_windows[btn_index as usize].0;
                            self.restore_window(win_id);
                            return;
                        }
                    }

                    return;
                }

                if self.taskbar.start_menu_open {
                    if self.start_tools_open {
                        let tools_rect = self.tools_menu_rect();
                        if tools_rect.contains(self.mouse_pos) {
                            let notepad_item = self.tools_menu_item_rect(0);
                            if notepad_item.contains(self.mouse_pos) {
                                self.open_notepad_blank();
                                self.taskbar.start_menu_open = false;
                                self.start_tools_open = false;
                                self.start_games_open = false;
                                self.start_apps_open = false;
                            }
                            let shell_item = self.tools_menu_item_rect(1);
                            if shell_item.contains(self.mouse_pos) {
                                self.taskbar.start_menu_open = false;
                                self.start_tools_open = false;
                                self.start_games_open = false;
                                self.start_apps_open = false;

                                let launch_result = crate::launch_uefi_shell();
                                let _ = crate::restore_gui_after_external_app();
                                match launch_result {
                                    Ok(_) => {}
                                    Err(err) => {
                                        let term_id =
                                            self.create_window("Terminal Shell", 100, 100, 800, 500);
                                        if let Some(win) =
                                            self.windows.iter_mut().find(|w| w.id == term_id)
                                        {
                                            win.add_output(
                                                alloc::format!(
                                                    "UEFI Shell: no pudo iniciar: {}",
                                                    err
                                                )
                                                .as_str(),
                                            );
                                            win.render_terminal();
                                        }
                                    }
                                }
                            }
                            return;
                        }
                    }

                    if self.start_games_open {
                        let games_rect = self.games_menu_rect();
                        if games_rect.contains(self.mouse_pos) {
                            let doom_item = self.games_menu_item_rect(0);
                            if doom_item.contains(self.mouse_pos) {
                                self.create_doom_launcher_window(
                                    "Juegos - DOOM",
                                    220,
                                    90,
                                    660,
                                    430,
                                );
                                self.taskbar.start_menu_open = false;
                                self.start_games_open = false;
                                self.start_tools_open = false;
                                self.start_apps_open = false;
                            }
                            return;
                        }
                    }

                    if self.start_apps_open {
                        let apps_rect = self.apps_menu_rect();
                        if apps_rect.contains(self.mouse_pos) {
                            if !self.start_app_shortcuts.is_empty() {
                                let mut selected: Option<StartAppShortcut> = None;
                                let visible = self.apps_menu_item_count();
                                for idx in 0..visible {
                                    let item_rect = self.apps_menu_item_rect(idx);
                                    if item_rect.contains(self.mouse_pos) {
                                        if let Some(shortcut) = self.start_app_shortcuts.get(idx) {
                                            selected = Some(shortcut.clone());
                                        }
                                        break;
                                    }
                                }
                                if let Some(shortcut) = selected {
                                    self.taskbar.start_menu_open = false;
                                    self.start_games_open = false;
                                    self.start_tools_open = false;
                                    self.start_apps_open = false;
                                    self.launch_start_app_shortcut(&shortcut);
                                }
                            }
                            return;
                        }
                    }

                    let menu_rect = self.start_menu_rect();
                    if menu_rect.contains(self.mouse_pos) {
                        let terminal_item = self.start_menu_item_rect(0);
                        let explorer_item = self.start_menu_item_rect(1);
                        let browser_item = self.start_menu_item_rect(2);
                        let settings_item = self.start_menu_item_rect(3);
                        let tools_item = self.start_menu_item_rect(4);
                        let games_item = self.start_menu_item_rect(5);
                        let apps_item = self.start_menu_item_rect(6);
                        let shutdown_item = self.start_menu_item_rect(7);

                        if terminal_item.contains(self.mouse_pos) {
                            self.create_window("Terminal Shell", 100, 100, 800, 500);
                            self.taskbar.start_menu_open = false;
                            self.start_tools_open = false;
                            self.start_games_open = false;
                            self.start_apps_open = false;
                        } else if explorer_item.contains(self.mouse_pos) {
                            self.create_explorer_window("File Explorer", 140, 80, 920, 580);
                            self.taskbar.start_menu_open = false;
                            self.start_tools_open = false;
                            self.start_games_open = false;
                            self.start_apps_open = false;
                        } else if browser_item.contains(self.mouse_pos) {
                            self.create_browser_window("Redux Browser", 180, 60, 800, 500);
                            self.taskbar.start_menu_open = false;
                            self.start_tools_open = false;
                            self.start_games_open = false;
                            self.start_apps_open = false;
                        } else if settings_item.contains(self.mouse_pos) {
                            self.create_settings_window("Configuracion", 200, 100, 500, 400);
                            self.taskbar.start_menu_open = false;
                            self.start_tools_open = false;
                            self.start_games_open = false;
                            self.start_apps_open = false;
                        } else if tools_item.contains(self.mouse_pos) {
                            self.start_tools_open = !self.start_tools_open;
                            self.start_games_open = false;
                            self.start_apps_open = false;
                        } else if games_item.contains(self.mouse_pos) {
                            self.start_games_open = !self.start_games_open;
                            self.start_tools_open = false;
                            self.start_apps_open = false;
                        } else if apps_item.contains(self.mouse_pos) {
                            self.refresh_start_app_shortcuts();
                            self.start_apps_open = !self.start_apps_open;
                            self.start_tools_open = false;
                            self.start_games_open = false;
                        } else if shutdown_item.contains(self.mouse_pos) {
                            uefi::runtime::reset(ResetType::SHUTDOWN, Status::SUCCESS, None);
                        }
                        return;
                    }
                }

                self.taskbar.start_menu_open = false;
                self.start_tools_open = false;
                self.start_games_open = false;
                self.start_apps_open = false;

                for i in (0..self.windows.len()).rev() {
                    if self.windows[i].state != WindowState::Normal
                        && self.windows[i].state != WindowState::Maximized
                    {
                        continue;
                    }

                    if self.windows[i].rect.contains(self.mouse_pos) {
                        let win_id = self.windows[i].id;
                        self.active_window_id = Some(win_id);

                        let top_idx = if i < self.windows.len() - 1 {
                            let win = self.windows.remove(i);
                            self.windows.push(win);
                            self.windows.len() - 1
                        } else {
                            i
                        };

                        if let Some(control) =
                            self.windows[top_idx].hit_test_controls(self.mouse_pos.x, self.mouse_pos.y)
                        {
                            match control {
                                "close" => self.close_window(win_id),
                                "minimize" => self.minimize_window(win_id),
                                "maximize" => {
                                    let w = self.width;
                                    let h = self.height;
                                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                        win.maximize(w, h);
                                    }
                                }
                                _ => {}
                            }
                            return;
                        }

                        if self.windows[top_idx].state == WindowState::Normal {
                            if self.windows[top_idx]
                                .resize_grip_contains(self.mouse_pos.x, self.mouse_pos.y)
                            {
                                self.begin_resize_capture(win_id, self.mouse_pos.x, self.mouse_pos.y);
                                return;
                            }

                            if self.windows[top_idx]
                                .title_bar_contains(self.mouse_pos.x, self.mouse_pos.y)
                            {
                                self.begin_move_capture(win_id, self.mouse_pos.x, self.mouse_pos.y);
                                return;
                            }
                        }

                        self.handle_explorer_click(win_id, self.mouse_pos.x, self.mouse_pos.y);
                        self.handle_notepad_click(win_id, self.mouse_pos.x, self.mouse_pos.y);
                        self.handle_browser_click(win_id, self.mouse_pos.x, self.mouse_pos.y);
                        self.handle_doom_launcher_click(win_id, self.mouse_pos.x, self.mouse_pos.y);
                        return;
                    }
                }

                if self.handle_desktop_usb_left_click() {
                    return;
                }

                if self.handle_desktop_surface_left_click(self.mouse_pos.x, self.mouse_pos.y) {
                    return;
                }
            }
            Event::Keyboard(k) => {
                if self.handle_copy_progress_prompt_key(k.key, k.down) {
                    return;
                }
                if self.handle_rename_prompt_key(k.key, k.down) {
                    return;
                }
                if self.handle_desktop_create_folder_key(k.key, k.down) {
                    return;
                }
                if self.handle_notepad_save_prompt_key(k.key, k.special, k.down) {
                    return;
                }
                if let Some(ch) = k.key {
                    let _ = crate::syscall::linux_gfx_bridge_push_key_event(ch, k.down);
                }
                let mut effective_active_id = self.active_window_id;
                if let Some(candidate_id) = effective_active_id {
                    let (cand_terminal, cand_notepad, cand_browser) =
                        match self.windows.iter().find(|w| w.id == candidate_id) {
                            Some(w) => (w.is_terminal(), w.is_notepad(), w.is_browser()),
                            None => (false, false, false),
                        };
                    if !cand_terminal && !cand_notepad && !cand_browser {
                        if let Some(run) = self.linux_runloop_container.as_ref() {
                            if run.active {
                                if let Some(run_win) = self.windows.iter().find(|w| w.id == run.win_id) {
                                    if run_win.is_terminal() {
                                        effective_active_id = Some(run.win_id);
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(active_id) = effective_active_id {
                    if !k.down {
                        return;
                    }

                    let (is_terminal, is_notepad, is_browser) = match self.windows.iter().find(|w| w.id == active_id) {
                        Some(w) => (w.is_terminal(), w.is_notepad(), w.is_browser()),
                        None => (false, false, false),
                    };

                    if !is_terminal && !is_notepad && !is_browser {
                        return;
                    }

                    if is_browser
                        && matches!(self.web_backend_mode, WebBackendMode::Vaev)
                        && crate::web_vaev_bridge::input_enabled()
                    {
                        if let Some(special) = k.special {
                            let mapped = match special {
                                SpecialKey::Up => Some(
                                    crate::web_vaev_bridge::VaevInputEvent::Scroll { delta: -120 },
                                ),
                                SpecialKey::Down => Some(
                                    crate::web_vaev_bridge::VaevInputEvent::Scroll { delta: 120 },
                                ),
                                SpecialKey::Left => {
                                    Some(crate::web_vaev_bridge::VaevInputEvent::Back)
                                }
                                SpecialKey::Right => {
                                    Some(crate::web_vaev_bridge::VaevInputEvent::Forward)
                                }
                            };
                            if let Some(event) = mapped {
                                self.browser_vaev_dispatch_input(active_id, event);
                                return;
                            }
                        }
                    }

                    if let Some(ch) = k.key {
                        if ch == '\n' || ch == '\r' {
                            let mut cmd_to_run = None;
                            if let Some(win) = self.windows.iter_mut().find(|w| w.id == active_id) {
                                cmd_to_run = win.handle_enter();
                            }
                            if is_terminal {
                                if let Some(cmd) = cmd_to_run {
                                    self.execute_command(active_id, &cmd);
                                }
                            } else if is_browser {
                                if let Some(url) = cmd_to_run {
                                    self.browser_navigate_to(active_id, url.as_str());
                                }
                            }
                        } else if ch == '\x08' || ch == '\x7f' {
                            if let Some(win) = self.windows.iter_mut().find(|w| w.id == active_id) {
                                win.handle_backspace();
                            }
                        } else if let Some(win) = self.windows.iter_mut().find(|w| w.id == active_id) {
                            win.handle_char(ch);
                        }
                    }
                }
            }
        }
    }

    pub fn paint(&mut self) {
        self.service_linux_runloop_container();
        self.service_linux_step_container();
        self.service_linux_bridge_window();
        framebuffer::clear(0x021F3F);
        self.refresh_desktop_usb_state_if_needed(false);
        self.draw_desktop_usb_overlay();
        self.draw_desktop_surface_overlay();

        for win in &self.windows {
            if win.state != WindowState::Normal && win.state != WindowState::Maximized {
                continue;
            }

            framebuffer::rect(
                win.rect.x as usize,
                win.rect.y as usize,
                win.rect.width as usize,
                WINDOW_TITLE_BAR_H as usize,
                0x1A1A1A,
            );
            framebuffer::draw_text_5x7(
                (win.rect.x + 8) as usize,
                (win.rect.y + 7) as usize,
                win.title.as_str(),
                0xEEEEEE,
            );

            framebuffer::rect(
                win.controls.close_btn.x as usize,
                win.controls.close_btn.y as usize,
                16,
                16,
                0xE74C3C,
            );
            framebuffer::draw_text_5x7(
                (win.controls.close_btn.x + 5) as usize,
                (win.controls.close_btn.y + 5) as usize,
                "X",
                0xFFFFFF,
            );

            framebuffer::rect(
                win.controls.maximize_btn.x as usize,
                win.controls.maximize_btn.y as usize,
                16,
                16,
                0x27AE60,
            );
            framebuffer::draw_text_5x7(
                (win.controls.maximize_btn.x + 4) as usize,
                (win.controls.maximize_btn.y + 5) as usize,
                "O",
                0xFFFFFF,
            );

            framebuffer::rect(
                win.controls.minimize_btn.x as usize,
                win.controls.minimize_btn.y as usize,
                16,
                16,
                0xF39C12,
            );
            framebuffer::draw_text_5x7(
                (win.controls.minimize_btn.x + 5) as usize,
                (win.controls.minimize_btn.y + 5) as usize,
                "-",
                0xFFFFFF,
            );

            framebuffer::blit(
                win.rect.x as usize,
                (win.rect.y + WINDOW_TITLE_BAR_H) as usize,
                win.rect.width as usize,
                (win.rect.height as i32 - WINDOW_TITLE_BAR_H).max(0) as usize,
                &win.buffer,
            );

            self.draw_explorer_selection_overlay_for_window(win);

            if win.state == WindowState::Normal {
                let grip = WINDOW_RESIZE_GRIP.max(8);
                let gx = (win.rect.x + win.rect.width as i32 - grip).max(0) as usize;
                let gy = (win.rect.y + win.rect.height as i32 - grip).max(0) as usize;
                let gsize = (grip - 1) as usize;

                framebuffer::rect(gx, gy, gsize, gsize, 0x243447);
                framebuffer::rect(gx, gy, gsize, 1, 0x6A7F95);
                framebuffer::rect(gx, gy, 1, gsize, 0x6A7F95);
                framebuffer::rect(gx + gsize - 1, gy, 1, gsize, 0x122030);
                framebuffer::rect(gx, gy + gsize - 1, gsize, 1, 0x122030);
            }
        }

        self.draw_taskbar_overlay();

        if self.taskbar.start_menu_open {
            let menu = self.start_menu_rect();
            let menu_x = menu.x.max(0) as usize;
            let menu_y = menu.y.max(0) as usize;
            framebuffer::rect(menu_x, menu_y, menu.width as usize, menu.height as usize, 0x222222);
            framebuffer::rect(menu_x, menu_y, menu.width as usize, 1, 0x555555);
            framebuffer::rect(
                menu_x + menu.width as usize - 1,
                menu_y,
                1,
                menu.height as usize,
                0x555555,
            );
            framebuffer::rect(
                menu_x,
                menu_y + menu.height as usize - 1,
                menu.width as usize,
                1,
                0x555555,
            );

            let terminal_item = self.start_menu_item_rect(0);
            framebuffer::rect(
                terminal_item.x.max(0) as usize,
                terminal_item.y.max(0) as usize,
                terminal_item.width as usize,
                terminal_item.height as usize,
                0x2D2D2D,
            );
            framebuffer::draw_text_5x7(
                (terminal_item.x + 8).max(0) as usize,
                (terminal_item.y + 8).max(0) as usize,
                "Terminal Window",
                0xFFFFFF,
            );

            let explorer_item = self.start_menu_item_rect(1);
            framebuffer::rect(
                explorer_item.x.max(0) as usize,
                explorer_item.y.max(0) as usize,
                explorer_item.width as usize,
                explorer_item.height as usize,
                0x203348,
            );
            framebuffer::draw_text_5x7(
                (explorer_item.x + 8).max(0) as usize,
                (explorer_item.y + 8).max(0) as usize,
                "File Explorer",
                0xE3F4FF,
            );

            let browser_item = self.start_menu_item_rect(2);
            framebuffer::rect(
                browser_item.x.max(0) as usize,
                browser_item.y.max(0) as usize,
                browser_item.width as usize,
                browser_item.height as usize,
                0x2A3B4C,
            );
            framebuffer::draw_text_5x7(
                (browser_item.x + 8).max(0) as usize,
                (browser_item.y + 8).max(0) as usize,
                "Web Browser",
                0xD0E0F0,
            );

            let settings_item = self.start_menu_item_rect(3);
            framebuffer::rect(
                settings_item.x.max(0) as usize,
                settings_item.y.max(0) as usize,
                settings_item.width as usize,
                settings_item.height as usize,
                0x37455A,
            );
            framebuffer::draw_text_5x7(
                (settings_item.x + 8).max(0) as usize,
                (settings_item.y + 8).max(0) as usize,
                "Configuracion",
                0xFFFFFF,
            );

            let tools_item = self.start_menu_item_rect(4);
            framebuffer::rect(
                tools_item.x.max(0) as usize,
                tools_item.y.max(0) as usize,
                tools_item.width as usize,
                tools_item.height as usize,
                if self.start_tools_open { 0x37455A } else { 0x2A3444 },
            );
            framebuffer::draw_text_5x7(
                (tools_item.x + 8).max(0) as usize,
                (tools_item.y + 8).max(0) as usize,
                "Herramientas >",
                0xDDEEFF,
            );

            let games_item = self.start_menu_item_rect(5);
            framebuffer::rect(
                games_item.x.max(0) as usize,
                games_item.y.max(0) as usize,
                games_item.width as usize,
                games_item.height as usize,
                if self.start_games_open { 0x2F4F42 } else { 0x233A31 },
            );
            framebuffer::draw_text_5x7(
                (games_item.x + 8).max(0) as usize,
                (games_item.y + 8).max(0) as usize,
                "Juegos >",
                0xDEFFEF,
            );

            let apps_item = self.start_menu_item_rect(6);
            framebuffer::rect(
                apps_item.x.max(0) as usize,
                apps_item.y.max(0) as usize,
                apps_item.width as usize,
                apps_item.height as usize,
                if self.start_apps_open { 0x3E3858 } else { 0x2C2A44 },
            );
            framebuffer::draw_text_5x7(
                (apps_item.x + 8).max(0) as usize,
                (apps_item.y + 8).max(0) as usize,
                "Apps >",
                0xEFEAFF,
            );

            let shutdown_item = self.start_menu_item_rect(7);
            framebuffer::rect(
                shutdown_item.x.max(0) as usize,
                shutdown_item.y.max(0) as usize,
                shutdown_item.width as usize,
                shutdown_item.height as usize,
                0x3A1F1F,
            );
            framebuffer::draw_text_5x7(
                (shutdown_item.x + 8).max(0) as usize,
                (shutdown_item.y + 8).max(0) as usize,
                "Apagar",
                0xFFDDDD,
            );

            if self.start_tools_open {
                let tmenu = self.tools_menu_rect();
                let tx = tmenu.x.max(0) as usize;
                let ty = tmenu.y.max(0) as usize;

                framebuffer::rect(tx, ty, tmenu.width as usize, tmenu.height as usize, 0x1F2A36);
                framebuffer::rect(tx, ty, tmenu.width as usize, 1, 0x556A7D);
                framebuffer::rect(
                    tx + tmenu.width as usize - 1,
                    ty,
                    1,
                    tmenu.height as usize,
                    0x556A7D,
                );
                framebuffer::rect(
                    tx,
                    ty + tmenu.height as usize - 1,
                    tmenu.width as usize,
                    1,
                    0x556A7D,
                );

                let note_item = self.tools_menu_item_rect(0);
                framebuffer::rect(
                    note_item.x.max(0) as usize,
                    note_item.y.max(0) as usize,
                    note_item.width as usize,
                    note_item.height as usize,
                    0x31445C,
                );
                framebuffer::draw_text_5x7(
                    (note_item.x + 8).max(0) as usize,
                    (note_item.y + 8).max(0) as usize,
                    "Notepad",
                    0xF0FAFF,
                );

                let shell_item = self.tools_menu_item_rect(1);
                framebuffer::rect(
                    shell_item.x.max(0) as usize,
                    shell_item.y.max(0) as usize,
                    shell_item.width as usize,
                    shell_item.height as usize,
                    0x2F4054,
                );
                framebuffer::draw_text_5x7(
                    (shell_item.x + 8).max(0) as usize,
                    (shell_item.y + 8).max(0) as usize,
                    "UEFI Shell",
                    0xEAF4FF,
                );
            }

            if self.start_games_open {
                let gmenu = self.games_menu_rect();
                let gx = gmenu.x.max(0) as usize;
                let gy = gmenu.y.max(0) as usize;

                framebuffer::rect(gx, gy, gmenu.width as usize, gmenu.height as usize, 0x1D3129);
                framebuffer::rect(gx, gy, gmenu.width as usize, 1, 0x5B8B72);
                framebuffer::rect(
                    gx + gmenu.width as usize - 1,
                    gy,
                    1,
                    gmenu.height as usize,
                    0x5B8B72,
                );
                framebuffer::rect(
                    gx,
                    gy + gmenu.height as usize - 1,
                    gmenu.width as usize,
                    1,
                    0x5B8B72,
                );

                let doom_item = self.games_menu_item_rect(0);
                framebuffer::rect(
                    doom_item.x.max(0) as usize,
                    doom_item.y.max(0) as usize,
                    doom_item.width as usize,
                    doom_item.height as usize,
                    0x2D4D3E,
                );
                framebuffer::draw_text_5x7(
                    (doom_item.x + 8).max(0) as usize,
                    (doom_item.y + 8).max(0) as usize,
                    "DOOM Launcher",
                    0xE7FFF0,
                );
            }

            if self.start_apps_open {
                let amenu = self.apps_menu_rect();
                let ax = amenu.x.max(0) as usize;
                let ay = amenu.y.max(0) as usize;

                framebuffer::rect(ax, ay, amenu.width as usize, amenu.height as usize, 0x282242);
                framebuffer::rect(ax, ay, amenu.width as usize, 1, 0x8475B7);
                framebuffer::rect(
                    ax + amenu.width as usize - 1,
                    ay,
                    1,
                    amenu.height as usize,
                    0x8475B7,
                );
                framebuffer::rect(
                    ax,
                    ay + amenu.height as usize - 1,
                    amenu.width as usize,
                    1,
                    0x8475B7,
                );

                if self.start_app_shortcuts.is_empty() {
                    let item = self.apps_menu_item_rect(0);
                    framebuffer::rect(
                        item.x.max(0) as usize,
                        item.y.max(0) as usize,
                        item.width as usize,
                        item.height as usize,
                        0x3A3654,
                    );
                    framebuffer::draw_text_5x7(
                        (item.x + 8).max(0) as usize,
                        (item.y + 8).max(0) as usize,
                        "No apps instaladas",
                        0xD8D4EA,
                    );
                } else {
                    let visible = self.apps_menu_item_count();
                    for idx in 0..visible {
                        let item = self.apps_menu_item_rect(idx);
                        framebuffer::rect(
                            item.x.max(0) as usize,
                            item.y.max(0) as usize,
                            item.width as usize,
                            item.height as usize,
                            if idx & 1 == 0 { 0x3A3654 } else { 0x342F4D },
                        );
                        if let Some(shortcut) = self.start_app_shortcuts.get(idx) {
                            framebuffer::draw_text_5x7(
                                (item.x + 8).max(0) as usize,
                                (item.y + 8).max(0) as usize,
                                Self::trim_ascii_line(shortcut.label.as_str(), 28).as_str(),
                                0xF3EEFF,
                            );
                        }
                    }
                }
            }
        }

        self.draw_desktop_context_menu_overlay();
        self.draw_explorer_context_menu_overlay();
        self.draw_desktop_create_folder_prompt();
        self.draw_rename_prompt();
        self.draw_notepad_save_prompt();
        self.draw_copy_progress_prompt();
        self.draw_cursor();
        framebuffer::present();
        // Run copy jobs after presenting a frame so the progress prompt is visible immediately.
        self.service_clipboard_paste_job();
    }

    fn draw_taskbar_overlay(&mut self) {
        let bg_color = 0x111111;
        framebuffer::rect(
            self.taskbar.rect.x as usize,
            self.taskbar.rect.y as usize,
            self.taskbar.rect.width as usize,
            self.taskbar.rect.height as usize,
            bg_color,
        );
        framebuffer::rect(0, self.taskbar.rect.y as usize, self.width, 1, 0x333333);

        self.taskbar_window.buffer.fill(0x00000000);
        self.taskbar.draw(&mut self.taskbar_window, self.taskbar.rect);

        let tabs_start_x = 90;
        for (i, (_id, title)) in self.minimized_windows.iter().enumerate() {
            let x = tabs_start_x + (i * 120);
            if x + 115 > self.width {
                break;
            }

            self.taskbar_window
                .fill_rect(Rect::new(x as i32, 5, 115, 30), Color(0x333333));
            let mut display_title = title.clone();
            if display_title.len() > 10 {
                display_title.truncate(7);
                display_title.push_str("...");
            }
            self.taskbar_window.draw_text(
                (x + 10) as u32,
                17,
                display_title.as_bytes(),
                Color(0xFFCCCCCC),
            );
        }

        framebuffer::blit(
            0,
            self.taskbar.rect.y as usize,
            self.width,
            self.taskbar.rect.height as usize,
            &self.taskbar_window.buffer,
        );
    }

    fn draw_cursor(&self) {
        let x = self.mouse_pos.x;
        let y = self.mouse_pos.y;
        if x < 0 || y < 0 {
            return;
        }

        let x = x as usize;
        let y = y as usize;
        if x >= self.width || y >= self.height {
            return;
        }

        framebuffer::rect(x, y, 10, 14, 0x000000);

        for i in 0..12 {
            framebuffer::rect(x + 1, y + 1 + i, 1, 1, 0xFFFFFF);
            if i < 8 {
                framebuffer::rect(x + 1 + i, y + 1 + i, 1, 1, 0xFFFFFF);
            }
        }
        framebuffer::rect(x + 1, y + 1, 8, 1, 0xFFFFFF);
    }

    pub fn minimize_window(&mut self, id: usize) {
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
            win.minimize();
            self.minimized_windows.push((id, win.title.clone()));
        }
    }

    pub fn restore_window(&mut self, id: usize) {
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
            win.restore();
        }
        self.minimized_windows.retain(|(win_id, _)| *win_id != id);
    }

    pub fn close_window(&mut self, id: usize) {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            let mut win = self.windows.remove(idx);
            win.close();
            self.closed_windows.push(win);
        }
        self.minimized_windows.retain(|(win_id, _)| *win_id != id);
        if self
            .explorer_context_menu
            .as_ref()
            .map(|menu| menu.win_id == id)
            .unwrap_or(false)
        {
            self.explorer_context_menu = None;
        }
        if self.linux_bridge_window_id == Some(id) {
            self.linux_bridge_window_id = None;
        }
        self.explorer_clear_selection_for_window(id);
        if self
            .notepad_save_prompt
            .as_ref()
            .map(|prompt| prompt.win_id == id)
            .unwrap_or(false)
        {
            self.notepad_save_prompt = None;
        }
        if self
            .rename_prompt
            .as_ref()
            .map(|prompt| matches!(prompt.origin, RenamePromptOrigin::ExplorerWindow(win_id) if win_id == id))
            .unwrap_or(false)
        {
            self.rename_prompt = None;
        }
    }

    fn push_unique_device_index(indices: &mut Vec<usize>, index: usize) {
        if !indices.iter().any(|existing| *existing == index) {
            indices.push(index);
        }
    }

    fn auto_mount_candidate_indices(&self) -> Vec<usize> {
        let devices = crate::fat32::Fat32::detect_uefi_block_devices();
        let mut out = Vec::new();
        let skipped = self.desktop_usb_ejected_device_index;
        let usb_hint = self.desktop_usb_device_index;

        if let Some(current) = self.current_volume_device_index {
            if Some(current) != skipped && devices.iter().any(|dev| dev.index == current) {
                Self::push_unique_device_index(&mut out, current);
            }
        }

        for dev in devices.iter() {
            if Some(dev.index) == skipped || Some(dev.index) == usb_hint {
                continue;
            }
            if !dev.removable && dev.logical_partition {
                Self::push_unique_device_index(&mut out, dev.index);
            }
        }

        for dev in devices.iter() {
            if Some(dev.index) == skipped || Some(dev.index) == usb_hint {
                continue;
            }
            if !dev.removable {
                Self::push_unique_device_index(&mut out, dev.index);
            }
        }

        for dev in devices.iter() {
            if Some(dev.index) == skipped || Some(dev.index) == usb_hint {
                continue;
            }
            if dev.logical_partition {
                Self::push_unique_device_index(&mut out, dev.index);
            }
        }

        for dev in devices.iter() {
            if Some(dev.index) == skipped || Some(dev.index) == usb_hint {
                continue;
            }
            if dev.removable && !dev.logical_partition {
                Self::push_unique_device_index(&mut out, dev.index);
            }
        }

        for dev in devices.iter() {
            if Some(dev.index) == skipped || Some(dev.index) == usb_hint {
                continue;
            }
            Self::push_unique_device_index(&mut out, dev.index);
        }

        if let Some(usb_idx) = usb_hint {
            if Some(usb_idx) != skipped {
                Self::push_unique_device_index(&mut out, usb_idx);
            }
        }

        out
    }

    fn try_auto_mount_available_volume(&mut self) -> bool {
        let candidates = self.auto_mount_candidate_indices();
        for index in candidates {
            let mounted = {
                let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                fat.mount_uefi_block_device(index).is_ok()
            };
            if mounted {
                self.current_volume_device_index = Some(index);
                self.clear_manual_unmount_lock();
                return true;
            }
        }
        false
    }

    fn ensure_volume_index_mounted(&mut self, index: usize) -> bool {
        let already_mounted = self.current_volume_device_index == Some(index)
            && unsafe { crate::fat32::GLOBAL_FAT.bytes_per_sector != 0 };
        if already_mounted {
            return true;
        }

        let mounted = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            fat.mount_uefi_block_device(index).is_ok()
        };
        if mounted {
            self.current_volume_device_index = Some(index);
            self.clear_manual_unmount_lock();
        }
        mounted
    }

    fn ensure_fat_ready(&mut self) -> bool {
        {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            if fat.bytes_per_sector != 0 {
                return true;
            }
        }

        if self.try_auto_mount_available_volume() {
            return true;
        }

        if self.manual_unmount_lock {
            return false;
        }

        let init_ok = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            fat.init()
        };
        if init_ok {
            return true;
        }

        false
    }

    fn ensure_fat_ready_for_explorer(&mut self, win_id: usize) -> bool {
        if !self.ensure_fat_ready() {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status("No hay volumen FAT32 disponible. Conecta o monta una unidad.");
            }
            return false;
        }

        let target_device = self
            .windows
            .iter()
            .find(|w| w.id == win_id && w.is_explorer())
            .and_then(|w| w.explorer_device_index);
        if let Some(index) = target_device {
            if !self.ensure_volume_index_mounted(index) {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_explorer_status("No se pudo montar la unidad de esta ventana Explorer.");
                }
                return false;
            }
        }

        true
    }

    fn ensure_fat_ready_for_notepad(&mut self, win_id: usize) -> bool {
        if self.ensure_fat_ready() {
            return true;
        }

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.set_notepad_status("No hay volumen FAT32 disponible para guardar.");
        }
        false
    }

    fn refresh_explorer_home(&mut self, win_id: usize) {
        let mut items = alloc::vec![
            ExplorerItem::new("Desktop", ExplorerItemKind::ShortcutDesktop, 0, 0),
            ExplorerItem::new("Downloads", ExplorerItemKind::ShortcutDownloads, 0, 0),
            ExplorerItem::new("Documents", ExplorerItemKind::ShortcutDocuments, 0, 0),
            ExplorerItem::new("Images", ExplorerItemKind::ShortcutImages, 0, 0),
            ExplorerItem::new("Videos", ExplorerItemKind::ShortcutVideos, 0, 0),
        ];

        let devices = crate::fat32::Fat32::detect_uefi_block_devices();
        let boot_device_index = crate::fat32::Fat32::boot_block_device_index();
        let status = if devices.is_empty() {
            items.push(ExplorerItem::new("Storage", ExplorerItemKind::ShortcutUsb, 0, 0));
            String::from("No BlockIO storage detected yet.")
        } else {
            let mut listed = 0usize;
            for dev in devices.iter() {
                if !dev.logical_partition {
                    continue;
                }
                let is_boot = Some(dev.index) == boot_device_index;
                let media = if dev.removable && !is_boot { "USB" } else { "NVME/HDD" };
                let boot_tag = if is_boot { " [BOOT]" } else { "" };
                let title =
                    alloc::format!("{} {} ({} MiB){}", media, dev.index, dev.total_mib, boot_tag);
                items.push(ExplorerItem::new(
                    title.as_str(),
                    ExplorerItemKind::ShortcutVolume,
                    dev.index as u32,
                    0,
                ));
                listed += 1;
            }

            if listed == 0 {
                for dev in devices.iter() {
                    if !dev.removable || dev.logical_partition {
                        continue;
                    }
                    let is_boot = Some(dev.index) == boot_device_index;
                    if is_boot {
                        continue;
                    }
                    let title = alloc::format!("USB {} ({} MiB)", dev.index, dev.total_mib);
                    items.push(ExplorerItem::new(
                        title.as_str(),
                        ExplorerItemKind::ShortcutVolume,
                        dev.index as u32,
                        0,
                    ));
                    listed += 1;
                }
            }

            if listed == 0 {
                items.push(ExplorerItem::new("Storage", ExplorerItemKind::ShortcutUsb, 0, 0));
                String::from("No likely mount targets found. Try terminal command: disks")
            } else {
                String::from("Select a device to probe and mount FAT32.")
            }
        };

        self.explorer_clear_selection_for_window(win_id);
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.set_explorer_listing("Quick Access", 0, None, items);
            win.set_explorer_status(status.as_str());
        }
    }

    fn build_explorer_dir_items(fat: &mut crate::fat32::Fat32, cluster: u32) -> Vec<ExplorerItem> {
        use crate::fs::FileType;

        let mut items = Vec::new();
        items.push(ExplorerItem::new("Home", ExplorerItemKind::Home, 0, 0));
        items.push(ExplorerItem::new("Up", ExplorerItemKind::Up, 0, 0));

        if let Ok(entries) = fat.read_dir_entries(cluster) {
            for entry in entries.iter() {
                if !entry.valid {
                    continue;
                }

                let name = entry.full_name();
                if name == "." || name == ".." {
                    continue;
                }

                let kind = if entry.file_type == FileType::Directory {
                    ExplorerItemKind::Directory
                } else {
                    ExplorerItemKind::File
                };

                let entry_cluster = if entry.cluster == 0 {
                    fat.root_cluster
                } else {
                    entry.cluster
                };

                items.push(ExplorerItem::new(name.as_str(), kind, entry_cluster, entry.size));
            }
        }

        items
    }

    fn show_explorer_directory(
        &mut self,
        win_id: usize,
        cluster: u32,
        path: String,
        status: String,
        device_hint: Option<usize>,
    ) {
        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        let sticky_device = self
            .windows
            .iter()
            .find(|w| w.id == win_id)
            .and_then(|w| w.explorer_device_index);
        if let Some(target_index) = device_hint.or(sticky_device) {
            if !self.ensure_volume_index_mounted(target_index) {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_explorer_status("No se pudo montar la unidad para este Explorer.");
                }
                return;
            }
        }

        let (cluster, path, items) = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let mut effective_cluster = if cluster < 2 {
                fat.root_cluster
            } else {
                cluster
            };
            if effective_cluster == 0 {
                effective_cluster = fat.root_cluster;
            }

            let mut effective_path = path;
            if effective_cluster == fat.root_cluster {
                let volume = Self::volume_label_text(fat).unwrap_or(String::from("USB"));
                effective_path = Self::explorer_path_root_component(effective_path.as_str())
                    .unwrap_or_else(|| alloc::format!("{}/", volume));
            }

            let items = Self::build_explorer_dir_items(fat, effective_cluster);
            (effective_cluster, effective_path, items)
        };
        let listing_device_index = self.current_volume_device_index;

        self.explorer_clear_selection_for_window(win_id);
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.set_explorer_listing(path.as_str(), cluster, listing_device_index, items);
            win.set_explorer_status(status.as_str());
        }
    }

    fn open_explorer_usb_root(&mut self, win_id: usize) {
        let devices = crate::fat32::Fat32::detect_uefi_block_devices();
        let boot_device_index = crate::fat32::Fat32::boot_block_device_index();
        if devices.is_empty() {
            self.refresh_explorer_home(win_id);
            return;
        }

        let mut selected = None;
        for dev in devices.iter() {
            if dev.removable
                && dev.logical_partition
                && Some(dev.index) != boot_device_index
            {
                selected = Some(dev.index);
                break;
            }
        }
        if selected.is_none() {
            for dev in devices.iter() {
                if dev.removable && Some(dev.index) != boot_device_index {
                    selected = Some(dev.index);
                    break;
                }
            }
        }
        if selected.is_none() {
            for dev in devices.iter() {
                if dev.removable && dev.logical_partition {
                    selected = Some(dev.index);
                    break;
                }
            }
        }
        if selected.is_none() {
            for dev in devices.iter() {
                if dev.removable {
                    selected = Some(dev.index);
                    break;
                }
            }
        }

        match selected {
            Some(idx) => self.open_explorer_volume(win_id, idx),
            None => self.refresh_explorer_home(win_id),
        }
    }

    fn open_explorer_volume(&mut self, win_id: usize, index: usize) {
        let (root_cluster, path, status) = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            match fat.mount_uefi_block_device(index) {
                Ok(vol) => {
                    self.clear_manual_unmount_lock();
                    let label = Self::volume_label_from_bytes(&vol.volume_label)
                        .unwrap_or(alloc::format!("VOL{}", vol.index));
                    let is_boot = crate::fat32::Fat32::boot_block_device_index() == Some(vol.index);
                    let media = if vol.removable && !is_boot {
                        "USB"
                    } else {
                        "NVME/HDD"
                    };
                    (
                        vol.root_cluster,
                        alloc::format!("{} [{} {}]/", label, media, vol.index),
                        alloc::format!(
                            "Mounted {} device {} (start LBA {}).",
                            media,
                            vol.index,
                            vol.partition_start
                        ),
                    )
                }
                Err(err) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status(err);
                    }
                    return;
                }
            }
        };

        self.clear_manual_unmount_lock();
        self.current_volume_device_index = Some(index);
        self.show_explorer_directory(win_id, root_cluster, path, status, Some(index));
    }

    fn open_explorer_named_root_dir(&mut self, win_id: usize, shortcut_name: &str) {
        match self.resolve_named_root_dir_on_best_volume(shortcut_name, false) {
            Ok((cluster, path)) => {
                let status = alloc::format!("Folder: {}", shortcut_name);
                self.show_explorer_directory(
                    win_id,
                    cluster,
                    path,
                    status,
                    self.current_volume_device_index,
                );
            }
            Err(err) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_explorer_status(err.as_str());
                }
            }
        }
    }

    fn open_explorer_directory(&mut self, win_id: usize, dir: &ExplorerItem) {
        let current_path = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => win.explorer_path.clone(),
            None => return,
        };

        let mut next_path = current_path;
        if !next_path.ends_with('/') {
            next_path.push('/');
        }
        next_path.push_str(dir.label.as_str());
        next_path.push('/');

        self.show_explorer_directory(
            win_id,
            dir.cluster,
            next_path,
            alloc::format!("Folder: {}", dir.label),
            self
                .windows
                .iter()
                .find(|w| w.id == win_id)
                .and_then(|w| w.explorer_device_index)
                .or(self.current_volume_device_index),
        );
    }

    fn open_explorer_up(&mut self, win_id: usize) {
        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        let (current_cluster, current_path) = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => (win.explorer_current_cluster, win.explorer_path.clone()),
            None => return,
        };

        let (root_cluster, volume_label, parent_cluster) = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let root = fat.root_cluster;
            let volume = Self::volume_label_text(fat).unwrap_or(String::from("USB"));

            let mut parent = root;
            if current_cluster != root {
                if let Ok(entries) = fat.read_dir_entries(current_cluster) {
                    for entry in entries.iter() {
                        if entry.matches_name("..") {
                            parent = if entry.cluster == 0 { root } else { entry.cluster };
                            break;
                        }
                    }
                }
            }

            (root, volume, parent)
        };
        let root_path = Self::explorer_path_root_component(current_path.as_str())
            .unwrap_or_else(|| alloc::format!("{}/", volume_label));
        let device_hint = self
            .windows
            .iter()
            .find(|w| w.id == win_id)
            .and_then(|w| w.explorer_device_index)
            .or(self.current_volume_device_index);

        if current_cluster == root_cluster {
            let current_norm = Self::ascii_lower(current_path.trim().trim_end_matches('/'));
            let root_norm = Self::ascii_lower(root_path.trim().trim_end_matches('/'));
            if current_norm != root_norm {
                self.show_explorer_directory(
                    win_id,
                    root_cluster,
                    root_path,
                    String::from("Volumen raiz."),
                    device_hint,
                );
            } else {
                self.refresh_explorer_home(win_id);
            }
            return;
        }

        let current_leaf = {
            let trimmed = current_path.trim_end_matches('/');
            match trimmed.rfind('/') {
                Some(idx) => &trimmed[idx + 1..],
                None => trimmed,
            }
        };
        if parent_cluster == root_cluster && Self::is_quick_access_shortcut_name(current_leaf) {
            self.refresh_explorer_home(win_id);
            return;
        }

        let mut next_path = current_path;
        if next_path.ends_with('/') {
            next_path.pop();
        }
        if let Some(idx) = next_path.rfind('/') {
            next_path.truncate(idx + 1);
        } else {
            next_path = root_path.clone();
        }

        if parent_cluster == root_cluster {
            next_path = root_path;
        }

        self.show_explorer_directory(
            win_id,
            parent_cluster,
            next_path,
            String::from("Moved up."),
            device_hint,
        );
    }

    fn trim_ascii_line(text: &str, max_chars: usize) -> String {
        if text.len() <= max_chars {
            return String::from(text);
        }

        let mut out = String::new();
        for b in text.bytes().take(max_chars.saturating_sub(3)) {
            out.push(b as char);
        }
        out.push_str("...");
        out
    }

    fn push_unique_start_app_shortcut(shortcuts: &mut Vec<StartAppShortcut>, label: &str, command: &str) {
        let command_trimmed = command.trim();
        if command_trimmed.is_empty() {
            return;
        }
        let command_lower = Self::ascii_lower(command_trimmed);
        if shortcuts
            .iter()
            .any(|existing| Self::ascii_lower(existing.command.as_str()) == command_lower)
        {
            return;
        }

        let mut safe_label = label.trim();
        if safe_label.is_empty() {
            safe_label = "App";
        }
        shortcuts.push(StartAppShortcut {
            label: Self::trim_ascii_line(safe_label, 22),
            command: String::from(command_trimmed),
        });
    }

    fn parse_start_app_shortcut_text(file_name: &str, text: &str) -> Option<StartAppShortcut> {
        let mut label = String::from(Self::filename_stem(file_name));
        let mut command: Option<String> = None;

        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(rest) = line.strip_prefix("NAME=") {
                let value = rest.trim();
                if !value.is_empty() {
                    label = String::from(value);
                }
                continue;
            }

            if let Some(rest) = line.strip_prefix("CMD=") {
                let value = rest.trim();
                if !value.is_empty() {
                    command = Some(Self::normalize_start_app_shortcut_command(value));
                }
                continue;
            }

            if let Some(rest) = line.strip_prefix("TARGET=") {
                let value = rest.trim();
                if !value.is_empty() && command.is_none() && Self::is_rml_file_name(value) {
                    command = Some(Self::normalize_start_app_shortcut_command(
                        alloc::format!("runapp {}", value).as_str(),
                    ));
                }
                continue;
            }
        }

        let command = command?;
        let mut safe_label = label.trim();
        if safe_label.is_empty() {
            safe_label = Self::filename_stem(file_name);
        }
        if safe_label.trim().is_empty() {
            safe_label = "App";
        }

        Some(StartAppShortcut {
            label: Self::trim_ascii_line(safe_label, 22),
            command,
        })
    }

    fn normalize_start_app_shortcut_target_path(path: &str) -> String {
        let mut normalized = String::new();
        for b in path.trim().bytes() {
            if b == b'\\' {
                normalized.push('/');
            } else {
                normalized.push(b as char);
            }
        }

        if normalized.is_empty()
            || normalized.starts_with('/')
            || normalized.starts_with("./")
            || normalized.starts_with("../")
        {
            return normalized;
        }

        if normalized.bytes().any(|b| b == b'/') {
            alloc::format!("/{}", normalized)
        } else {
            normalized
        }
    }

    fn normalize_start_app_shortcut_command(command: &str) -> String {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let mut parts = trimmed.splitn(2, ' ');
        let verb = Self::ascii_lower(parts.next().unwrap_or(""));
        let arg = parts.next().unwrap_or("").trim();

        if verb == "runapp" {
            let target = Self::normalize_start_app_shortcut_target_path(arg);
            return if target.is_empty() {
                String::from(trimmed)
            } else {
                alloc::format!("runapp {}", target)
            };
        }

        if verb == "linux" || verb == "lnx" {
            let mut sub_parts = arg.splitn(2, ' ');
            let sub = Self::ascii_lower(sub_parts.next().unwrap_or(""));
            let target_raw = sub_parts.next().unwrap_or("").trim();
            if sub == "runloop" {
                let mut runloop_parts = target_raw.splitn(3, ' ');
                let action = Self::ascii_lower(runloop_parts.next().unwrap_or(""));
                let action_target_raw = runloop_parts.next().unwrap_or("").trim();
                let extra = runloop_parts.next().unwrap_or("").trim();
                if (action == "start"
                    || action == "startx"
                    || action == "startm"
                    || action == "startmx")
                    && !action_target_raw.is_empty()
                    && extra.is_empty()
                {
                    let target = Self::normalize_start_app_shortcut_target_path(action_target_raw);
                    return alloc::format!("linux runloop {} {}", action, target);
                }
            }
            if (sub == "run"
                || sub == "runreal"
                || sub == "runrealx"
                || sub == "runx"
                || sub == "launch"
                || sub == "inspect")
                && !target_raw.is_empty()
            {
                let target = Self::normalize_start_app_shortcut_target_path(target_raw);
                return alloc::format!("linux {} {}", sub, target);
            }
        }

        String::from(trimmed)
    }

    fn derive_install_shortcut_label(app_id_arg: Option<&str>, package_name: &str) -> String {
        let base = app_id_arg
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Self::filename_stem(package_name).trim());
        if base.is_empty() {
            String::from("App")
        } else {
            Self::trim_ascii_line(base, 22)
        }
    }

    fn write_install_shortcut_file(
        fat: &mut crate::fat32::Fat32,
        app_tag4: &str,
        package_name: &str,
        app_id_arg: Option<&str>,
        layout_file_name: &str,
    ) -> Result<(String, String), &'static str> {
        let command = alloc::format!("runapp {}", layout_file_name);
        Self::write_install_shortcut_command(
            fat,
            app_tag4,
            package_name,
            app_id_arg,
            command.as_str(),
            None,
        )
    }

    fn write_install_shortcut_command(
        fat: &mut crate::fat32::Fat32,
        app_tag4: &str,
        package_name: &str,
        app_id_arg: Option<&str>,
        command: &str,
        label_suffix: Option<&str>,
    ) -> Result<(String, String), &'static str> {
        let shortcut_name = alloc::format!("{}.APP", app_tag4);
        let mut shortcut_label = Self::derive_install_shortcut_label(app_id_arg, package_name);
        if let Some(suffix) = label_suffix {
            let suffix_trim = suffix.trim();
            if !suffix_trim.is_empty() {
                let combined = alloc::format!("{} {}", shortcut_label, suffix_trim);
                shortcut_label = Self::trim_ascii_line(combined.as_str(), 22);
            }
        }
        let shortcut_text = alloc::format!(
            "NAME={}\nCMD={}\nSOURCE={}\n",
            shortcut_label,
            command,
            package_name
        );

        fat.write_text_file_in_dir(fat.root_cluster, shortcut_name.as_str(), shortcut_text.as_bytes())?;
        Ok((shortcut_name, shortcut_label))
    }

    fn write_install_linux_launch_metadata(
        fat: &mut crate::fat32::Fat32,
        target_cluster: u32,
        app_tag8: &str,
        package_name: &str,
        app_id_arg: Option<&str>,
        target_path: &str,
        command: &str,
        candidate: &LinuxInstallShortcutCandidate,
    ) -> Result<String, &'static str> {
        let metadata_name = alloc::format!("{}.LNX", app_tag8);
        let mut metadata_text = String::new();
        metadata_text.push_str("LINUX LAUNCH\n");
        metadata_text.push_str(alloc::format!("PACKAGE={}\n", package_name).as_str());
        if let Some(app_id) = app_id_arg {
            let app_id_trim = app_id.trim();
            if !app_id_trim.is_empty() {
                metadata_text.push_str(alloc::format!("APP_ID={}\n", app_id_trim).as_str());
            }
        }
        metadata_text.push_str(alloc::format!("MODE={}\n", candidate.mode.manifest_name()).as_str());
        metadata_text.push_str(alloc::format!("COMMAND={}\n", command).as_str());
        metadata_text.push_str(alloc::format!("TARGET={}\n", target_path).as_str());
        metadata_text.push_str(alloc::format!("EXEC_LOCAL={}\n", candidate.exec_name).as_str());
        metadata_text.push_str(alloc::format!("EXEC_SOURCE={}\n", candidate.source_path).as_str());
        metadata_text.push_str(alloc::format!("RANK={}\n", candidate.rank).as_str());
        if let Some(interp) = candidate.interp_path.as_deref() {
            metadata_text.push_str(alloc::format!("PT_INTERP={}\n", interp).as_str());
        }
        metadata_text.push_str(alloc::format!("DT_NEEDED={}\n", candidate.needed.len()).as_str());
        for (idx, needed) in candidate.needed.iter().enumerate() {
            metadata_text.push_str(alloc::format!("NEEDED_{:04}={}\n", idx + 1, needed).as_str());
        }

        fat.write_text_file_in_dir(target_cluster, metadata_name.as_str(), metadata_text.as_bytes())?;
        Ok(metadata_name)
    }

    fn refresh_start_app_shortcuts(&mut self) {
        use crate::fs::FileType;

        self.start_app_shortcuts.clear();

        let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
        if fat.bytes_per_sector == 0 {
            if self.manual_unmount_lock || !fat.init() {
                return;
            }
        }

        let root_cluster = fat.root_cluster;
        let entries = match fat.read_dir_entries(root_cluster) {
            Ok(v) => v,
            Err(_) => return,
        };

        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File || entry.cluster < 2 || entry.size == 0 {
                continue;
            }

            let name = entry.full_name();
            let lower = Self::ascii_lower(name.as_str());
            if !lower.ends_with(".app") {
                continue;
            }
            if entry.size as usize > 2048 {
                continue;
            }

            let mut raw = Vec::new();
            raw.resize(entry.size as usize, 0);
            let len = match fat.read_file_sized(entry.cluster, entry.size as usize, &mut raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            raw.truncate(len);

            let text = match core::str::from_utf8(raw.as_slice()) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(shortcut) = Self::parse_start_app_shortcut_text(name.as_str(), text) {
                Self::push_unique_start_app_shortcut(
                    &mut self.start_app_shortcuts,
                    shortcut.label.as_str(),
                    shortcut.command.as_str(),
                );
            }
        }

        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File {
                continue;
            }
            let name = entry.full_name();
            if !Self::is_rml_file_name(name.as_str()) {
                continue;
            }
            let label = Self::filename_stem(name.as_str());
            let command = alloc::format!("runapp {}", name);
            Self::push_unique_start_app_shortcut(
                &mut self.start_app_shortcuts,
                label,
                command.as_str(),
            );
        }

        self.start_app_shortcuts.sort_by(|a, b| {
            Self::ascii_lower(a.label.as_str()).cmp(&Self::ascii_lower(b.label.as_str()))
        });
        if self.start_app_shortcuts.len() > APPS_MENU_MAX_ITEMS {
            self.start_app_shortcuts.truncate(APPS_MENU_MAX_ITEMS);
        }
    }

    fn open_explorer_file(&mut self, win_id: usize, item: &ExplorerItem) {
        if !self.ensure_fat_ready_for_explorer(win_id) {
            return;
        }

        let mut status = alloc::format!("File: {} ({} bytes)", item.label, item.size);
        let mut preview_lines = Vec::new();

        {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };

            if item.cluster < 2 || item.size == 0 {
                preview_lines.push(String::from("(empty file)"));
            } else {
                let target = (item.size as usize).min(4096);
                let mut buffer = Vec::new();
                buffer.resize(target, 0);

                match fat.read_file_sized(item.cluster, target, &mut buffer) {
                    Ok(len) => match core::str::from_utf8(&buffer[..len]) {
                        Ok(text) => {
                            for line in text.lines().take(4) {
                                preview_lines.push(Self::trim_ascii_line(line, 72));
                            }
                            if preview_lines.is_empty() {
                                preview_lines.push(String::from("(empty file)"));
                            }
                        }
                        Err(_) => {
                            preview_lines.push(String::from("<binary content>"));
                        }
                    },
                    Err(_) => {
                        status = alloc::format!("Could not read file: {}", item.label);
                        preview_lines.push(String::from("Read error from FAT32 storage."));
                    }
                }
            }
        }

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.set_explorer_preview(status.as_str(), preview_lines);
        }
    }

    fn open_png_from_explorer_file(&mut self, explorer_win_id: usize, item: &ExplorerItem) {
        if !self.ensure_fat_ready_for_explorer(explorer_win_id) {
            return;
        }

        if item.cluster < 2 || item.size == 0 {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == explorer_win_id) {
                win.set_explorer_preview(
                    alloc::format!("No se pudo abrir {}", item.label).as_str(),
                    alloc::vec![String::from("Archivo PNG vacio o cluster invalido.")],
                );
            }
            return;
        }

        let file_len = item.size as usize;
        if file_len > IMAGE_VIEWER_MAX_FILE_BYTES {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == explorer_win_id) {
                win.set_explorer_preview(
                    alloc::format!("No se pudo abrir {}", item.label).as_str(),
                    alloc::vec![alloc::format!(
                        "PNG demasiado grande (max {} bytes).",
                        IMAGE_VIEWER_MAX_FILE_BYTES
                    )],
                );
            }
            return;
        }

        let mut file_bytes = Vec::new();
        file_bytes.resize(file_len, 0);
        let read_len = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            match fat.read_file_sized(item.cluster, file_len, &mut file_bytes) {
                Ok(n) => n,
                Err(_) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == explorer_win_id) {
                        win.set_explorer_preview(
                            alloc::format!("No se pudo abrir {}", item.label).as_str(),
                            alloc::vec![String::from("Error leyendo el PNG desde FAT32.")],
                        );
                    }
                    return;
                }
            }
        };
        file_bytes.truncate(read_len);

        match Self::decode_png_to_rgb(file_bytes.as_slice()) {
            Ok((img_w, img_h, pixels)) => {
                let title = alloc::format!(
                    "Image Viewer - {}",
                    Self::trim_ascii_line(item.label.as_str(), 24)
                );
                let viewer_id = self.create_image_viewer_window(title.as_str(), 160, 70, 920, 620);
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == viewer_id) {
                    let status = alloc::format!(
                        "PNG cargado: {}x{} ({} bytes).",
                        img_w, img_h, read_len
                    );
                    win.load_image_viewer(item.label.as_str(), img_w, img_h, pixels, status.as_str());
                }

                if let Some(win) = self.windows.iter_mut().find(|w| w.id == explorer_win_id) {
                    win.set_explorer_preview(
                        alloc::format!("Opened PNG: {}", item.label).as_str(),
                        alloc::vec![
                            alloc::format!("Resolution: {}x{}", img_w, img_h),
                            String::from("Image opened in separate Image Viewer window."),
                        ],
                    );
                }
            }
            Err(err) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == explorer_win_id) {
                    win.set_explorer_preview(
                        alloc::format!("No se pudo abrir {}", item.label).as_str(),
                        alloc::vec![String::from(err)],
                    );
                }
            }
        }
    }

    fn open_notepad_blank(&mut self) {
        let note_id = self.create_notepad_window("Notepad", 180, 90, 860, 560);

        let (dir_cluster, dir_path, status) = if self.ensure_fat_ready() {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            let label = Self::volume_label_text(fat).unwrap_or(String::from("USB"));
            (
                fat.root_cluster,
                alloc::format!("{}/", label),
                String::from("New document."),
            )
        } else {
            (
                0,
                String::from("/"),
                String::from("FAT32 not ready. You can edit text but not save yet."),
            )
        };

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == note_id) {
            win.load_notepad_document(dir_cluster, dir_path.as_str(), "NOTE.TXT", "", status.as_str());
        }
    }

    fn open_notepad_from_explorer_file(
        &mut self,
        dir_cluster: u32,
        dir_path: String,
        item: &ExplorerItem,
    ) {
        let mut text = String::new();
        let mut status = alloc::format!("Opened {}", item.label);

        if self.ensure_fat_ready() {
            if item.cluster >= 2 && item.size > 0 {
                let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                let max_load = item.size as usize;
                let target = max_load.min(NOTEPAD_MAX_TEXT_BYTES);
                let mut buffer = Vec::new();
                buffer.resize(target, 0);

                match fat.read_file_sized(item.cluster, target, &mut buffer) {
                    Ok(len) => match core::str::from_utf8(&buffer[..len]) {
                        Ok(s) => {
                            text = String::from(s);
                            if max_load > target {
                                status = alloc::format!(
                                    "Opened {} (truncated to {} bytes)",
                                    item.label,
                                    target
                                );
                            }
                        }
                        Err(_) => {
                            status = String::from(
                                "File is not UTF-8 legible. Edit manually or open another file.",
                            );
                        }
                    },
                    Err(_) => {
                        status = String::from("Read error while opening file.");
                    }
                }
            }
        } else {
            status = String::from("FAT32 not ready.");
        }

        let note_id = self.create_notepad_window("Notepad", 180, 90, 860, 560);
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == note_id) {
            win.load_notepad_document(
                dir_cluster,
                dir_path.as_str(),
                item.label.as_str(),
                text.as_str(),
                status.as_str(),
            );
        }
    }

    fn save_notepad_file(&mut self, win_id: usize) {
        let (mut dir_cluster, mut dir_path, file_name, text) = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => (
                win.notepad_dir_cluster,
                win.notepad_dir_path.clone(),
                win.notepad_file_name.clone(),
                win.notepad_text.clone(),
            ),
            None => return,
        };

        let trimmed_name = file_name.trim();
        if trimmed_name.is_empty() {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_notepad_status("Filename is empty.");
            }
            return;
        }

        if text.len() > NOTEPAD_MAX_TEXT_BYTES {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_notepad_status(alloc::format!("Text too large (max {} bytes). Shorten content.", NOTEPAD_MAX_TEXT_BYTES).as_str());
            }
            return;
        }

        if !self.ensure_fat_ready_for_notepad(win_id) {
            return;
        }

        {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            if dir_cluster < 2 {
                dir_cluster = fat.root_cluster;
                let label = Self::volume_label_text(fat).unwrap_or(String::from("USB"));
                dir_path = alloc::format!("{}/", label);
            }
        }

        let result = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            fat.write_text_file_in_dir(dir_cluster, trimmed_name, text.as_bytes())
        };

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.notepad_dir_cluster = dir_cluster;
            win.notepad_dir_path = dir_path;
            match result {
                Ok(()) => win.set_notepad_status("File saved."),
                Err(e) => win.set_notepad_status(alloc::format!("Save failed: {}", e).as_str()),
            }
        }
    }

    fn delete_notepad_file(&mut self, win_id: usize) {
        let (mut dir_cluster, file_name) = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => (win.notepad_dir_cluster, win.notepad_file_name.clone()),
            None => return,
        };

        let trimmed_name = file_name.trim();
        if trimmed_name.is_empty() {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_notepad_status("Filename is empty.");
            }
            return;
        }

        if !self.ensure_fat_ready_for_notepad(win_id) {
            return;
        }

        {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            if dir_cluster < 2 {
                dir_cluster = fat.root_cluster;
            }
        }

        let result = {
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            fat.delete_file_in_dir(dir_cluster, trimmed_name)
        };

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.notepad_dir_cluster = dir_cluster;
            match result {
                Ok(()) => {
                    win.notepad_text.clear();
                    win.set_notepad_status("File deleted.");
                }
                Err(e) => win.set_notepad_status(alloc::format!("Delete failed: {}", e).as_str()),
            }
        }
    }

    fn handle_notepad_click(&mut self, win_id: usize, mouse_x: i32, mouse_y: i32) {
        let action = match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => win.notepad_action_at(mouse_x, mouse_y),
            None => None,
        };

        match action {
            Some(NotepadClickAction::New) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.prepare_notepad_new("NEWFILE.TXT");
                }
            }
            Some(NotepadClickAction::Save) => self.begin_notepad_save_prompt(win_id),
            Some(NotepadClickAction::Delete) => self.delete_notepad_file(win_id),
            Some(NotepadClickAction::FilenameField) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_notepad_filename_focus(true);
                }
            }
            Some(NotepadClickAction::EditorArea) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.set_notepad_filename_focus(false);
                }
            }
            None => {}
        }
    }

    fn handle_explorer_click(&mut self, win_id: usize, mouse_x: i32, mouse_y: i32) {
        let (clicked, is_canvas, dir_cluster, dir_path, items) =
            match self.windows.iter().find(|w| w.id == win_id) {
            Some(win) => (
                win.explorer_item_at(mouse_x, mouse_y),
                win.explorer_canvas_contains(mouse_x, mouse_y),
                win.explorer_current_cluster,
                win.explorer_path.clone(),
                win.explorer_items.clone(),
            ),
            None => return,
        };

        let Some(item) = clicked else {
            if is_canvas {
                self.explorer_clear_selection_scope(win_id, dir_cluster);
            }
            return;
        };
        let was_selected = self.explorer_item_selected(win_id, dir_cluster, &item);

        if item.kind == ExplorerItemKind::File || item.kind == ExplorerItemKind::Directory {
            if !was_selected {
                let selected =
                    self.explorer_collect_selected_items(win_id, dir_cluster, items.as_slice());
                if selected.is_empty() {
                    self.explorer_select_single(win_id, dir_cluster, &item);
                } else if !self.explorer_item_selected(win_id, dir_cluster, &item) {
                    self.explorer_add_selection(win_id, dir_cluster, &item);
                }
                let selected_count = self
                    .explorer_collect_selected_items(win_id, dir_cluster, items.as_slice())
                    .len()
                    .max(1);

                if item.kind == ExplorerItemKind::Directory {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        if selected_count > 1 {
                            win.set_explorer_status(
                                alloc::format!("{} elementos seleccionados.", selected_count)
                                    .as_str(),
                            );
                        } else {
                            win.set_explorer_status(
                                alloc::format!(
                                    "Carpeta seleccionada: {}. Clic de nuevo para abrir.",
                                    item.label
                                )
                                .as_str(),
                            );
                        }
                    }
                    return;
                }

                if selected_count > 1 {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.set_explorer_status(
                            alloc::format!("{} elementos seleccionados.", selected_count).as_str(),
                        );
                    }
                } else if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    if Self::explorer_item_is_zip(&item) {
                        win.set_explorer_status(
                            alloc::format!(
                                "Archivo ZIP: {}. Clic de nuevo para abrir.",
                                item.label
                            )
                            .as_str(),
                        );
                    } else {
                        win.set_explorer_status(
                            alloc::format!(
                                "Archivo seleccionado: {}. Clic de nuevo para abrir.",
                                item.label
                            )
                            .as_str(),
                        );
                    }
                }
                return;
            }

            if item.kind == ExplorerItemKind::Directory {
                self.open_explorer_directory(win_id, &item);
            } else {
                if Self::is_png_file_name(item.label.as_str()) {
                    self.open_png_from_explorer_file(win_id, &item);
                } else {
                    self.open_notepad_from_explorer_file(dir_cluster, dir_path, &item);
                }
            }
            return;
        }

        if !was_selected {
            self.explorer_select_single(win_id, dir_cluster, &item);
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.set_explorer_status(
                    alloc::format!("Seleccionado: {}. Clic de nuevo para abrir.", item.label)
                        .as_str(),
                );
            }
            return;
        }

        self.explorer_clear_selection_scope(win_id, dir_cluster);

        match item.kind {
            ExplorerItemKind::ShortcutUsb => self.open_explorer_usb_root(win_id),
            ExplorerItemKind::ShortcutVolume => self.open_explorer_volume(win_id, item.cluster as usize),
            ExplorerItemKind::ShortcutDesktop => self.open_explorer_named_root_dir(win_id, "Desktop"),
            ExplorerItemKind::ShortcutDownloads => self.open_explorer_named_root_dir(win_id, "Downloads"),
            ExplorerItemKind::ShortcutDocuments => self.open_explorer_named_root_dir(win_id, "Documents"),
            ExplorerItemKind::ShortcutImages => self.open_explorer_named_root_dir(win_id, "Images"),
            ExplorerItemKind::ShortcutVideos => self.open_explorer_named_root_dir(win_id, "Videos"),
            ExplorerItemKind::Home => self.refresh_explorer_home(win_id),
            ExplorerItemKind::Up => self.open_explorer_up(win_id),
            ExplorerItemKind::Directory | ExplorerItemKind::File | ExplorerItemKind::ShortcutRecycleBin => {}
        }
    }

    fn web_proxy_auto_gateway_base(&self) -> Option<String> {
        let gateway = crate::net::get_gateway()?;
        let gateway_text = alloc::format!("{}", gateway);
        if gateway_text.is_empty() || gateway_text == "0.0.0.0" {
            return None;
        }
        Some(alloc::format!(
            "http://{}:{}",
            gateway_text,
            WEB_PROXY_DEFAULT_PORT
        ))
    }

    fn web_proxy_is_auto_hint(value: &str) -> bool {
        let lowered = Self::ascii_lower(value);
        lowered == "auto" || lowered == "gateway" || lowered == "gw"
    }

    fn web_proxy_candidate_bases(&self) -> Vec<String> {
        let mut bases = Vec::new();
        let configured = self.web_proxy_endpoint_base.trim();
        let configured_is_auto =
            configured.is_empty() || Self::web_proxy_is_auto_hint(configured);

        if !configured.is_empty() && !configured_is_auto {
            bases.push(String::from(configured));
        }

        if configured_is_auto {
            if let Some(gateway_base) = self.web_proxy_auto_gateway_base() {
                let gateway_ip = gateway_base
                    .trim()
                    .trim_start_matches("http://")
                    .split(':')
                    .next()
                    .map(String::from)
                    .unwrap_or_else(String::new);
                bases.push(gateway_base);
                if !gateway_ip.is_empty() {
                    bases.push(alloc::format!(
                        "http://{}:{}",
                        gateway_ip, WEB_PROXY_ALT_PORT
                    ));
                }
            }
            bases.push(String::from(WEB_PROXY_FALLBACK_BASE));
            bases.push(String::from("http://10.0.2.2:37820"));
        }

        if bases.is_empty() {
            bases.push(String::from(WEB_PROXY_FALLBACK_BASE));
        }

        let mut dedup: Vec<String> = Vec::new();
        for base in bases.into_iter() {
            if !dedup.iter().any(|item| item.as_str() == base.as_str()) {
                dedup.push(base);
            }
        }
        dedup
    }

    fn web_proxy_base(&self) -> String {
        let candidates = self.web_proxy_candidate_bases();
        if let Some(first) = candidates.into_iter().next() {
            first
        } else {
            String::from(WEB_PROXY_FALLBACK_BASE)
        }
    }

    fn web_proxy_url_with_base(base: &str, path_and_query: &str) -> String {
        if path_and_query.is_empty() {
            return String::from(base);
        }

        let mut out = String::new();
        let base_trimmed = base.trim_end_matches('/');
        out.push_str(base_trimmed);
        if !path_and_query.starts_with('/') {
            out.push('/');
        }
        out.push_str(path_and_query);
        out
    }

    fn web_http_get_short(&mut self, url: &str) -> Option<String> {
        let mut pump = || self.pump_ui_while_blocked_net();
        crate::net::http_get_request_with_timeout(url, &mut pump, WEB_PROXY_PROBE_TIMEOUT_TICKS)
    }

    fn web_cef_request_first_reachable(
        &mut self,
        path_and_query: &str,
    ) -> (Option<String>, Option<String>, Vec<String>) {
        let candidates = self.web_proxy_candidate_bases();
        for base in candidates.iter() {
            let endpoint = Self::web_proxy_url_with_base(base.as_str(), path_and_query);
            if let Some(raw) = self.web_http_get_short(endpoint.as_str()) {
                self.web_proxy_endpoint_base = base.clone();
                return (Some(base.clone()), Some(raw), candidates);
            }
        }
        (None, None, candidates)
    }

    fn hex_upper(n: u8) -> char {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        HEX[(n & 0x0F) as usize] as char
    }

    fn url_encode_component(text: &str) -> String {
        let mut out = String::new();
        for b in text.bytes() {
            let keep = (b >= b'A' && b <= b'Z')
                || (b >= b'a' && b <= b'z')
                || (b >= b'0' && b <= b'9')
                || b == b'-'
                || b == b'_'
                || b == b'.'
                || b == b'~';
            if keep {
                out.push(b as char);
            } else {
                out.push('%');
                out.push(Self::hex_upper(b >> 4));
                out.push(Self::hex_upper(b));
            }
        }
        out
    }

    fn parse_http_status_and_body(raw: &str) -> (Option<u16>, String) {
        let mut code = None;
        if raw.starts_with("HTTP/") {
            if let Some(first_line_end) = raw.find('\n') {
                let first = raw[..first_line_end].trim();
                let mut parts = first.split_whitespace();
                let _proto = parts.next();
                code = parts.next().and_then(|v| v.parse::<u16>().ok());
            }
        }

        if let Some(idx) = raw.find("\r\n\r\n") {
            return (code, String::from(&raw[idx + 4..]));
        }
        if let Some(idx) = raw.find("\n\n") {
            return (code, String::from(&raw[idx + 2..]));
        }
        (code, String::new())
    }

    fn parse_http_status_and_body_bytes(raw: &[u8]) -> (Option<u16>, Vec<u8>) {
        let mut code = None;
        if raw.starts_with(b"HTTP/") {
            if let Some(first_line_end) = raw.iter().position(|b| *b == b'\n') {
                let first = core::str::from_utf8(&raw[..first_line_end]).unwrap_or("").trim();
                let mut parts = first.split_whitespace();
                let _proto = parts.next();
                code = parts.next().and_then(|v| v.parse::<u16>().ok());
            }
        }

        if let Some(idx) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            return (code, raw[idx + 4..].to_vec());
        }
        if let Some(idx) = raw.windows(2).position(|w| w == b"\n\n") {
            return (code, raw[idx + 2..].to_vec());
        }
        (code, Vec::new())
    }

    fn parse_ppm_token<'a>(bytes: &'a [u8], idx: &mut usize) -> Option<&'a [u8]> {
        while *idx < bytes.len() {
            let b = bytes[*idx];
            if b == b'#' {
                while *idx < bytes.len() && bytes[*idx] != b'\n' {
                    *idx += 1;
                }
            } else if b.is_ascii_whitespace() {
                *idx += 1;
            } else {
                break;
            }
        }
        if *idx >= bytes.len() {
            return None;
        }
        let start = *idx;
        while *idx < bytes.len() && !bytes[*idx].is_ascii_whitespace() {
            *idx += 1;
        }
        Some(&bytes[start..*idx])
    }

    fn parse_ppm_p6_surface(
        body: &[u8],
        source: &str,
    ) -> Option<crate::web_servo_bridge::ServoBridgeSurface> {
        let mut idx = 0usize;
        let magic = Self::parse_ppm_token(body, &mut idx)?;
        if magic != b"P6" {
            return None;
        }
        let width = core::str::from_utf8(Self::parse_ppm_token(body, &mut idx)?)
            .ok()?
            .parse::<usize>()
            .ok()?;
        let height = core::str::from_utf8(Self::parse_ppm_token(body, &mut idx)?)
            .ok()?
            .parse::<usize>()
            .ok()?;
        let maxv = core::str::from_utf8(Self::parse_ppm_token(body, &mut idx)?)
            .ok()?
            .parse::<usize>()
            .ok()?;

        if maxv != 255 || width == 0 || height == 0 {
            return None;
        }
        if width.saturating_mul(height) > WEB_CEF_FRAME_MAX_PIXELS {
            return None;
        }

        while idx < body.len() && body[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= body.len() {
            return None;
        }

        let rgb = &body[idx..];
        let expected = width.saturating_mul(height).saturating_mul(3);
        if rgb.len() < expected {
            return None;
        }

        let mut pixels = Vec::new();
        pixels.resize(width.saturating_mul(height), 0);
        let mut src = 0usize;
        for dst in pixels.iter_mut() {
            let r = rgb[src] as u32;
            let g = rgb[src + 1] as u32;
            let b = rgb[src + 2] as u32;
            *dst = (r << 16) | (g << 8) | b;
            src += 3;
        }

        Some(crate::web_servo_bridge::ServoBridgeSurface {
            source: String::from(source),
            width: width as u32,
            height: height as u32,
            pixels,
        })
    }

    fn browser_fetch_cef_frame_with_base(
        &mut self,
        base: &str,
    ) -> Option<crate::web_servo_bridge::ServoBridgeSurface> {
        let endpoint = Self::web_proxy_url_with_base(base, "frame");
        let mut pump = || self.pump_ui_while_blocked_net();
        let raw = crate::net::http_get_request_bytes_with_timeout(
            endpoint.as_str(),
            &mut pump,
            WEB_PROXY_FRAME_TIMEOUT_TICKS,
        )?;
        let (code, body) = Self::parse_http_status_and_body_bytes(raw.as_slice());
        if code != Some(200) {
            return None;
        }
        Self::parse_ppm_p6_surface(body.as_slice(), "webkit-host-frame")
    }

    fn browser_cef_dispatch_input(&mut self, win_id: usize, path_and_query: &str) {
        let (base, raw, tried_bases) = self.web_cef_request_first_reachable(path_and_query);
        let Some(raw) = raw else {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.browser_status = String::from("WEBKIT input offline");
                win.browser_content_lines
                    .push(String::from("[WEBKIT] input fallo: endpoint no alcanzable."));
                for base in tried_bases.iter().take(3) {
                    win.browser_content_lines
                        .push(alloc::format!("[WEBKIT] tried: {}", base));
                }
                win.render_browser();
            }
            self.paint();
            return;
        };

        let selected_base = base.unwrap_or_else(|| self.web_proxy_base());
        let (code, body) = Self::parse_http_status_and_body(raw.as_str());
        let mut status = alloc::format!("WEBKIT input HTTP {:?}", code);
        if code == Some(200) {
            if let Some(surface) = self.browser_fetch_cef_frame_with_base(selected_base.as_str()) {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.browser_surface_source = surface.source;
                    win.browser_surface_width = surface.width;
                    win.browser_surface_height = surface.height;
                    win.browser_surface_pixels = surface.pixels;
                    status = alloc::format!(
                        "WEBKIT frame {}x{}",
                        win.browser_surface_width, win.browser_surface_height
                    );
                    win.browser_status = status.clone();
                    if !body.trim().is_empty() {
                        win.browser_content_lines
                            .push(alloc::format!("[WEBKIT] {}", body.lines().next().unwrap_or("ok")));
                    }
                    win.render_browser();
                }
                self.paint();
                return;
            }
            status = String::from("WEBKIT input ok (sin frame)");
        }

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.browser_status = status;
            if !body.trim().is_empty() {
                win.browser_content_lines
                    .push(alloc::format!(
                        "[WEBKIT] {}",
                        body.lines().next().unwrap_or("respuesta")
                    ));
            }
            win.render_browser();
        }
        self.paint();
    }

    fn browser_fetch_with_cef_bridge(&mut self, url: &str) -> crate::web_servo_bridge::ServoBridgeRender {
        let encoded_url = Self::url_encode_component(url);
        let (open_base, open_raw, tried_bases) = self.web_cef_request_first_reachable(
            alloc::format!("open?url={}", encoded_url).as_str(),
        );
        let Some(open_raw) = open_raw else {
            let mut lines = vec![
                String::from("[HOST] No se pudo conectar al renderer HTTPS."),
                String::from("[HOST] GO no usa fallback texto en modo host."),
                String::from("[HOST] Inicia bridge en host (macOS/Linux):"),
                String::from(
                    "  bash scripts/run_webkit_host_bridge.sh 0.0.0.0:37810 https://example.com",
                ),
                String::from("  (opcional) bash scripts/run_cef_host_bridge.sh 0.0.0.0:37820 https://example.com"),
                String::from("[HOST] Luego en ReduxOS:"),
                String::from("  web backend webkit"),
                String::from("  web webkit endpoint auto  (o http://<IP_HOST>:37810)"),
                String::from("  web webkit ping"),
            ];
            if !tried_bases.is_empty() {
                lines.push(String::new());
                lines.push(String::from("[HOST] Endpoints intentados:"));
                for base in tried_bases.iter().take(4) {
                    lines.push(alloc::format!("  - {}", base));
                }
            }
            return crate::web_servo_bridge::ServoBridgeRender {
                output: Some(crate::web_engine::BrowserRenderOutput {
                    final_url: String::from(url),
                    status: String::from("HOST BRIDGE OFFLINE"),
                    title: Some(String::from("Redux Browser - Host Bridge")),
                    lines,
                    surface: None,
                }),
                note: Some(alloc::format!(
                    "Host bridge no alcanzable ({} endpoint(s) probados).",
                    tried_bases.len()
                )),
                surface: None,
            };
        };

        let selected_base = open_base.unwrap_or_else(|| self.web_proxy_base());
        let (open_code, open_body) = Self::parse_http_status_and_body(open_raw.as_str());
        if open_code != Some(200) {
            return crate::web_servo_bridge::ServoBridgeRender {
                output: Some(crate::web_engine::BrowserRenderOutput {
                    final_url: String::from(url),
                    status: String::from("HOST BRIDGE ERROR"),
                    title: Some(String::from("Redux Browser - Host Bridge")),
                    lines: vec![
                        String::from("[HOST] El renderer remoto devolvio error en /open."),
                        alloc::format!("[HOST] endpoint: {}", selected_base),
                        alloc::format!("[HOST] status: {:?}", open_code),
                        String::from("[HOST] Revisa 'web webkit ping' y logs del bridge."),
                    ],
                    surface: None,
                }),
                note: Some(alloc::format!(
                    "Host bridge respondio status {:?} en /open.",
                    open_code
                )),
                surface: None,
            };
        }

        let mut lines = Vec::new();
        lines.push(String::from("[WEBKIT] URL enviada al host renderer."));
        lines.push(alloc::format!("[WEBKIT] endpoint: {}", selected_base));
        lines.push(alloc::format!("[WEBKIT] url: {}", url));
        if !open_body.trim().is_empty() {
            lines.push(String::new());
            lines.push(String::from("[WEBKIT] open response:"));
            for line in open_body.lines().take(8) {
                lines.push(String::from(line.trim_end()));
            }
        }

        let status_endpoint = Self::web_proxy_url_with_base(selected_base.as_str(), "status");
        let status_raw = self.web_http_get_short(status_endpoint.as_str());
        if let Some(status_raw) = status_raw {
            let (status_code, status_body) = Self::parse_http_status_and_body(status_raw.as_str());
            lines.push(String::new());
            lines.push(alloc::format!(
                "[WEBKIT] status endpoint: HTTP {:?}",
                status_code
            ));
            for line in status_body.lines().take(20) {
                lines.push(String::from(line.trim_end()));
            }
        }

        if lines.is_empty() {
            lines.push(String::from("[WEBKIT] solicitud enviada."));
        }

        let surface = self.browser_fetch_cef_frame_with_base(selected_base.as_str());
        if let Some(surface) = surface.as_ref() {
            lines.push(alloc::format!(
                "[WEBKIT] frame: {}x{} ({})",
                surface.width, surface.height, surface.source
            ));
        } else {
            lines.push(String::from(
                "[WEBKIT] frame no disponible (host sin /frame o timeout).",
            ));
        }

        crate::web_servo_bridge::ServoBridgeRender {
            output: Some(crate::web_engine::BrowserRenderOutput {
                final_url: String::from(url),
                status: String::from("WEBKIT BRIDGE OK"),
                title: Some(String::from("Redux WebKit Bridge")),
                lines,
                surface: None,
            }),
            note: Some(String::from("render remoto via WebKit/Wry host HTTP bridge.")),
            surface,
        }
    }

    fn browser_fetch_with_vaev_bridge(
        &mut self,
        url: &str,
    ) -> crate::web_servo_bridge::ServoBridgeRender {
        let mut pump = || self.pump_ui_while_blocked_net();
        crate::web_vaev_bridge::fetch_and_render(url, &mut pump)
    }

    fn browser_target_for_web_input(&self) -> Option<usize> {
        if let Some(active_id) = self.active_window_id {
            if self
                .windows
                .iter()
                .any(|win| win.id == active_id && win.is_browser())
            {
                return Some(active_id);
            }
        }

        self.windows
            .iter()
            .rev()
            .find(|win| win.is_browser())
            .map(|win| win.id)
    }

    fn browser_apply_vaev_result(
        &mut self,
        win_id: usize,
        mut result: crate::web_servo_bridge::ServoBridgeRender,
    ) -> bool {
        let note = result.note.take();
        let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) else {
            return false;
        };

        if let Some(surface) = result.surface.take() {
            win.browser_surface_source = surface.source;
            win.browser_surface_width = surface.width;
            win.browser_surface_height = surface.height;
            win.browser_surface_pixels = surface.pixels;
        }

        if let Some(page) = result.output.take() {
            win.browser_status = page.status;
            win.browser_content_lines.clear();
            win.browser_scroll = 0;
            win.browser_url = page.final_url;

            if let Some(title) = page.title {
                if !title.trim().is_empty() {
                    win.title = alloc::format!("Redux Browser - {}", title);
                } else {
                    win.title = String::from("Redux Browser");
                }
            } else {
                win.title = String::from("Redux Browser");
            }

            for line in page.lines {
                win.browser_content_lines.push(line);
            }
        } else if let Some(note_text) = note.as_ref() {
            if !note_text.trim().is_empty() {
                win.browser_status = note_text.clone();
            }
        }

        if let Some(note_text) = note {
            if !note_text.trim().is_empty() {
                win.browser_content_lines
                    .push(alloc::format!("[VAEV] {}", note_text));
            }
        }

        win.render_browser();
        true
    }

    fn browser_vaev_dispatch_input(
        &mut self,
        win_id: usize,
        event: crate::web_vaev_bridge::VaevInputEvent,
    ) {
        let mut pump = || self.pump_ui_while_blocked_net();
        let result = crate::web_vaev_bridge::dispatch_input(event, &mut pump);
        let _ = self.browser_apply_vaev_result(win_id, result);
        self.paint();
    }

    fn web_backend_label(&self) -> &'static str {
        match self.web_backend_mode {
            WebBackendMode::Builtin => "builtin",
            WebBackendMode::Cef => "webkit",
            WebBackendMode::Vaev => "vaev",
        }
    }

    fn web_backend_status_line(&self) -> String {
        let detail = match self.web_backend_mode {
            WebBackendMode::Builtin => {
                "motor HTML interno original (siempre disponible)"
            }
            WebBackendMode::Cef => {
                if WEB_CEF_BRIDGE_ENABLED {
                    "host WebKit bridge remoto (Wry; HTML/CSS/JS real via host)"
                } else {
                    "WebKit bridge deshabilitado en esta build (modo nativo local)"
                }
            }
            WebBackendMode::Vaev => {
                if crate::web_vaev_bridge::feature_enabled() {
                    if crate::web_vaev_bridge::binding_mode() == "integrated-shim" {
                        "Vaev embebido (shim integrado con frame/input bridge en kernel)"
                    } else {
                        "Vaev embebido (libreria externa enlazada)"
                    }
                } else {
                    "Vaev bridge deshabilitado en esta build"
                }
            }
        };
        alloc::format!("Web backend: {} ({})", self.web_backend_label(), detail)
    }

    fn browser_fetch_with_backend(
        &mut self,
        url: &str,
    ) -> crate::web_servo_bridge::ServoBridgeRender {
        match self.web_backend_mode {
            WebBackendMode::Builtin => {
                let mut pump = || self.pump_ui_while_blocked_net();
                let output = crate::web_engine::fetch_and_render(url, &mut pump);
                let surface = output
                    .as_ref()
                    .and_then(crate::web_servo_bridge::builtin_surface_from_output);
                crate::web_servo_bridge::ServoBridgeRender {
                    output,
                    note: Some(String::from(
                        "render interno builtin (HTML/CSS/JS subset visual) sin proxy.",
                    )),
                    surface,
                }
            }
            WebBackendMode::Cef => {
                if WEB_CEF_BRIDGE_ENABLED {
                    self.browser_fetch_with_cef_bridge(url)
                } else {
                    let mut pump = || self.pump_ui_while_blocked_net();
                    let output = crate::web_engine::fetch_and_render(url, &mut pump);
                    let surface = output
                        .as_ref()
                        .and_then(crate::web_servo_bridge::builtin_surface_from_output);
                    crate::web_servo_bridge::ServoBridgeRender {
                        output,
                        note: Some(String::from(
                            "WebKit bridge deshabilitado; usando render interno nativo local.",
                        )),
                        surface,
                    }
                }
            }
            WebBackendMode::Vaev => self.browser_fetch_with_vaev_bridge(url),
        }
    }

    fn browser_navigate_to(&mut self, win_id: usize, target_url: &str) {
        let url = target_url.trim();
        if url.is_empty() {
            return;
        }

        {
            let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) else {
                return;
            };

            if !win.is_browser() {
                return;
            }

            win.browser_status = String::from("Connecting...");
            win.browser_content_lines.clear();
            win.browser_scroll = 0;
            win.browser_content_lines
                .push(alloc::format!("Contacting {}...", url));
            win.render_browser();
        }

        // Force paint so user sees immediate feedback.
        self.paint();

        let link_up = unsafe {
            if crate::intel_net::GLOBAL_INTEL_NET.is_some() {
                crate::intel_net::is_link_up()
            } else {
                true // Assume VirtIO link is up if device exists
            }
        };

        if !link_up && !url.starts_with("redux://") {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.browser_status = String::from("No Link");
                win.browser_content_lines.clear();
                win.browser_content_lines.push(String::from("NO ETHERNET LINK DETECTED."));
                win.browser_content_lines.push(String::from("PLEASE CHECK YOUR CABLE."));
                win.render_browser();
            }
            self.paint();
            return;
        }

        let render_result = if url.starts_with("redux://") {
            None
        } else {
            Some(self.browser_fetch_with_backend(url))
        };

        // Update window with render result.
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            if let Some(mut result) = render_result {
                if let Some(surface) = result.surface.take() {
                    win.browser_surface_source = surface.source;
                    win.browser_surface_width = surface.width;
                    win.browser_surface_height = surface.height;
                    win.browser_surface_pixels = surface.pixels;
                } else {
                    win.browser_surface_source.clear();
                    win.browser_surface_width = 0;
                    win.browser_surface_height = 0;
                    win.browser_surface_pixels.clear();
                }

                if let Some(page) = result.output.take() {
                    win.browser_status = page.status;
                    win.browser_content_lines.clear();
                    win.browser_scroll = 0;
                    win.browser_url = page.final_url;

                    if let Some(title) = page.title {
                        if !title.trim().is_empty() {
                            win.title = alloc::format!("Redux Browser - {}", title);
                        } else {
                            win.title = String::from("Redux Browser");
                        }
                    } else {
                        win.title = String::from("Redux Browser");
                    }

                    for line in page.lines {
                        win.browser_content_lines.push(line);
                    }
                }
            } else if url.starts_with("redux://") {
                // Handle local pages
                win.browser_surface_source.clear();
                win.browser_surface_width = 0;
                win.browser_surface_height = 0;
                win.browser_surface_pixels.clear();
                win.browser_url = String::from(url);
                if url.ends_with("about") {
                    win.browser_content_lines.clear();
                    win.browser_scroll = 0;
                    win.browser_content_lines.push(String::from("About ReduxOS Browser"));
                    win.browser_content_lines.push(String::from("Version 0.2.0 (Network Enabled)"));
                    win.browser_status = String::from("Done");
                    win.title = String::from("Redux Browser - About");
                } else {
                    win.browser_content_lines.clear();
                    win.browser_scroll = 0;
                    win.browser_content_lines.push(String::from("Welcome to ReduxOS Web Browser!"));
                    win.browser_status = String::from("Ready");
                    win.title = String::from("Redux Browser");
                }
            } else {
                win.browser_surface_source.clear();
                win.browser_surface_width = 0;
                win.browser_surface_height = 0;
                win.browser_surface_pixels.clear();
                win.browser_status = String::from("Error");
                win.browser_content_lines.clear();
                win.browser_scroll = 0;
                win.browser_content_lines.push(String::from("Request failed or timed out."));
                win.title = String::from("Redux Browser");
                
                let ip_str = if let Some(ip) = crate::net::get_ip_address() {
                    alloc::format!("IP: {}", ip)
                } else {
                    String::from("IP: None (DHCP?)")
                };
                
                let link_up = crate::intel_net::is_link_up();
                let link_str = if link_up { "Link: UP" } else { "Link: DOWN" };
                
                win.browser_content_lines.push(alloc::format!("{} | {}", ip_str, link_str));
                win.browser_content_lines.push(String::from("Check Settings for more info."));
            }
            win.render_browser();
        }
        self.paint();
    }

    pub fn handle_browser_click(&mut self, win_id: usize, mouse_x: i32, mouse_y: i32) {
        enum BrowserClickAction {
            Navigate(String),
            ScrollRows(i32),
            CefInput(String),
            VaevInput(crate::web_vaev_bridge::VaevInputEvent),
            None,
        }

        let use_cef = WEB_CEF_BRIDGE_ENABLED && matches!(self.web_backend_mode, WebBackendMode::Cef);
        let use_vaev =
            matches!(self.web_backend_mode, WebBackendMode::Vaev) && crate::web_vaev_bridge::input_enabled();
        let action = {
            let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) else {
                return;
            };

            if !win.is_browser() {
                return;
            }

            let scroll_dir = win.browser_scroll_clicked(mouse_x, mouse_y);
            if scroll_dir != 0 {
                if use_cef {
                    let delta = if scroll_dir < 0 { -120 } else { 120 };
                    BrowserClickAction::CefInput(alloc::format!("input?type=scroll&delta={}", delta))
                } else if use_vaev {
                    let delta = if scroll_dir < 0 { -120 } else { 120 };
                    BrowserClickAction::VaevInput(crate::web_vaev_bridge::VaevInputEvent::Scroll {
                        delta,
                    })
                } else {
                    BrowserClickAction::ScrollRows(8 * scroll_dir)
                }
            } else if win.browser_go_clicked(mouse_x, mouse_y) {
                BrowserClickAction::Navigate(win.browser_url.clone())
            } else if use_cef && win.browser_back_clicked(mouse_x, mouse_y) {
                BrowserClickAction::CefInput(String::from("input?type=back"))
            } else if use_vaev && win.browser_back_clicked(mouse_x, mouse_y) {
                BrowserClickAction::VaevInput(crate::web_vaev_bridge::VaevInputEvent::Back)
            } else if use_cef && win.browser_forward_clicked(mouse_x, mouse_y) {
                BrowserClickAction::CefInput(String::from("input?type=forward"))
            } else if use_vaev && win.browser_forward_clicked(mouse_x, mouse_y) {
                BrowserClickAction::VaevInput(crate::web_vaev_bridge::VaevInputEvent::Forward)
            } else if let Some(link) = win.browser_link_at(mouse_x, mouse_y) {
                win.browser_url = link.clone();
                BrowserClickAction::Navigate(link)
            } else if use_cef {
                if let Some((sx, sy)) = win.browser_surface_point_at(mouse_x, mouse_y) {
                    BrowserClickAction::CefInput(alloc::format!("input?type=click&x={}&y={}", sx, sy))
                } else {
                    BrowserClickAction::None
                }
            } else if use_vaev {
                if let Some((sx, sy)) = win.browser_surface_point_at(mouse_x, mouse_y) {
                    BrowserClickAction::VaevInput(crate::web_vaev_bridge::VaevInputEvent::Click {
                        x: sx,
                        y: sy,
                    })
                } else {
                    BrowserClickAction::None
                }
            } else {
                BrowserClickAction::None
            }
        };

        match action {
            BrowserClickAction::Navigate(url) => {
                self.browser_navigate_to(win_id, url.as_str());
            }
            BrowserClickAction::ScrollRows(step) => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    if win.browser_scroll_by(step) {
                        self.paint();
                    }
                }
            }
            BrowserClickAction::CefInput(path_and_query) => {
                self.browser_cef_dispatch_input(win_id, path_and_query.as_str());
            }
            BrowserClickAction::VaevInput(event) => {
                self.browser_vaev_dispatch_input(win_id, event);
            }
            BrowserClickAction::None => {}
        }
    }

    fn handle_doom_launcher_click(&mut self, win_id: usize, mouse_x: i32, mouse_y: i32) {
        let mut launch_requested = false;
        {
            let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) else {
                return;
            };

            if !win.is_doom_launcher() {
                return;
            }

            if win.doom_launch_clicked(mouse_x, mouse_y) {
                win.set_doom_status("Iniciando DOOM desde EFI...");
                win.render();
                launch_requested = true;
            }
        }

        if !launch_requested {
            return;
        }

        self.paint();
        let launch_result = crate::launch_doom_uefi();
        let mut shell_result = None;
        if let Err(err) = &launch_result {
            if crate::doom_error_requires_shell(err.as_str()) {
                shell_result = Some(crate::launch_doom_via_shell());
            }
        }
        let _ = crate::restore_gui_after_external_app();

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            match &launch_result {
                Ok(path) => {
                    win.set_doom_status(alloc::format!("DOOM termino y regreso ({})", path).as_str());
                }
                Err(err) => {
                    if let Some(result) = &shell_result {
                        match result {
                            Ok(path) => win.set_doom_status(
                                alloc::format!(
                                    "DOOM via Shell: ejecucion finalizada y regreso ({})",
                                    path
                                )
                                .as_str(),
                            ),
                            Err(shell_err) => win.set_doom_status(
                                alloc::format!(
                                    "DOOM no inicio: {} | Shell fallo: {}",
                                    err, shell_err
                                )
                                .as_str(),
                            ),
                        }
                    } else {
                        win.set_doom_status(alloc::format!("DOOM no inicio: {}", err).as_str());
                    }
                }
            }
            win.render();
        }
    }

    fn run_app_layout_from_cluster(
        &mut self,
        fat: &mut crate::fat32::Fat32,
        dir_cluster: u32,
        layout_name: &str,
        out: &mut Vec<String>,
    ) -> bool {
        use crate::fs::FileType;

        let mut layout_entry = None;
        match fat.read_dir_entries(dir_cluster) {
            Ok(entries) => {
                for entry in entries.iter() {
                    if !entry.valid || entry.file_type != FileType::File {
                        continue;
                    }
                    if entry.matches_name(layout_name) {
                        layout_entry = Some(*entry);
                        break;
                    }
                }
            }
            Err(_) => {
                out.push(String::from("RunApp error: no se pudo leer el directorio actual."));
            }
        }

        if !out.is_empty() {
            return false;
        }

        let Some(entry) = layout_entry else {
            out.push(String::from("RunApp error: archivo .RML no encontrado."));
            return false;
        };

        if entry.size == 0 {
            out.push(String::from("RunApp error: archivo .RML vacio."));
            return false;
        }
        if entry.size as usize > APP_RUNNER_MAX_LAYOUT_BYTES {
            out.push(alloc::format!(
                "RunApp error: layout demasiado grande (max {} bytes).",
                APP_RUNNER_MAX_LAYOUT_BYTES
            ));
            return false;
        }
        if entry.cluster < 2 {
            out.push(String::from("RunApp error: cluster invalido."));
            return false;
        }

        let mut raw = Vec::new();
        raw.resize(entry.size as usize, 0);
        match fat.read_file_sized(entry.cluster, entry.size as usize, &mut raw) {
            Ok(len) => {
                raw.truncate(len);
            }
            Err(err) => {
                out.push(alloc::format!("RunApp error: {}", err));
                return false;
            }
        }

        match core::str::from_utf8(raw.as_slice()) {
            Ok(layout_text) => match Self::parse_rml_layout(layout_text, layout_name) {
                Ok(spec) => {
                    let window_title = alloc::format!(
                        "App Runner - {}",
                        Self::trim_ascii_line(spec.app_title.as_str(), 22)
                    );
                    let runner_id =
                        self.create_app_runner_window(window_title.as_str(), 170, 80, 860, 560);
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == runner_id) {
                        win.load_app_runner_layout(
                            layout_name,
                            spec.theme.as_str(),
                            spec.header_text.as_str(),
                            spec.body_text.as_str(),
                            spec.button_label.as_str(),
                            spec.background_color,
                            spec.header_color,
                            spec.body_color,
                            spec.button_color,
                            alloc::format!("App loaded from {} ({} bytes).", layout_name, entry.size)
                                .as_str(),
                        );
                    }
                    out.push(alloc::format!("RunApp: launched {}", layout_name));
                    out.push(String::from("Opened in App Runner window."));
                    true
                }
                Err(err) => {
                    out.push(err);
                    false
                }
            },
            Err(_) => {
                out.push(String::from("RunApp error: el archivo .RML debe ser UTF-8."));
                false
            }
        }
    }

    fn launch_start_app_shortcut(&mut self, shortcut: &StartAppShortcut) {
        let command = shortcut.command.trim();
        if command.is_empty() {
            return;
        }

        let mut parts = command.splitn(2, ' ');
        let verb = Self::ascii_lower(parts.next().unwrap_or(""));
        let arg = parts.next().unwrap_or("").trim();

        if verb == "runapp" && !arg.is_empty() {
            let mut out = Vec::new();
            let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
            if fat.bytes_per_sector == 0 {
                if self.manual_unmount_lock {
                    out.push(String::from(
                        "RunApp error: volumen desmontado. Usa 'mount <n>' primero.",
                    ));
                } else if !fat.init() {
                    out.push(String::from(
                        "RunApp error: FAT32 no disponible. Usa 'disks' y 'mount <n>'.",
                    ));
                }
            }

            if out.is_empty() {
                let root_cluster = fat.root_cluster;
                self.run_app_layout_from_cluster(fat, root_cluster, arg, &mut out);
            }

            let has_error = out.iter().any(|line| {
                let lower = Self::ascii_lower(line.as_str());
                lower.contains("error")
            });
            if has_error {
                let term_id = self
                    .windows
                    .iter()
                    .find(|w| w.is_terminal())
                    .map(|w| w.id)
                    .unwrap_or_else(|| self.create_window("Terminal Shell", 100, 100, 800, 500));
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == term_id) {
                    win.add_output(alloc::format!("Start App '{}':", shortcut.label).as_str());
                    for line in out.iter() {
                        win.add_output(line.as_str());
                    }
                    win.render_terminal();
                }
            }
            return;
        }

        let term_id = self
            .windows
            .iter()
            .find(|w| w.is_terminal())
            .map(|w| w.id)
            .unwrap_or_else(|| self.create_window("Terminal Shell", 100, 100, 800, 500));
        let mut effective_command = String::from(command);
        if verb == "linux" {
            let mut sub_parts = arg.splitn(2, ' ');
            let sub = Self::ascii_lower(sub_parts.next().unwrap_or(""));
            let target = sub_parts.next().unwrap_or("").trim();
            if (sub == "run" || sub == "runreal") && !target.is_empty() {
                effective_command = alloc::format!("linux runloop start {}", target);
            } else if (sub == "runrealx" || sub == "runx" || sub == "launch") && !target.is_empty() {
                effective_command = alloc::format!("linux runloop startx {}", target);
            }
        }

        if effective_command != command {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == term_id) {
                win.add_output(
                    "Start App: linux run* -> runloop (start/startx) para modo no bloqueante.",
                );
                win.render_terminal();
            }
            self.paint();
        }

        self.execute_command(term_id, effective_command.as_str());
    }

    fn execute_command(&mut self, win_id: usize, cmd: &str) {
        use crate::fs::FileType;
        let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };

        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return;
        }

        let mut parts = trimmed.splitn(2, ' ');
        let verb_raw = parts.next().unwrap_or("");
        let arg_raw = parts.next().unwrap_or("").trim();
        let verb = Self::ascii_lower(verb_raw);
        let is_fs_cmd = verb == "ls" || verb == "cd" || verb == "cat" || verb == "cp" || verb == "mv";

        if verb == "wry" {
            if !WEB_CEF_BRIDGE_ENABLED {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.add_output("WRY/WebKit host bridge deshabilitado en esta build nativa.");
                    win.add_output("Usa el navegador interno con: web backend builtin");
                    win.render_terminal();
                }
                return;
            }
            let mapped = if arg_raw.is_empty() {
                String::from("web webkit status")
            } else {
                alloc::format!("web webkit {}", arg_raw)
            };
            self.execute_command(win_id, mapped.as_str());
            return;
        }

        if verb == "webkit" {
            if !WEB_CEF_BRIDGE_ENABLED {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.add_output("WebKit host bridge deshabilitado en esta build nativa.");
                    win.add_output("Usa el navegador interno con: web backend builtin");
                    win.render_terminal();
                }
                return;
            }
            let mapped = if arg_raw.is_empty() {
                String::from("web webkit status")
            } else {
                alloc::format!("web webkit {}", arg_raw)
            };
            self.execute_command(win_id, mapped.as_str());
            return;
        }

        if verb == "mem" {
            let stats = crate::memory::stats();
            let heap_bytes = crate::allocator::heap_size_bytes() as u64;
            let heap_reserved = crate::allocator::heap_reserved_bytes() as u64;
            let install_budget = Self::install_task_budget_bytes() as u64;
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.add_output("Memory statistics:");
                win.add_output(alloc::format!("  regions:              {}", stats.regions).as_str());
                win.add_output(alloc::format!("  total pages:          {}", stats.total_pages).as_str());
                win.add_output(
                    alloc::format!("  conventional pages:   {}", stats.conventional_pages).as_str(),
                );
                win.add_output(alloc::format!("  reserved pages:       {}", stats.reserved_pages).as_str());
                win.add_output(
                    alloc::format!(
                        "  heap reservado:       {} MiB ({} bytes)",
                        heap_bytes / (1024 * 1024),
                        heap_bytes
                    )
                    .as_str(),
                );
                win.add_output(
                    alloc::format!(
                        "  heap reservado tarea: {} MiB ({} bytes)",
                        heap_reserved / (1024 * 1024),
                        heap_reserved
                    )
                    .as_str(),
                );
                win.add_output(
                    alloc::format!(
                        "  install budget:       {} MiB",
                        install_budget / (1024 * 1024)
                    )
                    .as_str(),
                );
                win.render_terminal();
            }
            return;
        }

        if verb == "web" {
            let mut out = Vec::new();
            let sub = arg_raw.trim();
            let mut parts = sub.split_whitespace();
            let action = Self::ascii_lower(parts.next().unwrap_or(""));

            if action.is_empty() || action == "help" || action == "-h" || action == "--help" {
                out.push(String::from("Web explorer backend:"));
                out.push(String::from("  web backend status"));
                out.push(String::from("  web backend builtin"));
                out.push(String::from("  web backend vaev"));
                out.push(String::from("  web backend webkit"));
                out.push(String::from("  web vaev status"));
                out.push(String::from(
                    "  web vaev input <click x y|scroll d|key K|text T|back|forward|reload>",
                ));
                if WEB_CEF_BRIDGE_ENABLED {
                    out.push(String::from("  web backend cef (legacy alias)"));
                }
                out.push(String::from("  web native status"));
                out.push(String::from("  web native on"));
                out.push(String::from("  web native off"));
                if WEB_CEF_BRIDGE_ENABLED {
                    out.push(String::from("  web webkit status"));
                    out.push(String::from("  web webkit endpoint <http://host:port|auto>"));
                    out.push(String::from("  web webkit ping"));
                    out.push(String::from("  web webkit open <url>"));
                    out.push(String::from("  web webkit frame"));
                    out.push(String::from(
                        "  web webkit input <click x y|scroll d|key K|text T|back|forward|reload>",
                    ));
                    out.push(String::from(
                        "  web cef ... (legacy alias de web webkit)",
                    ));
                    out.push(String::from(
                        "  wry <status|endpoint|ping|open|frame|input> (alias de web webkit)",
                    ));
                }
                out.push(String::from("Tip: el backend activo aplica al boton GO del navegador."));
                out.push(self.web_backend_status_line());
                out.push(alloc::format!(
                    "Web native renderer: {}",
                    if crate::web_engine::is_native_render_enabled() {
                        "ON (DOM/layout/raster interno)"
                    } else {
                        "OFF (preview textual clasico)"
                    }
                ));
                if WEB_CEF_BRIDGE_ENABLED {
                    out.push(alloc::format!("Host endpoint: {}", self.web_proxy_base()));
                }
            } else if action == "backend" {
                let mode = Self::ascii_lower(parts.next().unwrap_or("status"));
                let extra = parts.next();
                if extra.is_some() {
                    out.push(String::from(
                        "Usage: web backend <builtin|vaev|webkit|cef|status>",
                    ));
                } else if mode == "status" {
                    out.push(self.web_backend_status_line());
                    if WEB_CEF_BRIDGE_ENABLED {
                        out.push(alloc::format!("Host endpoint: {}", self.web_proxy_base()));
                    }
                } else if mode == "builtin" {
                    self.web_backend_mode = WebBackendMode::Builtin;
                    out.push(String::from("Web backend: builtin activo."));
                } else if mode == "vaev" {
                    self.web_backend_mode = WebBackendMode::Vaev;
                    out.push(String::from("Web backend: vaev activo."));
                    out.push(alloc::format!(
                        "Vaev bridge mode: {}",
                        crate::web_vaev_bridge::binding_mode()
                    ));
                    if !crate::web_vaev_bridge::feature_enabled() {
                        out.push(String::from(
                            "Vaev bridge no compilado en esta build; GO usara fallback builtin.",
                        ));
                    }
                } else if mode == "webkit" || mode == "wry" || mode == "cef" {
                    if WEB_CEF_BRIDGE_ENABLED {
                        self.web_backend_mode = WebBackendMode::Cef;
                        out.push(String::from("Web backend: webkit activo (host bridge)."));
                        out.push(alloc::format!("WebKit endpoint: {}", self.web_proxy_base()));
                    } else {
                        self.web_backend_mode = WebBackendMode::Builtin;
                        out.push(String::from(
                            "Web backend webkit deshabilitado en esta build (sin dependencia host).",
                        ));
                        out.push(String::from("Web backend: builtin activo."));
                    }
                } else {
                    out.push(String::from(
                        "Usage: web backend <builtin|vaev|webkit|cef|status>",
                    ));
                }
            } else if action == "native" {
                let mode = Self::ascii_lower(parts.next().unwrap_or("status"));
                let extra = parts.next();
                if extra.is_some() {
                    out.push(String::from("Usage: web native <on|off|status>"));
                } else if mode == "status" {
                    out.push(alloc::format!(
                        "Web native renderer: {}",
                        if crate::web_engine::is_native_render_enabled() {
                            "ON (DOM/layout/raster interno)"
                        } else {
                            "OFF (preview textual clasico)"
                        }
                    ));
                } else if mode == "on" {
                    crate::web_engine::set_native_render_enabled(true);
                    out.push(String::from(
                        "Web native renderer ON: GO dibuja superficie DOM/layout/raster interna.",
                    ));
                } else if mode == "off" {
                    crate::web_engine::set_native_render_enabled(false);
                    out.push(String::from(
                        "Web native renderer OFF: vuelve a preview textual clasico.",
                    ));
                } else {
                    out.push(String::from("Usage: web native <on|off|status>"));
                }
            } else if action == "vaev" {
                let cmd = Self::ascii_lower(parts.next().unwrap_or("status"));
                if cmd == "status" {
                    out.push(alloc::format!(
                        "Vaev bridge feature: {}",
                        if crate::web_vaev_bridge::feature_enabled() {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ));
                    out.push(alloc::format!(
                        "Vaev bridge mode: {}",
                        crate::web_vaev_bridge::binding_mode()
                    ));
                    out.push(alloc::format!(
                        "Vaev input bridge: {}",
                        if crate::web_vaev_bridge::input_enabled() {
                            "ON (click/scroll/back/forward/reload)"
                        } else {
                            "OFF (disponible solo en shim embebido)"
                        }
                    ));
                    out.push(String::from(
                        "Tip: usa `web backend vaev` para que GO use el bridge embebido.",
                    ));
                } else if cmd == "input" {
                    let kind = Self::ascii_lower(parts.next().unwrap_or(""));
                    let mut event: Option<crate::web_vaev_bridge::VaevInputEvent> = None;

                    if kind == "click" {
                        let x_raw = parts.next().unwrap_or("");
                        let y_raw = parts.next().unwrap_or("");
                        let extra = parts.next();
                        if !x_raw.is_empty() && !y_raw.is_empty() && extra.is_none() {
                            if let (Ok(x), Ok(y)) = (x_raw.parse::<u32>(), y_raw.parse::<u32>()) {
                                event = Some(crate::web_vaev_bridge::VaevInputEvent::Click {
                                    x,
                                    y,
                                });
                            }
                        }
                    } else if kind == "scroll" {
                        let delta_raw = parts.next().unwrap_or("120");
                        let extra = parts.next();
                        if extra.is_none() {
                            if let Ok(delta) = delta_raw.parse::<i32>() {
                                event = Some(crate::web_vaev_bridge::VaevInputEvent::Scroll {
                                    delta,
                                });
                            }
                        }
                    } else if kind == "key" {
                        let key_raw = parts.next().unwrap_or("Enter");
                        let extra = parts.next();
                        if !key_raw.is_empty() && extra.is_none() {
                            event = Some(crate::web_vaev_bridge::VaevInputEvent::Key {
                                key: String::from(key_raw),
                            });
                        }
                    } else if kind == "text" {
                        let mut text = String::new();
                        for part in parts {
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(part);
                        }
                        if !text.is_empty() {
                            event = Some(crate::web_vaev_bridge::VaevInputEvent::Text { text });
                        }
                    } else if (kind == "back" || kind == "forward" || kind == "reload")
                        && parts.next().is_none()
                    {
                        event = Some(if kind == "back" {
                            crate::web_vaev_bridge::VaevInputEvent::Back
                        } else if kind == "forward" {
                            crate::web_vaev_bridge::VaevInputEvent::Forward
                        } else {
                            crate::web_vaev_bridge::VaevInputEvent::Reload
                        });
                    }

                    if event.is_none() {
                        out.push(String::from(
                            "Usage: web vaev input <click x y|scroll d|key K|text T|back|forward|reload>",
                        ));
                    } else if !crate::web_vaev_bridge::feature_enabled() {
                        out.push(String::from(
                            "Vaev bridge OFF en esta build (feature 'vaev_bridge' desactivado).",
                        ));
                    } else if !crate::web_vaev_bridge::input_enabled() {
                        out.push(String::from(
                            "Vaev input bridge no disponible en modo externo actual.",
                        ));
                    } else {
                        let mut pump = || self.pump_ui_while_blocked_net();
                        let result =
                            crate::web_vaev_bridge::dispatch_input(event.unwrap(), &mut pump);
                        out.push(String::from("Vaev input enviado."));
                        if let Some(page) = result.output.as_ref() {
                            out.push(alloc::format!("Status: {}", page.status));
                            out.push(alloc::format!("URL: {}", page.final_url));
                            for line in page.lines.iter().take(3) {
                                out.push(alloc::format!("  {}", line));
                            }
                        }
                        if let Some(note) = result.note.as_ref() {
                            out.push(alloc::format!("Nota: {}", note));
                        }

                        if let Some(browser_id) = self.browser_target_for_web_input() {
                            if self.browser_apply_vaev_result(browser_id, result) {
                                out.push(alloc::format!("Browser actualizado: ventana #{}", browser_id));
                                self.paint();
                            }
                        }
                    }
                } else {
                    out.push(String::from("Usage: web vaev <status|input ...>"));
                }
            } else if action == "cef" || action == "webkit" {
                if !WEB_CEF_BRIDGE_ENABLED {
                    out.push(String::from(
                        "web webkit deshabilitado en esta build (ruta nativa local activa).",
                    ));
                    out.push(String::from(
                        "Usa: web backend builtin  |  web native on  |  GO en Web Explorer.",
                    ));
                } else {
                let cmd = Self::ascii_lower(parts.next().unwrap_or("status"));
                if cmd == "status" {
                    out.push(self.web_backend_status_line());
                    out.push(alloc::format!("WebKit endpoint activo: {}", self.web_proxy_base()));
                    out.push(alloc::format!(
                        "WebKit endpoint config: {}",
                        if self.web_proxy_endpoint_base.trim().is_empty() {
                            WEB_PROXY_DEFAULT_BASE
                        } else {
                            self.web_proxy_endpoint_base.trim()
                        }
                    ));
                    out.push(String::from(
                        "Tip: usa `web backend webkit` para que GO use host WebKit/Wry.",
                    ));
                } else if cmd == "endpoint" {
                    let endpoint = parts.next().unwrap_or("").trim();
                    let extra = parts.next();
                    if endpoint.is_empty() || extra.is_some() {
                        out.push(String::from("Usage: web webkit endpoint <http://host:port|auto>"));
                        out.push(alloc::format!("Actual config: {}", self.web_proxy_endpoint_base));
                        out.push(alloc::format!("Activo: {}", self.web_proxy_base()));
                    } else {
                        if Self::web_proxy_is_auto_hint(endpoint) {
                            self.web_proxy_endpoint_base = String::from(WEB_PROXY_DEFAULT_BASE);
                            out.push(String::from("WebKit endpoint en modo auto."));
                        } else {
                            self.web_proxy_endpoint_base = String::from(endpoint);
                            out.push(String::from("WebKit endpoint manual actualizado."));
                        }
                        out.push(alloc::format!("WebKit endpoint activo: {}", self.web_proxy_base()));
                    }
                } else if cmd == "ping" {
                    let (base, raw, tried_bases) = self.web_cef_request_first_reachable("status");
                    match raw {
                        Some(raw) => {
                            let (code, body) = Self::parse_http_status_and_body(raw.as_str());
                            out.push(alloc::format!("WebKit ping: HTTP {:?}", code));
                            if let Some(base) = base {
                                out.push(alloc::format!("Endpoint: {}", base));
                            }
                            for line in body.lines().take(10) {
                                out.push(String::from(line.trim_end()));
                            }
                        }
                        None => {
                            out.push(String::from(
                                "WebKit ping fallo: no se pudo conectar al endpoint.",
                            ));
                            out.push(String::from("Endpoints intentados:"));
                            for base in tried_bases.iter().take(4) {
                                out.push(alloc::format!("  - {}", base));
                            }
                        }
                    }
                } else if cmd == "open" {
                    let url = parts.next().unwrap_or("").trim();
                    let extra = parts.next();
                    if url.is_empty() || extra.is_some() {
                        out.push(String::from("Usage: web webkit open <url>"));
                    } else {
                        let (base, raw, tried_bases) = self.web_cef_request_first_reachable(
                            alloc::format!("open?url={}", Self::url_encode_component(url)).as_str(),
                        );
                        match raw {
                            Some(raw) => {
                                let (code, body) = Self::parse_http_status_and_body(raw.as_str());
                                out.push(alloc::format!("WebKit open: HTTP {:?}", code));
                                if let Some(base) = base {
                                    out.push(alloc::format!("Endpoint: {}", base));
                                    if let Some(surface) =
                                        self.browser_fetch_cef_frame_with_base(base.as_str())
                                    {
                                        out.push(alloc::format!(
                                            "Frame: {}x{} ({})",
                                            surface.width, surface.height, surface.source
                                        ));
                                    } else {
                                        out.push(String::from(
                                            "Frame: no disponible (host sin /frame o timeout).",
                                        ));
                                    }
                                }
                                for line in body.lines().take(10) {
                                    out.push(String::from(line.trim_end()));
                                }
                            }
                            None => {
                                out.push(String::from(
                                    "WebKit open fallo: no se pudo conectar al endpoint.",
                                ));
                                out.push(String::from("Endpoints intentados:"));
                                for base in tried_bases.iter().take(4) {
                                    out.push(alloc::format!("  - {}", base));
                                }
                            }
                        }
                    }
                } else if cmd == "frame" {
                    let mut frame_found = false;
                    for base in self.web_proxy_candidate_bases().into_iter() {
                        if let Some(surface) = self.browser_fetch_cef_frame_with_base(base.as_str()) {
                            self.web_proxy_endpoint_base = base.clone();
                            out.push(String::from("WebKit frame: OK"));
                            out.push(alloc::format!("Endpoint: {}", base));
                            out.push(alloc::format!(
                                "Frame: {}x{} ({})",
                                surface.width, surface.height, surface.source
                            ));
                            frame_found = true;
                            break;
                        }
                    }
                    if !frame_found {
                        out.push(String::from(
                            "WebKit frame: no disponible (host sin /frame o timeout).",
                        ));
                    }
                } else if cmd == "input" {
                    let kind = Self::ascii_lower(parts.next().unwrap_or(""));
                    let mut query = String::new();
                    if kind == "click" {
                        let x = parts.next().unwrap_or("");
                        let y = parts.next().unwrap_or("");
                        if !x.is_empty() && !y.is_empty() {
                            query = alloc::format!("input?type=click&x={}&y={}", x, y);
                        }
                    } else if kind == "scroll" {
                        let d = parts.next().unwrap_or("120");
                        query = alloc::format!("input?type=scroll&delta={}", d);
                    } else if kind == "key" {
                        let k = parts.next().unwrap_or("Enter");
                        query = alloc::format!("input?type=key&key={}", Self::url_encode_component(k));
                    } else if kind == "text" {
                        let mut text = String::new();
                        for part in parts {
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(part);
                        }
                        if !text.is_empty() {
                            query = alloc::format!(
                                "input?type=text&text={}",
                                Self::url_encode_component(text.as_str())
                            );
                        }
                    } else if kind == "back" || kind == "forward" || kind == "reload" {
                        query = alloc::format!("input?type={}", kind);
                    }

                    if query.is_empty() {
                        out.push(String::from(
                            "Usage: web webkit input <click x y|scroll d|key K|text T|back|forward|reload>",
                        ));
                    } else {
                        let (base, raw, tried_bases) =
                            self.web_cef_request_first_reachable(query.as_str());
                        match raw {
                            Some(raw) => {
                                let (code, body) = Self::parse_http_status_and_body(raw.as_str());
                                out.push(alloc::format!("WebKit input: HTTP {:?}", code));
                                if let Some(base) = base {
                                    out.push(alloc::format!("Endpoint: {}", base));
                                }
                                if !body.trim().is_empty() {
                                    for line in body.lines().take(6) {
                                        out.push(String::from(line.trim_end()));
                                    }
                                }
                            }
                            None => {
                                out.push(String::from(
                                    "WebKit input fallo: no se pudo conectar al endpoint.",
                                ));
                                for base in tried_bases.iter().take(4) {
                                    out.push(alloc::format!("  - {}", base));
                                }
                            }
                        }
                    }
                } else {
                    out.push(String::from(
                        "Usage: web webkit <status|endpoint|ping|open|frame|input>",
                    ));
                }
                }
            } else if action == "proxy" {
                if WEB_CEF_BRIDGE_ENABLED {
                    self.web_backend_mode = WebBackendMode::Cef;
                    out.push(String::from("`web proxy` ahora es alias de backend WebKit."));
                    out.push(alloc::format!("WebKit endpoint: {}", self.web_proxy_base()));
                } else {
                    self.web_backend_mode = WebBackendMode::Builtin;
                    out.push(String::from(
                        "`web proxy` no aplica en build nativa local (sin host bridge).",
                    ));
                    out.push(String::from("Web backend: builtin activo."));
                }
            } else {
                out.push(String::from(
                    "Usage: web backend <builtin|vaev|webkit|cef|status>",
                ));
            }

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in out.iter() {
                    win.add_output(line.as_str());
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "doom" {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.add_output("DOOM: iniciando desde GUI...");
                win.render_terminal();
            }
            self.paint();
            let launch_result = crate::launch_doom_uefi();
            let mut shell_result = None;
            if let Err(err) = &launch_result {
                if crate::doom_error_requires_shell(err.as_str()) {
                    shell_result = Some(crate::launch_doom_via_shell());
                }
            }
            let _ = crate::restore_gui_after_external_app();
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                match &launch_result {
                    Ok(path) => {
                        win.add_output(
                            alloc::format!("DOOM: ejecucion terminada y regreso ({})", path).as_str(),
                        );
                    }
                    Err(err) => {
                        if let Some(result) = &shell_result {
                            win.add_output(
                                "DOOM: requiere UEFI Shell, ejecutando script automatico...",
                            );
                            match result {
                                Ok(path) => win.add_output(
                                    alloc::format!(
                                        "DOOM via Shell: sesion terminada y regreso ({})",
                                        path
                                    )
                                    .as_str(),
                                ),
                                Err(shell_err) => win.add_output(
                                    alloc::format!(
                                        "DOOM: no pudo iniciar: {} | Shell fallo: {}",
                                        err, shell_err
                                    )
                                    .as_str(),
                                ),
                            }
                        } else {
                            win.add_output(alloc::format!("DOOM: no pudo iniciar: {}", err).as_str());
                        }
                    }
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "shell" {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.add_output("UEFI Shell: iniciando desde GUI...");
                win.render_terminal();
            }
            self.paint();
            let launch_result = crate::launch_uefi_shell();
            let _ = crate::restore_gui_after_external_app();
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                match launch_result {
                    Ok(path) => {
                        win.add_output(
                            alloc::format!(
                                "UEFI Shell: sesion terminada y regreso ({})",
                                path
                            )
                            .as_str(),
                        );
                    }
                    Err(err) => {
                        win.add_output(
                            alloc::format!("UEFI Shell: no pudo iniciar: {}", err).as_str(),
                        );
                    }
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "entry" || verb == "openinst" {
            let mut out = Vec::new();
            let arg = arg_raw.trim();

            if arg.is_empty() {
                out.push(String::from("Usage: entry <archivo> [app_id]"));
                out.push(String::from("Ejemplo: entry APPS.ZIP MIAPP"));
                out.push(String::from("Ejemplo: entry INSTALADOR.EXE"));
            } else {
                let mut parts = arg.split_whitespace();
                let file_name = parts.next().unwrap_or("");
                let app_id_arg = parts.next();

                if file_name.is_empty() || parts.next().is_some() {
                    out.push(String::from("Usage: entry <archivo> [app_id]"));
                } else {
                    let lower = Self::ascii_lower(file_name);
                    if !lower.ends_with(".efi") {
                        let mut reroute = alloc::format!("install {}", file_name);
                        if let Some(app_id) = app_id_arg {
                            reroute.push(' ');
                            reroute.push_str(app_id);
                        }
                        out.push(alloc::format!("Entry: paquete detectado -> {}", reroute));
                        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                            for line in out.iter() {
                                win.add_output(line.as_str());
                            }
                            win.render_terminal();
                        }
                        self.execute_command(win_id, reroute.as_str());
                        return;
                    }

                    if lower.ends_with(".efi") {
                        if lower.contains("shell") {
                            out.push(String::from("Entry: UEFI Shell detectado -> shell"));
                            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                for line in out.iter() {
                                    win.add_output(line.as_str());
                                }
                                win.render_terminal();
                            }
                            self.execute_command(win_id, "shell");
                            return;
                        }
                        if lower.contains("doom") || lower.contains("bootx64") {
                            out.push(String::from("Entry: UEFI app detectada -> doom"));
                            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                for line in out.iter() {
                                    win.add_output(line.as_str());
                                }
                                win.render_terminal();
                            }
                            self.execute_command(win_id, "doom");
                            return;
                        }

                        out.push(String::from(
                            "Entry EFI: soporte generico limitado. Usa 'doom' o 'shell'.",
                        ));
                    }
                }
            }

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in out.iter() {
                    win.add_output(line.as_str());
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "disks" {
            let devices = crate::fat32::Fat32::detect_uefi_block_devices();
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                if devices.is_empty() {
                    win.add_output("No UEFI BlockIO devices detected.");
                } else {
                    win.add_output("Detected BlockIO devices:");
                    for dev in devices.iter() {
                        let media = if dev.removable { "USB" } else { "NVME/HDD" };
                        let scope = if dev.logical_partition { "part" } else { "disk" };
                        win.add_output(
                            alloc::format!(
                                "  [{}] {} {} {} MiB",
                                dev.index,
                                media,
                                scope,
                                dev.total_mib
                            )
                            .as_str(),
                        );
                    }
                    win.add_output("Use 'mount <index>' to probe and mount FAT32.");
                }
            }
            return;
        }

        if verb == "vols" {
            let volumes = crate::fat32::Fat32::detect_uefi_fat_volumes();
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                if volumes.is_empty() {
                    win.add_output("No FAT32 volumes detected on USB/NVMe/HDD.");
                } else {
                    win.add_output("Detected FAT32 volumes:");
                    for vol in volumes.iter() {
                        let label = Self::volume_label_from_bytes(&vol.volume_label)
                            .unwrap_or(String::from("NO_LABEL"));
                        let media = if vol.removable { "USB" } else { "NVME/HDD" };
                        let scope = if vol.logical_partition { "part" } else { "disk" };
                        win.add_output(
                            alloc::format!(
                                "  [{}] {} {} {} MiB '{}' LBA {}",
                                vol.index,
                                media,
                                scope,
                                vol.total_mib,
                                label,
                                vol.partition_start
                            )
                            .as_str(),
                        );
                    }
                }
            }
            return;
        }

        if verb == "mount" {
            if arg_raw.is_empty() {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.add_output("Usage: mount <index> (use 'disks').");
                }
                return;
            }

            let index = match arg_raw.parse::<usize>() {
                Ok(v) => v,
                Err(_) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.add_output("Invalid index. Usage: mount <index>.");
                    }
                    return;
                }
            };

            match fat.mount_uefi_block_device(index) {
                Ok(vol) => {
                    self.current_volume_device_index = Some(index);
                    let label = Self::volume_label_from_bytes(&vol.volume_label)
                        .unwrap_or(alloc::format!("VOL{}", vol.index));
                    let media = if vol.removable { "USB" } else { "NVME/HDD" };
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.current_dir_cluster = vol.root_cluster;
                        win.current_path = alloc::format!("{}/", label);
                        win.add_output(
                            alloc::format!(
                                "Mounted [{}] {} '{}' root={} LBA={}.",
                                vol.index,
                                media,
                                label,
                                vol.root_cluster,
                                vol.partition_start
                            )
                            .as_str(),
                        );
                    }
                }
                Err(err) => {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.add_output(err);
                    }
                }
            }
            return;
        }

        if verb == "unmount" || verb == "umount" {
            if !arg_raw.is_empty() {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.add_output("Usage: unmount");
                }
                return;
            }

            let msg = self.unmount_active_volume();
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.add_output(msg.as_str());
                win.render_terminal();
            }
            return;
        }

        if verb == "cpdev" {
            let mut out = Vec::new();
            let mut args = arg_raw.split_whitespace();
            let src_dev_raw = args.next().unwrap_or("");
            let src_path = args.next().unwrap_or("");
            let dst_dev_raw = args.next().unwrap_or("");
            let dst_path = args.next().unwrap_or("");
            let extra = args.next().is_some();

            if src_dev_raw.is_empty() || src_path.is_empty() || dst_dev_raw.is_empty() || dst_path.is_empty() || extra {
                out.push(String::from("Usage: cpdev <src_dev> <src_path> <dst_dev> <dst_path>"));
                out.push(String::from("Example: cpdev 0 /SNOTE.DEB 1 /APPS/SNOTE.DEB"));
                out.push(String::from("Tip: usa 'disks' para ver indices."));
            } else {
                let src_dev = match src_dev_raw.parse::<usize>() {
                    Ok(v) => v,
                    Err(_) => {
                        out.push(String::from("CPDEV error: src_dev invalido."));
                        usize::MAX
                    }
                };
                let dst_dev = match dst_dev_raw.parse::<usize>() {
                    Ok(v) => v,
                    Err(_) => {
                        out.push(String::from("CPDEV error: dst_dev invalido."));
                        usize::MAX
                    }
                };

                if out.is_empty() {
                    let mut src_fat = crate::fat32::Fat32::new();
                    let mut dst_fat = crate::fat32::Fat32::new();

                    let src_mount = src_fat.mount_uefi_block_device(src_dev);
                    let dst_mount = dst_fat.mount_uefi_block_device(dst_dev);

                    match (src_mount, dst_mount) {
                        (Ok(src_vol), Ok(dst_vol)) => {
                            let src_root = src_fat.root_cluster;
                            let dst_root = dst_fat.root_cluster;
                            let src_resolved = Self::resolve_terminal_parent_and_leaf(
                                &mut src_fat,
                                src_root,
                                src_path,
                            );
                            let dst_resolved = Self::resolve_terminal_parent_and_leaf(
                                &mut dst_fat,
                                dst_root,
                                dst_path,
                            );

                            match (src_resolved, dst_resolved) {
                                (Ok((src_dir, src_leaf)), Ok((dst_dir, dst_leaf))) => {
                                    match src_fat.read_dir_entries(src_dir) {
                                        Ok(entries) => {
                                            let mut src_entry: Option<crate::fs::DirEntry> = None;
                                            for entry in entries.iter() {
                                                if !entry.valid || entry.file_type != FileType::File {
                                                    continue;
                                                }
                                                if entry.matches_name(src_leaf.as_str())
                                                    || entry.full_name().eq_ignore_ascii_case(src_leaf.as_str())
                                                {
                                                    src_entry = Some(*entry);
                                                    break;
                                                }
                                            }

                                            if let Some(source) = src_entry {
                                                if source.size == 0 {
                                                    out.push(String::from("CPDEV error: origen vacio."));
                                                } else if source.size as usize > COPY_MAX_FILE_BYTES {
                                                    out.push(alloc::format!(
                                                        "CPDEV error: archivo demasiado grande (max {} bytes).",
                                                        COPY_MAX_FILE_BYTES
                                                    ));
                                                } else {
                                                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                                        win.add_output(
                                                            alloc::format!(
                                                                "CPDEV: alloc {} bytes...",
                                                                source.size
                                                            ).as_str(),
                                                        );
                                                        win.render_terminal();
                                                    }
                                                    self.paint();
                                                    let mut raw = match Self::try_alloc_zeroed(source.size as usize) {
                                                        Ok(v) => v,
                                                        Err(err) => {
                                                            out.push(alloc::format!(
                                                                "CPDEV error: {}",
                                                                err
                                                            ));
                                                            Vec::new()
                                                        }
                                                    };
                                                    if out.is_empty() {
                                                        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                                            win.add_output(
                                                                alloc::format!(
                                                                    "CPDEV: leyendo {} bytes del origen, puede tardar...",
                                                                    source.size
                                                                ).as_str(),
                                                            );
                                                            win.render_terminal();
                                                        }
                                                        self.paint();
                                                        match src_fat.read_file_sized(
                                                            source.cluster,
                                                            source.size as usize,
                                                            &mut raw,
                                                        ) {
                                                            Ok(len) => {
                                                                raw.truncate(len);
                                                                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                                                    win.add_output(
                                                                        alloc::format!(
                                                                            "CPDEV: leido {} bytes OK. Escribiendo destino...",
                                                                            len
                                                                        ).as_str(),
                                                                    );
                                                                    win.render_terminal();
                                                                }
                                                                self.paint();
                                                                match dst_fat.write_text_file_in_dir(
                                                                    dst_dir,
                                                                    dst_leaf.as_str(),
                                                                    raw.as_slice(),
                                                                ) {
                                                                    Ok(()) => {
                                                                        let src_label = Self::volume_label_from_bytes(&src_vol.volume_label)
                                                                            .unwrap_or_else(|| alloc::format!("DEV{}", src_dev));
                                                                        let dst_label = Self::volume_label_from_bytes(&dst_vol.volume_label)
                                                                            .unwrap_or_else(|| alloc::format!("DEV{}", dst_dev));
                                                                        out.push(alloc::format!(
                                                                            "CPDEV: {} bytes {}:{} -> {}:{}",
                                                                            len, src_label, src_path, dst_label, dst_path
                                                                        ));
                                                                    }
                                                                    Err(err) => out.push(alloc::format!(
                                                                        "CPDEV error escribiendo destino: {}",
                                                                        err
                                                                    )),
                                                                }
                                                            }
                                                            Err(err) => out.push(alloc::format!(
                                                                "CPDEV error leyendo origen: {}",
                                                                err
                                                            )),
                                                        }
                                                    }
                                                }
                                            } else {
                                                out.push(String::from("CPDEV error: archivo origen no encontrado."));
                                            }
                                        }
                                        Err(err) => out.push(alloc::format!(
                                            "CPDEV error leyendo directorio origen: {}",
                                            err
                                        )),
                                    }
                                }
                                (Err(err), _) => {
                                    out.push(alloc::format!("CPDEV error ruta origen: {}", err));
                                }
                                (_, Err(err)) => {
                                    out.push(alloc::format!("CPDEV error ruta destino: {}", err));
                                }
                            }
                        }
                        (Err(err), _) => out.push(alloc::format!("CPDEV error montando origen: {}", err)),
                        (_, Err(err)) => out.push(alloc::format!("CPDEV error montando destino: {}", err)),
                    }
                }
            }

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in out.iter() {
                    win.add_output(line.as_str());
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "net" {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                let sub = arg_raw.trim();
                let sub_lower = Self::ascii_lower(sub);

                if sub_lower == "dhcp" {
                    win.add_output(alloc::format!("Net: {}", crate::net::set_dhcp_mode()).as_str());
                    win.render_terminal();
                    return;
                }

                if sub_lower == "static" {
                    win.add_output(alloc::format!("Net: {}", crate::net::use_default_static_ipv4()).as_str());
                    win.render_terminal();
                    return;
                }

                if let Some(rest) = sub.strip_prefix("static ") {
                    let mut parts = rest.split_whitespace();
                    let ip = parts.next();
                    let prefix = parts.next();
                    let gateway = parts.next();
                    let extra = parts.next();

                    if let (Some(ip), Some(prefix), Some(gateway), None) = (ip, prefix, gateway, extra) {
                        match crate::net::set_static_ipv4_from_text(ip, prefix, gateway) {
                            Ok(msg) => win.add_output(alloc::format!("Net: {}", msg).as_str()),
                            Err(err) => win.add_output(alloc::format!("Net: {}", err).as_str()),
                        }
                    } else {
                        win.add_output("Usage: net static <ip> <prefijo> <gateway>");
                    }
                    win.render_terminal();
                    return;
                }

                if sub_lower.starts_with("https") {
                    let mut parts = sub.split_whitespace();
                    let _verb = parts.next();
                    let mode = parts.next().unwrap_or("status");
                    if mode.eq_ignore_ascii_case("on") {
                        win.add_output(alloc::format!("Net: {}", crate::net::set_https_mode_proxy()).as_str());
                    } else if mode.eq_ignore_ascii_case("off") {
                        win.add_output(alloc::format!("Net: {}", crate::net::set_https_mode_disabled()).as_str());
                    } else if mode.eq_ignore_ascii_case("status") {
                        win.add_output(alloc::format!("Net: HTTPS mode -> {}", crate::net::get_https_mode()).as_str());
                    } else {
                        win.add_output("Usage: net https <on|off|status>");
                    }
                    win.render_terminal();
                    return;
                }

                if sub_lower == "diag" {
                    if let Some(diag) = crate::intel_net::get_diagnostics() {
                        let rxq_en = (diag.rxdctl & 0x0200_0000) != 0;
                        let txq_en = (diag.txdctl & 0x0200_0000) != 0;
                        let link = (diag.status & 0x0000_0002) != 0;

                        win.add_output(
                            alloc::format!(
                                "NetDiag: PCI_CMD={:#010x} STATUS={:#010x} CTRL={:#010x} CTRL_EXT={:#010x}",
                                diag.pci_cmd, diag.status, diag.ctrl, diag.ctrl_ext
                            )
                            .as_str(),
                        );
                        win.add_output(
                            alloc::format!(
                                "NetDiag: RX RXCTRL={:#010x} RCTL={:#010x} RXDCTL={:#010x} RDH={} RDT={} RDLEN={} enabled={}",
                                diag.rxctrl, diag.rctl, diag.rxdctl, diag.rdh, diag.rdt, diag.rdlen, rxq_en
                            )
                            .as_str(),
                        );
                        win.add_output(
                            alloc::format!(
                                "NetDiag: TX TCTL={:#010x} TXDCTL={:#010x} TDH={} TDT={} TDLEN={} enabled={}",
                                diag.tctl, diag.txdctl, diag.tdh, diag.tdt, diag.tdlen, txq_en
                            )
                            .as_str(),
                        );
                        win.add_output(
                            alloc::format!(
                                "NetDiag: IMS={:#010x} IMC={:#010x} LinkUp={} rx_cur={} tx_cur={}",
                                diag.ims, diag.imc, link, diag.rx_cur, diag.tx_cur
                            )
                            .as_str(),
                        );
                        win.add_output(
                            alloc::format!(
                                "NetDiag: SRRCTL={:#010x} HW_GPRC={} HW_GPTC={}",
                                diag.srrctl, diag.gprc, diag.gptc
                            )
                            .as_str(),
                        );
                        win.add_output(
                            alloc::format!(
                                "NetDiag: RXDESC[cur] addr={:#x} len={} status={:#04x} cso={:#04x} cmd={:#04x} css={:#04x} special={:#06x}",
                                diag.rx_desc_addr,
                                diag.rx_desc_length,
                                diag.rx_desc_status,
                                diag.rx_desc_cso,
                                diag.rx_desc_cmd,
                                diag.rx_desc_css,
                                diag.rx_desc_special
                            )
                            .as_str(),
                        );
                    } else {
                        win.add_output("NetDiag: Intel Ethernet no inicializado.");
                    }
                    win.render_terminal();
                    return;
                }

                if !sub.is_empty() && sub_lower != "mode" {
                    win.add_output("Usage: net [dhcp|static|static <ip> <prefijo> <gateway>|mode|https|diag]");
                    win.render_terminal();
                    return;
                }

                let dhcp_status = unsafe { crate::net::DHCP_STATUS };
                let (s_ip, s_prefix, s_gw) = crate::net::get_static_ipv4_config();
                win.add_output(alloc::format!("Net: transporte activo -> {}", crate::net::get_active_transport()).as_str());
                win.add_output(alloc::format!("Net: failover policy -> {}", crate::net::get_failover_policy()).as_str());
                win.add_output(alloc::format!("Net: modo IP -> {}", crate::net::get_network_mode()).as_str());
                win.add_output(alloc::format!("Net: HTTPS mode -> {}", crate::net::get_https_mode()).as_str());
                win.add_output(alloc::format!("Net: estado IP -> {}", dhcp_status).as_str());
                win.add_output(
                    alloc::format!(
                        "Net: perfil fija -> {}.{}.{}.{}/{} gw {}.{}.{}.{}",
                        s_ip[0],
                        s_ip[1],
                        s_ip[2],
                        s_ip[3],
                        s_prefix,
                        s_gw[0],
                        s_gw[1],
                        s_gw[2],
                        s_gw[3]
                    )
                    .as_str(),
                );
                if let Some(ip) = crate::net::get_ip_address() {
                    win.add_output(alloc::format!("Net: IP -> {}", ip).as_str());
                } else {
                    win.add_output("Net: IP -> (sin asignar)");
                }
                if let Some(gw) = crate::net::get_gateway() {
                    win.add_output(alloc::format!("Net: Gateway -> {}", gw).as_str());
                } else {
                    win.add_output("Net: Gateway -> (none)");
                }
                if crate::intel_net::get_model_name().is_some() {
                    let (rx, tx) = crate::net::get_packet_stats();
                    win.add_output(
                        alloc::format!(
                            "Net: Ethernet link -> {}",
                            if crate::intel_net::is_link_up() { "UP" } else { "DOWN" }
                        )
                        .as_str(),
                    );
                    win.add_output(alloc::format!("Net: Ethernet packets RX={} TX={}", rx, tx).as_str());
                }
                if crate::intel_wifi::is_present() {
                    win.add_output(
                        alloc::format!(
                            "Net: WiFi -> {} | datapath={}",
                            crate::intel_wifi::get_status(),
                            if crate::intel_wifi::is_data_path_ready() { "ready" } else { "pending" }
                        )
                        .as_str(),
                    );
                    if let Some((ssid, len)) = crate::intel_wifi::connected_ssid() {
                        let ssid_str = core::str::from_utf8(&ssid[..len]).unwrap_or("<invalid-ssid>");
                        win.add_output(alloc::format!("Net: WiFi conectado -> {}", ssid_str).as_str());
                    }
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "wifi" {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                let sub = arg_raw.trim();
                let sub_lower = Self::ascii_lower(sub);

                if sub.is_empty() {
                    let model = crate::intel_wifi::get_model_name().unwrap_or("Intel WiFi (unknown)");
                    win.add_output(alloc::format!("WiFi: model -> {}", model).as_str());
                    win.add_output(alloc::format!("WiFi: status -> {}", crate::intel_wifi::get_status()).as_str());
                    win.add_output(
                        alloc::format!(
                            "WiFi: datapath ready -> {}",
                            if crate::intel_wifi::is_data_path_ready() { "yes" } else { "no (phase1)" }
                        )
                        .as_str(),
                    );
                    win.add_output(alloc::format!("WiFi: last scan -> {}", crate::intel_wifi::get_last_scan_status()).as_str());
                } else if sub_lower == "scan" {
                    let status = crate::intel_wifi::scan_networks();
                    win.add_output(alloc::format!("WiFi: {}", status).as_str());
                    let count = crate::intel_wifi::get_last_scan_count();
                    if count == 0 {
                        win.add_output("WiFi: no hay redes detectadas.");
                    } else {
                        for i in 0..count {
                            if let Some(entry) = crate::intel_wifi::get_scan_entry(i) {
                                win.add_output(
                                    alloc::format!(
                                        "WiFi[{}]: '{}' RSSI={}dBm CH={} {}",
                                        i,
                                        entry.ssid_str(),
                                        entry.rssi_dbm,
                                        entry.channel,
                                        if entry.secure { "secure" } else { "open" }
                                    )
                                    .as_str(),
                                );
                            }
                        }
                    }
                } else if let Some(rest) = sub.strip_prefix("connect ") {
                    let mut parts = rest.trim().splitn(2, ' ');
                    let ssid = parts.next().unwrap_or("").trim();
                    let psk = parts.next().unwrap_or("").trim();
                    if ssid.is_empty() {
                        win.add_output("Usage: wifi connect <ssid> <clave>");
                    } else {
                        match crate::intel_wifi::configure_profile(ssid, psk) {
                            Ok(msg) => win.add_output(alloc::format!("WiFi: {}", msg).as_str()),
                            Err(err) => {
                                win.add_output(alloc::format!("WiFi: {}", err).as_str());
                                win.render_terminal();
                                return;
                            }
                        }
                        let res = crate::intel_wifi::connect_profile();
                        win.add_output(alloc::format!("WiFi: {}", res).as_str());
                    }
                } else if sub_lower == "disconnect" {
                    win.add_output(alloc::format!("WiFi: {}", crate::intel_wifi::disconnect()).as_str());
                } else if sub_lower == "profile" {
                    if let Some(profile) = crate::intel_wifi::get_profile_info() {
                        win.add_output(
                            alloc::format!(
                                "WiFi: perfil '{}' secure={}",
                                profile.ssid_str(),
                                if profile.secure { "yes" } else { "no" }
                            )
                            .as_str(),
                        );
                    } else {
                        win.add_output("WiFi: sin perfil configurado.");
                    }
                } else if sub_lower == "profile clear" {
                    win.add_output(alloc::format!("WiFi: {}", crate::intel_wifi::clear_profile()).as_str());
                } else if let Some(mode) = sub_lower.strip_prefix("failover ") {
                    if mode == "ethernet" {
                        crate::net::set_failover_policy_ethernet_first();
                        win.add_output("WiFi: failover policy -> EthernetFirst");
                    } else if mode == "wifi" {
                        crate::net::set_failover_policy_wifi_first();
                        win.add_output("WiFi: failover policy -> WifiFirst");
                    } else if mode == "status" {
                        win.add_output(alloc::format!("WiFi: failover policy -> {}", crate::net::get_failover_policy()).as_str());
                    } else {
                        win.add_output("Usage: wifi failover <ethernet|wifi|status>");
                    }
                } else {
                    win.add_output("Usage: wifi [scan|connect|disconnect|profile|profile clear|failover]");
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "fetch" {
            let mut out = Vec::new();
            let mut url = String::new();
            let mut output_name: Option<String> = None;
            let mut repo_mode = false;

            let arg = arg_raw.trim();
            if arg.is_empty() {
                out.push(String::from("Usage:"));
                out.push(String::from("  fetch <url> [file_8_3]"));
                out.push(String::from("  fetch repo <owner/repo> [path] [branch] [file_8_3]"));
                out.push(String::from("Examples:"));
                out.push(String::from("  fetch https://example.com/script.rb SCRIPT.RB"));
                out.push(String::from("  fetch repo ruby/ruby README.md master README.TXT"));
            } else if let Some(rest) = arg.strip_prefix("repo ") {
                repo_mode = true;
                let mut parts = rest.split_whitespace();
                let owner_repo = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("README.md");
                let branch = parts.next().unwrap_or("main");
                let custom_out = parts.next();

                if owner_repo.is_empty() || !owner_repo.contains('/') {
                    out.push(String::from("Usage: fetch repo <owner/repo> [path] [branch] [file_8_3]"));
                } else {
                    url = alloc::format!(
                        "https://raw.githubusercontent.com/{}/{}/{}",
                        owner_repo,
                        branch,
                        path
                    );
                    let default_name = path.rsplit('/').next().unwrap_or("README.md");
                    output_name = Some(match custom_out {
                        Some(name) => Self::normalize_to_short_filename(name, "REPO", "TXT"),
                        None => Self::normalize_to_short_filename(default_name, "REPO", "TXT"),
                    });
                }
            } else {
                let mut parts = arg.split_whitespace();
                let target_url = parts.next().unwrap_or("");
                if target_url.is_empty() {
                    out.push(String::from("Usage: fetch <url> [file_8_3]"));
                } else {
                    url = String::from(target_url);
                    if let Some(name) = parts.next() {
                        output_name = Some(Self::normalize_to_short_filename(name, "FETCH", "TXT"));
                    }
                    if parts.next().is_some() {
                        out.push(String::from("Usage: fetch <url> [file_8_3]"));
                    }
                }
            }

            if out.is_empty() {
                if !Self::is_http_url(url.as_str()) {
                    out.push(String::from("Fetch error: URL must start with http:// or https://"));
                } else {
                    if fat.bytes_per_sector == 0 {
                        if self.manual_unmount_lock {
                            out.push(String::from(
                                "Fetch error: volume desmontado. Usa 'mount <n>' primero.",
                            ));
                        } else if !fat.init() {
                            out.push(String::from(
                                "Fetch error: FAT32 not available. Use 'disks' and 'mount <n>'.",
                            ));
                        }
                    } else {
                        let current_cluster = match self.windows.iter().find(|w| w.id == win_id) {
                            Some(win) => {
                                if win.current_dir_cluster == 0 {
                                    fat.root_cluster
                                } else {
                                    win.current_dir_cluster
                                }
                            }
                            None => fat.root_cluster,
                        };

                        let file_name =
                            output_name.unwrap_or_else(|| Self::derive_filename_from_url(url.as_str()));
                        let request_urls = Self::build_fetch_url_candidates(url.as_str());
                        if request_urls.is_empty() {
                            out.push(String::from("Fetch error: invalid URL."));
                        } else {
                            out.push(alloc::format!("Fetch: {}", request_urls[0]));

                            let mut selected_payload: Option<Vec<u8>> = None;
                            let mut selected_url: Option<String> = None;
                            let mut hard_error = false;

                            let mut pump = || self.pump_ui_while_blocked_net();
                            for (idx, candidate_url) in request_urls.iter().enumerate() {
                                if idx > 0 {
                                    out.push(alloc::format!("Fetch retry: {}", candidate_url));
                                }

                                let Some(raw) =
                                    crate::net::http_get_request_bytes(candidate_url.as_str(), &mut pump)
                                else {
                                    continue;
                                };
                                let (status_code, payload) =
                                    Self::extract_http_status_and_body_bytes(raw.as_slice());

                                if let Some(code) = status_code {
                                    if (300..400).contains(&code) {
                                        out.push(alloc::format!(
                                            "Fetch error: HTTP {} redirect not supported yet. Use final URL.",
                                            code
                                        ));
                                        hard_error = true;
                                        break;
                                    } else if code >= 400 {
                                        if code == 404 && idx + 1 < request_urls.len() {
                                            continue;
                                        }
                                        out.push(alloc::format!("Fetch error: HTTP {}", code));
                                        hard_error = true;
                                        break;
                                    }
                                }

                                selected_payload = Some(payload);
                                selected_url = Some(candidate_url.clone());
                                break;
                            }

                            if !hard_error {
                                if let Some(mut payload) = selected_payload {
                                    if payload.is_empty() {
                                        out.push(String::from("Fetch error: empty response body."));
                                    } else {
                                        if payload.len() > FETCH_MAX_FILE_BYTES {
                                            payload.truncate(FETCH_MAX_FILE_BYTES);
                                            out.push(alloc::format!(
                                                "Fetch warning: truncated to {} bytes.",
                                                FETCH_MAX_FILE_BYTES
                                            ));
                                        }

                                        if let Some(final_url) = selected_url {
                                            if final_url != request_urls[0] {
                                                out.push(alloc::format!("Fetch final URL: {}", final_url));
                                            }
                                        }

                                        match fat.write_text_file_in_dir(
                                            current_cluster,
                                            file_name.as_str(),
                                            payload.as_slice(),
                                        ) {
                                            Ok(()) => {
                                                out.push(alloc::format!(
                                                    "Saved {} bytes to {}",
                                                    payload.len(),
                                                    file_name
                                                ));
                                                if file_name.ends_with(".RB") {
                                                    out.push(alloc::format!("Run with: ruby {}", file_name));
                                                } else if repo_mode {
                                                    out.push(String::from(
                                                        "Tip: fetch a .rb file from repo and run `ruby <file>.`",
                                                    ));
                                                }
                                            }
                                            Err(err) => {
                                                out.push(alloc::format!("Fetch error: {}", err));
                                            }
                                        }
                                    }
                                } else {
                                    out.push(String::from("Fetch error: network request failed."));
                                }
                            }
                        }
                    }
                }
            }

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in out.iter() {
                    win.add_output(line.as_str());
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "linux" || verb == "lnx" {
            let mut out = Vec::new();
            let arg = arg_raw.trim();

            if arg.is_empty() {
                out.push(String::from("Linux ABI compat (fase1/fase2)"));
                out.push(String::from("Usage:"));
                out.push(String::from("  linux inspect <programa.elf>"));
                out.push(String::from("  linux run <programa.elf> [args...]"));
                out.push(String::from("  linux runreal <programa.elf> [args...]"));
                out.push(String::from("  linux runrealx <programa.elf> [args...]"));
                out.push(String::from("  linux launch <programa.elf> [args...]"));
                out.push(String::from("  linux launchmeta [--strict] <programa.elf>"));
                out.push(String::from("  linux transfer <on|off|status>"));
                out.push(String::from("  linux runtime <quick|deep|status>"));
                out.push(String::from("  linux proc <start|startm|status|verify|step|stop> [args]"));
                out.push(String::from("  linux runloop <start|startx|startm|startmx|status|verify|step|stop> [args]"));
                out.push(String::from("  linux bridge <open|close|status|test> [WxH]"));
                out.push(String::from("Notes:"));
                out.push(String::from("  fase1: ET_EXEC estatico x86_64"));
                out.push(String::from("  fase2 basica: ET_DYN + PT_INTERP + DT_NEEDED + preflight libs"));
                out.push(String::from("  runreal: alias rapido de runloop start (time-slice, no bloqueante)."));
                out.push(String::from(
                    "  runrealx: alias de runloop startx (real-slice, con retorno al GUI).",
                ));
                out.push(String::from("  proc: alias de runloop (mismo engine real por time-slice)."));
                out.push(String::from("  runloop: contenedor de proceso Linux por slices (scheduler real + retorno seguro)."));
                out.push(String::from("  bridge: salida grafica Linux en ventana GUI (SDL/X11 subset inicial)."));
                out.push(String::from("  abi: incluye stubs utiles (sched_yield/nanosleep/getppid/tgkill) para compat dinamica."));
                out.push(String::from("  porting C++/newlib: usa scripts/newlib_port.sh (scaffold/build/doctor)."));
                out.push(String::from(
                    "  launch: usa compatibilidad Linux; el salto directo sin retorno fue deshabilitado.",
                ));
                out.push(String::from("  .EFI/.EXE (PE/COFF) no son ELF Linux."));
            } else {
                let mut parts = arg.splitn(2, ' ');
                let sub = Self::ascii_lower(parts.next().unwrap_or("").trim());
                let sub_tail = parts.next().unwrap_or("").trim();
                let mut sub_tokens = sub_tail.split_whitespace();
                let arg1 = sub_tokens.next().unwrap_or("");
                let arg2 = sub_tokens.next();
                let arg3 = sub_tokens.next();
                let mut file_name = arg1;
                let mut has_extra = arg2.is_some();

                let inspect_mode = sub == "inspect";
                let runreal_safe_mode = sub == "runreal";
                let runreal_transfer_mode = sub == "runrealx" || sub == "runx";
                let runreal_mode = runreal_safe_mode || runreal_transfer_mode;
                let launch_mode = sub == "launch";
                let launchmeta_mode = sub == "launchmeta" || sub == "meta";
                let run_mode = sub == "run" || runreal_mode || launch_mode;
                let transfer_mode = sub == "transfer" || sub == "xfer";
                let runtime_mode = sub == "runtime" || sub == "rt";
                let proc_mode = sub == "proc" || sub == "container" || sub == "ctr";
                let runloop_mode = sub == "runloop" || sub == "loop" || sub == "rl";
                let bridge_mode = sub == "bridge" || sub == "gfx";
                let file_mode = inspect_mode || run_mode || launchmeta_mode;
                let mut launchmeta_strict = false;
                if launchmeta_mode {
                    let a1 = arg1.trim();
                    let a2 = arg2.unwrap_or("").trim();
                    let a3 = arg3.unwrap_or("").trim();
                    let flag1 = Self::ascii_lower(a1);
                    let flag2 = Self::ascii_lower(a2);

                    if a1.is_empty() {
                        file_name = "";
                        has_extra = false;
                    } else if flag1 == "--strict" || flag1 == "strict" {
                        launchmeta_strict = true;
                        file_name = a2;
                        has_extra = a2.is_empty() || !a3.is_empty();
                    } else {
                        file_name = a1;
                        if a2.is_empty() {
                            has_extra = false;
                        } else if (flag2 == "--strict" || flag2 == "strict") && a3.is_empty() {
                            launchmeta_strict = true;
                            has_extra = false;
                        } else {
                            has_extra = true;
                        }
                    }
                }

                if sub == "help" || sub == "-h" || sub == "--help" {
                    out.push(String::from("Linux ABI compat (fase1/fase2)"));
                    out.push(String::from("  linux inspect <programa.elf>"));
                    out.push(String::from("  linux run <programa.elf> [args...]"));
                    out.push(String::from("  linux runreal <programa.elf> [args...]"));
                    out.push(String::from("  linux runrealx <programa.elf> [args...]"));
                    out.push(String::from("  linux launch <programa.elf> [args...]"));
                    out.push(String::from("  linux launchmeta [--strict] <programa.elf>"));
                    out.push(String::from("  linux transfer <on|off|status>"));
                    out.push(String::from("  linux runtime <quick|deep|status>"));
                    out.push(String::from("  linux proc <start|startm|status|verify|step|stop> [args]"));
                    out.push(String::from("  linux runloop <start|startx|startm|startmx|status|verify|step|stop> [args]"));
                    out.push(String::from("  linux bridge <open|close|status|test> [WxH]"));
                    out.push(String::from("  # porting C++/newlib: scripts/newlib_port.sh"));
                } else if transfer_mode {
                    let mode = Self::ascii_lower(file_name);
                    if has_extra || mode.is_empty() {
                        out.push(String::from("Usage: linux transfer <on|off|status>"));
                    } else if mode == "on" {
                        self.linux_real_transfer_enabled = true;
                        out.push(String::from(
                            "Linux transfer: ON (compat legacy). Runloop real-slice funciona con retorno al GUI.",
                        ));
                    } else if mode == "off" {
                        self.linux_real_transfer_enabled = false;
                        crate::syscall::linux_gfx_bridge_set_direct_present(false);
                        out.push(String::from(
                            "Linux transfer: OFF (legacy). Runloop real-slice sigue disponible.",
                        ));
                    } else if mode == "status" {
                        out.push(alloc::format!(
                            "Linux transfer status: {}",
                            if self.linux_real_transfer_enabled { "ON" } else { "OFF" }
                        ));
                    } else {
                        out.push(String::from("Usage: linux transfer <on|off|status>"));
                    }
                } else if runtime_mode {
                    let mode = Self::ascii_lower(file_name);
                    if has_extra || mode.is_empty() {
                        out.push(String::from("Usage: linux runtime <quick|deep|status>"));
                    } else if mode == "quick" || mode == "off" {
                        self.linux_runtime_lookup_enabled = false;
                        out.push(String::from(
                            "Linux runtime lookup: QUICK (sin escaneo profundo /LINUXRT).",
                        ));
                    } else if mode == "deep" || mode == "on" {
                        self.linux_runtime_lookup_enabled = true;
                        out.push(String::from(
                            "Linux runtime lookup: DEEP (escaneo dirigido /LINUXRT, puede tardar).",
                        ));
                    } else if mode == "status" {
                        out.push(alloc::format!(
                            "Linux runtime lookup status: {}",
                            if self.linux_runtime_lookup_enabled {
                                "DEEP"
                            } else {
                                "QUICK"
                            }
                        ));
                    } else {
                        out.push(String::from("Usage: linux runtime <quick|deep|status>"));
                    }
                } else if proc_mode {
                    let mut action_parts = sub_tail.splitn(2, ' ');
                    let action_raw = action_parts.next().unwrap_or("").trim();
                    let action_rest = action_parts.next().unwrap_or("").trim();
                    let action = Self::ascii_lower(action_raw);
                    if action.is_empty() || action == "help" || action == "-h" || action == "--help" {
                        out.push(String::from(
                            "Linux proc (compat): alias de linux runloop (engine real por time-slice)",
                        ));
                        out.push(String::from("  linux proc start <programa.elf> [args...]"));
                        out.push(String::from("  linux proc startm <programa.elf> [args...]"));
                        out.push(String::from("  linux proc status"));
                        out.push(String::from("  linux proc verify"));
                        out.push(String::from("  linux proc step [n]"));
                        out.push(String::from("  linux proc stop"));
                    } else if action == "start" {
                        if action_rest.is_empty() {
                            out.push(String::from("Usage: linux proc start <programa.elf> [args...]"));
                        } else {
                            out.push(String::from("Linux proc: redirigido a runloop start."));
                            let lines = self.linux_runloop_start(win_id, action_rest, true, true);
                            out.extend(lines.into_iter());
                        }
                    } else if action == "startm" || action == "start-manual" {
                        if action_rest.is_empty() {
                            out.push(String::from("Usage: linux proc startm <programa.elf> [args...]"));
                        } else {
                            out.push(String::from("Linux proc: redirigido a runloop startm."));
                            let lines = self.linux_runloop_start(win_id, action_rest, false, true);
                            out.extend(lines.into_iter());
                        }
                    } else if action == "status" {
                        if !action_rest.is_empty() {
                            out.push(String::from("Usage: linux proc status"));
                        } else {
                            out.extend(self.linux_runloop_status_lines().into_iter());
                        }
                    } else if action == "verify" || action == "e2e" {
                        if !action_rest.is_empty() {
                            out.push(String::from("Usage: linux proc verify"));
                        } else {
                            out.extend(self.linux_runloop_e2e_lines().into_iter());
                            out.extend(self.linux_runloop_status_lines().into_iter());
                        }
                    } else if action == "step" {
                        let mut step_parts = action_rest.split_whitespace();
                        let first = step_parts.next().unwrap_or("");
                        let extra = step_parts.next();
                        if extra.is_some() {
                            out.push(String::from("Usage: linux proc step [n]"));
                        } else {
                            let mut count = 1usize;
                            if !first.is_empty() {
                                match Self::parse_loose_positive_usize(first, LINUX_RUNLOOP_MAX_STEPS) {
                                    Some(v) => count = v,
                                    None => out.push(String::from("Linux proc step: n invalido.")),
                                }
                            }
                            if out.is_empty() {
                                let step_lines = self.linux_runloop_advance(count);
                                out.extend(step_lines.into_iter());
                                out.extend(self.linux_runloop_status_lines().into_iter());
                            }
                        }
                    } else if action == "stop" {
                        if !action_rest.is_empty() {
                            out.push(String::from("Usage: linux proc stop"));
                        } else {
                            let lines = self.linux_runloop_stop();
                            out.extend(lines.into_iter());
                            out.extend(self.linux_runloop_status_lines().into_iter());
                        }
                    } else {
                        out.push(String::from(
                            "Usage: linux proc <start|startm|status|verify|step|stop> [args]",
                        ));
                    }
                } else if runloop_mode {
                    let mut action_parts = sub_tail.splitn(2, ' ');
                    let action_raw = action_parts.next().unwrap_or("").trim();
                    let action_rest = action_parts.next().unwrap_or("").trim();
                    let action = Self::ascii_lower(action_raw);
                    let action_arg = arg2.unwrap_or("");
                    if action.is_empty() || action == "help" || action == "-h" || action == "--help" {
                        out.push(String::from(
                            "Linux runloop (scheduler por time-slice con retorno seguro)",
                        ));
                        out.push(String::from("  linux runloop start <programa.elf> [args...]    (auto, real-slice)"));
                        out.push(String::from("  linux runloop startx <programa.elf> [args...]   (auto, real-slice)"));
                        out.push(String::from("  linux runloop startm <programa.elf> [args...]   (manual, real-slice)"));
                        out.push(String::from("  linux runloop startmx <programa.elf> [args...]  (manual, real-slice)"));
                        out.push(String::from("  linux runloop status"));
                        out.push(String::from("  linux runloop verify"));
                        out.push(String::from("  linux runloop step [n]"));
                        out.push(String::from("  linux runloop stop"));
                    } else if action == "start" {
                        if action_rest.is_empty() {
                            out.push(String::from("Usage: linux runloop start <programa.elf> [args...]"));
                        } else {
                            let lines = self.linux_runloop_start(win_id, action_rest, true, true);
                            out.extend(lines.into_iter());
                        }
                    } else if action == "startx" || action == "start-transfer" {
                        if action_rest.is_empty() {
                            out.push(String::from("Usage: linux runloop startx <programa.elf> [args...]"));
                        } else {
                            let lines = self.linux_runloop_start(win_id, action_rest, true, true);
                            out.extend(lines.into_iter());
                        }
                    } else if action == "startm" || action == "start-manual" {
                        if action_rest.is_empty() {
                            out.push(String::from("Usage: linux runloop startm <programa.elf> [args...]"));
                        } else {
                            let lines = self.linux_runloop_start(win_id, action_rest, false, true);
                            out.extend(lines.into_iter());
                        }
                    } else if action == "startmx" || action == "startm-transfer" {
                        if action_rest.is_empty() {
                            out.push(String::from("Usage: linux runloop startmx <programa.elf> [args...]"));
                        } else {
                            let lines = self.linux_runloop_start(win_id, action_rest, false, true);
                            out.extend(lines.into_iter());
                        }
                    } else if action == "status" {
                        if !action_rest.is_empty() {
                            out.push(String::from("Usage: linux runloop status"));
                        } else {
                            out.extend(self.linux_runloop_status_lines().into_iter());
                        }
                    } else if action == "verify" || action == "e2e" {
                        if !action_rest.is_empty() {
                            out.push(String::from("Usage: linux runloop verify"));
                        } else {
                            out.extend(self.linux_runloop_e2e_lines().into_iter());
                            out.extend(self.linux_runloop_status_lines().into_iter());
                        }
                    } else if action == "step" {
                        if arg3.is_some() {
                            out.push(String::from("Usage: linux runloop step [n]"));
                        } else {
                            let mut count = 1usize;
                            if !action_arg.is_empty() {
                                match Self::parse_loose_positive_usize(action_arg, LINUX_RUNLOOP_MAX_STEPS) {
                                    Some(v) => count = v,
                                    None => out.push(String::from("Linux runloop step: n invalido.")),
                                }
                            }
                            if out.is_empty() {
                                let step_lines = self.linux_runloop_advance(count);
                                out.extend(step_lines.into_iter());
                                out.extend(self.linux_runloop_status_lines().into_iter());
                            }
                        }
                    } else if action == "stop" {
                        if !action_rest.is_empty() {
                            out.push(String::from("Usage: linux runloop stop"));
                        } else {
                            let lines = self.linux_runloop_stop();
                            out.extend(lines.into_iter());
                            out.extend(self.linux_runloop_status_lines().into_iter());
                        }
                    } else {
                        out.push(String::from(
                            "Usage: linux runloop <start|startx|startm|startmx|status|verify|step|stop> [args]",
                        ));
                    }
                } else if bridge_mode {
                    let action = Self::ascii_lower(arg1);
                    let action_arg = arg2.unwrap_or("");
                    let parse_dims = |text: &str| -> Option<(u32, u32)> {
                        let trimmed = text.trim();
                        let mut parts = trimmed.split('x');
                        let w = parts.next()?.trim().parse::<u32>().ok()?;
                        let h = parts.next()?.trim().parse::<u32>().ok()?;
                        if parts.next().is_some() {
                            return None;
                        }
                        if w < 64 || h < 64 {
                            return None;
                        }
                        Some((w, h))
                    };

                    if action.is_empty() || action == "help" || action == "-h" || action == "--help" {
                        out.push(String::from("Linux bridge (SDL/X11 subset inicial)"));
                        out.push(String::from("  linux bridge open [WxH]"));
                        out.push(String::from("  linux bridge test [n]"));
                        out.push(String::from("  linux bridge status"));
                        out.push(String::from("  linux bridge close"));
                    } else if action == "open" {
                        if arg3.is_some() {
                            out.push(String::from("Usage: linux bridge open [WxH]"));
                        } else {
                            let (w, h) = if action_arg.is_empty() {
                                (LINUX_BRIDGE_DEFAULT_WIDTH, LINUX_BRIDGE_DEFAULT_HEIGHT)
                            } else if let Some((pw, ph)) = parse_dims(action_arg) {
                                (pw, ph)
                            } else {
                                out.push(String::from(
                                    "Linux bridge open: formato invalido, usa WxH (ej. 800x450).",
                                ));
                                (0, 0)
                            };
                            if out.is_empty() {
                                crate::syscall::linux_gfx_bridge_open(w, h);
                                crate::syscall::linux_gfx_bridge_fill_test(crate::timer::ticks());
                                self.ensure_linux_bridge_window();
                                self.service_linux_bridge_window();
                                out.push(alloc::format!("Linux bridge: abierto {}x{}.", w, h));
                            }
                        }
                    } else if action == "close" {
                        if !action_arg.is_empty() || arg3.is_some() {
                            out.push(String::from("Usage: linux bridge close"));
                        } else {
                            crate::syscall::linux_gfx_bridge_close();
                            out.push(String::from("Linux bridge: cerrado."));
                        }
                    } else if action == "status" {
                        if !action_arg.is_empty() || arg3.is_some() {
                            out.push(String::from("Usage: linux bridge status"));
                        } else {
                            let status = crate::syscall::linux_gfx_bridge_status();
                            let x11 = crate::syscall::linux_x11_socket_status();
                            out.push(alloc::format!(
                                "Linux bridge: active={} size={}x{} frame_seq={} dirty={}",
                                if status.active { "yes" } else { "no" },
                                status.width,
                                status.height,
                                status.frame_seq,
                                if status.dirty { "yes" } else { "no" }
                            ));
                            out.push(alloc::format!(
                                "Linux bridge: input queue={} dropped={} event_seq={} last_input_tick={}",
                                status.event_count,
                                status.event_dropped,
                                status.event_seq,
                                status.last_input_tick
                            ));
                            out.push(alloc::format!(
                                "Linux bridge: status={}",
                                Self::linux_decode_status_ascii(status.status.as_slice(), status.status_len)
                            ));
                            out.push(alloc::format!(
                                "Linux bridge: x11 endpoint={} connected={} ready={} handshake={} last_errno={}",
                                x11.endpoint_count,
                                x11.connected_count,
                                x11.ready_count,
                                x11.handshake_count,
                                x11.last_error
                            ));
                            if x11.last_path_len > 0 {
                                out.push(alloc::format!(
                                    "Linux bridge: x11 path={}",
                                    Self::linux_decode_status_ascii(x11.last_path.as_slice(), x11.last_path_len)
                                ));
                            }
                            out.push(alloc::format!(
                                "Linux bridge: unix connect errno={} path={}",
                                x11.last_unix_connect_errno,
                                Self::linux_decode_status_ascii(
                                    x11.last_unix_connect_path.as_slice(),
                                    x11.last_unix_connect_len
                                )
                            ));
                            if x11.endpoint_count == 0
                                && x11.connected_count == 0
                                && x11.ready_count == 0
                                && x11.handshake_count == 0
                                && x11.last_unix_connect_len == 0
                                && x11.last_unix_connect_errno == 0
                            {
                                out.push(String::from(
                                    "Linux bridge: sin intento X11 (cliente no abrio /tmp/.X11-unix/Xn).",
                                ));
                            }
                        }
                    } else if action == "test" {
                        if arg3.is_some() {
                            out.push(String::from("Usage: linux bridge test [n]"));
                        } else {
                            let mut frames = 1usize;
                            if !action_arg.is_empty() {
                                match action_arg.parse::<usize>() {
                                    Ok(v) => frames = v.max(1).min(16),
                                    Err(_) => out.push(String::from("Linux bridge test: n invalido.")),
                                }
                            }
                            if out.is_empty() {
                                if !crate::syscall::linux_gfx_bridge_status().active {
                                    crate::syscall::linux_gfx_bridge_open(
                                        LINUX_BRIDGE_DEFAULT_WIDTH,
                                        LINUX_BRIDGE_DEFAULT_HEIGHT,
                                    );
                                }
                                for idx in 0..frames {
                                    crate::syscall::linux_gfx_bridge_fill_test(
                                        crate::timer::ticks().saturating_add(idx as u64),
                                    );
                                }
                                self.ensure_linux_bridge_window();
                                self.service_linux_bridge_window();
                                out.push(alloc::format!(
                                    "Linux bridge: test frame(s) generado(s) = {}.",
                                    frames
                                ));
                            }
                        }
                    } else {
                        out.push(String::from(
                            "Usage: linux bridge <open|close|status|test> [WxH|n]",
                        ));
                    }
                } else if sub == "run" || runreal_mode || launch_mode {
                    let run_target = sub_tail.trim();
                    if run_target.is_empty() {
                        out.push(String::from("Usage: linux run <programa.elf> [args...]"));
                        out.push(String::from("Usage: linux runreal <programa.elf> [args...]"));
                        out.push(String::from("Usage: linux runrealx <programa.elf> [args...]"));
                        out.push(String::from("Usage: linux launch <programa.elf> [args...]"));
                    } else {
                        if sub == "run" {
                            out.push(String::from(
                                "Linux run: alias de runloop start (time-slice, GUI no se bloquea).",
                            ));
                        } else if runreal_transfer_mode {
                            out.push(String::from(
                                "Linux runrealx: runloop usara ejecucion real por time-slice con retorno al GUI.",
                            ));
                        } else if launch_mode {
                            out.push(String::from(
                                "Linux launch: runloop usara ejecucion real por time-slice con retorno al GUI.",
                            ));
                        } else {
                            out.push(String::from(
                                "Linux runreal: alias de runloop start (real-slice, GUI no se bloquea).",
                            ));
                        }

                        let request_real_transfer = true;
                        let lines =
                            self.linux_runloop_start(win_id, run_target, true, request_real_transfer);
                        out.extend(lines.into_iter());
                        out.push(String::from("Tip: usa 'linux runloop status' para ver progreso."));
                    }
                } else if !file_mode || file_name.is_empty() || has_extra {
                    out.push(String::from("Usage: linux inspect <programa.elf>"));
                    out.push(String::from("Usage: linux run <programa.elf> [args...]"));
                    out.push(String::from("Usage: linux runreal <programa.elf> [args...]"));
                    out.push(String::from("Usage: linux runrealx <programa.elf> [args...]"));
                    out.push(String::from("Usage: linux launch <programa.elf> [args...]"));
                    out.push(String::from("Usage: linux launchmeta [--strict] <programa.elf>"));
                    out.push(String::from("Usage: linux transfer <on|off|status>"));
                    out.push(String::from("Usage: linux runtime <quick|deep|status>"));
                    out.push(String::from("Usage: linux proc <start|startm|status|step|stop> [args]"));
                    out.push(String::from("Usage: linux runloop <start|startx|startm|startmx|status|step|stop> [args]"));
                    out.push(String::from("Usage: linux runloop verify"));
                    out.push(String::from("Usage: linux bridge <open|close|status|test> [WxH|n]"));
                } else if fat.bytes_per_sector == 0 {
                    if self.manual_unmount_lock {
                        out.push(String::from(
                            "Linux error: volumen desmontado. Usa 'mount <n>' primero.",
                        ));
                    } else if !fat.init() {
                        out.push(String::from(
                            "Linux error: FAT32 no disponible. Usa 'disks' y 'mount <n>'.",
                        ));
                    }
                } else {
                    let current_cwd_cluster = match self.windows.iter().find(|w| w.id == win_id) {
                        Some(win) => {
                            if win.current_dir_cluster == 0 {
                                fat.root_cluster
                            } else {
                                win.current_dir_cluster
                            }
                        }
                        None => fat.root_cluster,
                    };

                    let (target_dir, target_leaf) = match Self::resolve_terminal_parent_and_leaf(
                        fat,
                        current_cwd_cluster,
                        file_name,
                    ) {
                        Ok(v) => v,
                        Err(err) => {
                            out.push(alloc::format!("Linux error resolving path: {}", err));
                            (0, String::new())
                        }
                    };

                    let mut elf_entry = None;
                    let mut current_entries: Vec<crate::fs::DirEntry> = Vec::new();

                    if out.is_empty() {
                        match fat.read_dir_entries(target_dir) {
                            Ok(entries) => {
                                for (scan_idx, entry) in entries.iter().enumerate() {
                                    if (scan_idx & 31) == 0 {
                                        self.pump_ui_while_linux_preflight(win_id, scan_idx + 1);
                                    }
                                    if !entry.valid || entry.file_type != FileType::File {
                                        continue;
                                    }
                                    current_entries.push(*entry);
                                    if entry.matches_name(target_leaf.as_str())
                                        || entry.full_name().eq_ignore_ascii_case(target_leaf.as_str())
                                    {
                                        elf_entry = Some(*entry);
                                    }
                                }
                            }
                            Err(_) => {
                                out.push(String::from(
                                    "Linux error: no se pudo leer el directorio destino.",
                                ));
                            }
                        }
                    }

                    if out.is_empty() {
                        let Some(entry) = elf_entry else {
                            out.push(String::from(
                                "Linux error: archivo ELF no encontrado en directorio actual.",
                            ));
                            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                for line in out.iter() {
                                    win.add_output(line.as_str());
                                }
                                win.render_terminal();
                            }
                            return;
                        };

                        if entry.size == 0 {
                            out.push(String::from("Linux error: archivo ELF vacio."));
                        } else if entry.size as usize > crate::linux_compat::ELF_MAX_FILE_BYTES {
                            out.push(alloc::format!(
                                "Linux error: ELF demasiado grande (max {} bytes).",
                                crate::linux_compat::ELF_MAX_FILE_BYTES
                            ));
                        } else if entry.cluster < 2 {
                            out.push(String::from("Linux error: cluster invalido."));
                        } else {
                            let mut raw = Vec::new();
                            raw.resize(entry.size as usize, 0);
                            match fat.read_file_sized(entry.cluster, entry.size as usize, &mut raw) {
                                Ok(len) => {
                                    raw.truncate(len);
                                }
                                Err(err) => {
                                    out.push(alloc::format!("Linux error: {}", err));
                                }
                            }

                            if out.is_empty() {
                                if launchmeta_mode {
                                    let exec_name = entry.full_name();
                                    let mut strict_failed = false;
                                    out.push(alloc::format!(
                                        "Linux launchmeta: {}",
                                        exec_name
                                    ));
                                    if launchmeta_strict {
                                        out.push(String::from("  strict=ON"));
                                    }
                                    match Self::load_linux_launch_manifest_for_exec(
                                        fat,
                                        current_entries.as_slice(),
                                        exec_name.as_str(),
                                    ) {
                                        Some(metadata) => {
                                            let exact =
                                                Self::linux_launch_manifest_matches_exec(&metadata, exec_name.as_str());
                                            out.push(alloc::format!("  manifest={}", metadata.file_name));
                                            out.push(alloc::format!(
                                                "  match={}",
                                                if exact { "exact" } else { "fallback" }
                                            ));
                                            if let Some(target) = metadata.target.as_deref() {
                                                out.push(alloc::format!("  target={}", target));
                                            } else {
                                                out.push(String::from("  target=<none>"));
                                            }
                                            if let Some(local) = metadata.exec_local.as_deref() {
                                                out.push(alloc::format!("  exec_local={}", local));
                                            } else {
                                                out.push(String::from("  exec_local=<none>"));
                                            }
                                            if let Some(interp) = metadata.interp_path.as_deref() {
                                                out.push(alloc::format!("  PT_INTERP={}", interp));
                                            } else {
                                                out.push(String::from("  PT_INTERP=<none>"));
                                            }
                                            out.push(alloc::format!(
                                                "  DT_NEEDED declared={} parsed={}",
                                                metadata.needed_declared.unwrap_or(metadata.needed.len()),
                                                metadata.needed.len()
                                            ));
                                            if metadata.needed.is_empty() {
                                                out.push(String::from("  NEEDED=<none>"));
                                            } else {
                                                for needed in metadata.needed.iter() {
                                                    out.push(alloc::format!("  NEEDED {}", needed));
                                                }
                                            }
                                            if !exact {
                                                if launchmeta_strict {
                                                    strict_failed = true;
                                                    out.push(String::from(
                                                        "  error: manifest no coincide exacto con el ELF (fallback detectado).",
                                                    ));
                                                } else {
                                                    out.push(String::from(
                                                        "  warning: manifest no coincide exacto con el ELF; se uso fallback del directorio.",
                                                    ));
                                                }
                                            }
                                        }
                                        None => {
                                            out.push(String::from("  manifest=<none>"));
                                            out.push(String::from(
                                                "  tip: reinstala el paquete para generar <APPTAG8>.LNX.",
                                            ));
                                            if launchmeta_strict {
                                                strict_failed = true;
                                                out.push(String::from(
                                                    "  error: strict requiere manifest .LNX presente.",
                                                ));
                                            }
                                        }
                                    }
                                    if launchmeta_strict {
                                        out.push(alloc::format!(
                                            "  strict result={}",
                                            if strict_failed { "FAIL" } else { "PASS" }
                                        ));
                                    }
                                } else if Self::is_pe_payload(raw.as_slice()) {
                                    out.push(String::from(
                                        "Linux error: archivo PE/COFF detectado (.EFI/.EXE), no ELF.",
                                    ));
                                    out.push(String::from(
                                        "Para DOOM UEFI usa comando 'doom' (o 'shell' para UEFI Shell).",
                                    ));
                                } else {
                                    match crate::linux_compat::inspect_elf64(raw.as_slice()) {
                                        Ok(report) => {
                                        let span_size =
                                            report.span_end.saturating_sub(report.span_start);
                                        out.push(alloc::format!(
                                            "Linux inspect: {}",
                                            file_name
                                        ));
                                        out.push(alloc::format!(
                                            "  Type={} Machine={} PH={} Entry={:#x}",
                                            crate::linux_compat::elf_type_name(report.e_type),
                                            crate::linux_compat::machine_name(report.machine),
                                            report.ph_count,
                                            report.entry
                                        ));
                                        out.push(alloc::format!(
                                            "  PT_LOAD={} file={} bytes mem={} bytes",
                                            report.load_segments.len(),
                                            report.load_file_bytes,
                                            report.load_mem_bytes
                                        ));
                                        out.push(alloc::format!(
                                            "  Span={:#x}-{:#x} ({} KiB)",
                                            report.span_start,
                                            report.span_end,
                                            span_size / 1024
                                        ));
                                        if report.has_interp {
                                            let interp = report
                                                .interp_path
                                                .as_deref()
                                                .unwrap_or("<desconocido>");
                                            out.push(alloc::format!("  PT_INTERP={}", interp));
                                        } else {
                                            out.push(String::from("  PT_INTERP=<none>"));
                                        }
                                        out.push(alloc::format!(
                                            "  PT_DYNAMIC={} PT_TLS={}",
                                            if report.has_dynamic { "yes" } else { "no" },
                                            if report.has_tls { "yes" } else { "no" }
                                        ));
                                        out.push(alloc::format!(
                                            "  Syscall sites (0F 05)={}",
                                            report.syscall_sites
                                        ));

                                        let phase1 = crate::linux_compat::phase1_static_compatibility(&report);
                                        let newlib_note =
                                            crate::linux_compat::newlib_cpp_port_diagnosis(&report);
                                        out.push(alloc::format!("  {}", newlib_note));
                                        match phase1 {
                                            Ok(()) => {
                                                out.push(String::from(
                                                    "Phase1 check: compatible para carga estatica.",
                                                ));
                                                if run_mode {
                                                    match crate::linux_compat::stage_static_elf64(
                                                        raw.as_slice(),
                                                    ) {
                                                        Ok(stage) => {
                                                            out.push(String::from(
                                                                "Linux run (fase1): imagen staged.",
                                                            ));
                                                            out.push(alloc::format!(
                                                                "  Entry={:#x} offset={:#x}",
                                                                stage.entry_virt,
                                                                stage.entry_offset
                                                            ));
                                                            out.push(alloc::format!(
                                                                "  Span start={:#x} size={} KiB",
                                                                stage.span_start,
                                                                stage.span_size / 1024
                                                            ));
                                                            out.push(alloc::format!(
                                                                "  PT_LOAD={} Syscall sites={}",
                                                                stage.load_segments,
                                                                stage.syscall_sites
                                                            ));
                                                            out.push(alloc::format!(
                                                                "  Sample hash={:#010x}",
                                                                stage.sample_hash
                                                            ));
                                                            if launch_mode {
                                                                out.push(String::from(
                                                                    "Linux launch: ET_EXEC fase1 no tiene ruta de control transfer aun.",
                                                                ));
                                                            } else if runreal_mode {
                                                                out.push(String::from(
                                                                    "Linux runreal: ET_EXEC validado; falta contenedor de proceso Linux para ejecutar y volver al GUI.",
                                                                ));
                                                            } else {
                                                                out.push(String::from(
                                                                    "Linux run: salto real a userspace Linux aun no habilitado.",
                                                                ));
                                                            }
                                                        }
                                                        Err(err) => {
                                                            out.push(alloc::format!(
                                                                "Linux run error: {}",
                                                                err
                                                            ));
                                                        }
                                                    }
                                                }
                                            }
                                            Err(err) => {
                                                out.push(alloc::format!(
                                                    "Phase1 check: no compatible ({})",
                                                    err
                                                ));
                                            }
                                        }

                                        if run_mode && phase1.is_err() {
                                            match crate::linux_compat::inspect_dynamic_elf64(raw.as_slice()) {
                                                Ok(dynamic) => {
                                                    out.push(alloc::format!(
                                                        "Phase2 dynamic inspect: NEEDED={} STRTAB={:#x} size={} bytes",
                                                        dynamic.needed.len(),
                                                        dynamic.strtab_virt,
                                                        dynamic.strtab_size
                                                    ));
                                                    if let Some(soname) = dynamic.soname.as_deref() {
                                                        out.push(alloc::format!("  SONAME={}", soname));
                                                    }
                                                    if let Some(rpath) = dynamic.rpath.as_deref() {
                                                        out.push(alloc::format!("  RPATH={}", rpath));
                                                    }
                                                    if let Some(runpath) = dynamic.runpath.as_deref() {
                                                        out.push(alloc::format!("  RUNPATH={}", runpath));
                                                    }

                                                    match crate::linux_compat::phase2_dynamic_compatibility(
                                                        &report,
                                                        &dynamic,
                                                    ) {
                                                        Ok(()) => {
                                                            if let Some(win) =
                                                                self.windows.iter_mut().find(|w| w.id == win_id)
                                                            {
                                                                win.add_output(
                                                                    "Linux run: iniciando preflight dinamico (runtime + dependencias)...",
                                                                );
                                                                win.render_terminal();
                                                            }
                                                            self.paint();
                                                            let runtime_targeted_mode = runreal_transfer_mode
                                                                && self.linux_runtime_lookup_enabled;
                                                            if runreal_transfer_mode
                                                                && !self.linux_runtime_lookup_enabled
                                                            {
                                                                out.push(String::from(
                                                                    "Linux runrealx: runtime lookup QUICK activo (sin escaneo profundo /LINUXRT).",
                                                                ));
                                                            }
                                                            let manifest_map = Self::load_manifest_for_installed_exec(
                                                                fat,
                                                                current_entries.as_slice(),
                                                                file_name,
                                                            );
                                                            let mut runtime_wants: Vec<String> = Vec::new();
                                                            if let Some(interp) = dynamic.interp_path.as_deref() {
                                                                runtime_wants.push(String::from(interp));
                                                            }
                                                            for needed in dynamic.needed.iter() {
                                                                if runtime_wants.iter().any(|existing| {
                                                                    existing.eq_ignore_ascii_case(needed.as_str())
                                                                }) {
                                                                    continue;
                                                                }
                                                                runtime_wants.push(needed.clone());
                                                            }
                                                            let (runtime_entries, runtime_manifest_map) =
                                                                if runtime_targeted_mode {
                                                                    if let Some(win) =
                                                                        self.windows.iter_mut().find(|w| w.id == win_id)
                                                                    {
                                                                        win.add_output(
                                                                            "Linux runrealx: preflight dirigido /LINUXRT en progreso...",
                                                                        );
                                                                        win.render_terminal();
                                                                    }
                                                                    self.paint();
                                                                    out.push(String::from(
                                                                        "Linux runrealx: fast-safe activo, aplicando busqueda dirigida de runtime /LINUXRT.",
                                                                    ));
                                                                    let (entries, maps, timed_out) =
                                                                        self.collect_targeted_linux_runtime_support(
                                                                            win_id,
                                                                            fat,
                                                                            runtime_wants
                                                                                .as_slice(),
                                                                        );
                                                                    out.push(alloc::format!(
                                                                        "Linux runrealx: runtime dirigido detectado ({} archivos, {} mapas).",
                                                                        entries.len(),
                                                                        maps.len()
                                                                    ));
                                                                    if timed_out {
                                                                        out.push(String::from(
                                                                            "Linux runrealx warning: preflight dirigido recortado por timeout de seguridad.",
                                                                        ));
                                                                    }
                                                                    (entries, maps)
                                                                } else {
                                                                    self.collect_global_linux_runtime_support(win_id, fat)
                                                                };
                                                            self.pump_ui_while_blocked_net();
                                                            let mut dependency_entries = current_entries.clone();
                                                            if !runtime_entries.is_empty() {
                                                                if runtime_targeted_mode {
                                                                    out.push(alloc::format!(
                                                                        "Phase2: runtime /LINUXRT dirigido detectado ({} archivos).",
                                                                        runtime_entries.len()
                                                                    ));
                                                                } else {
                                                                    out.push(alloc::format!(
                                                                        "Phase2: runtime global /LINUXRT detectado ({} archivos).",
                                                                        runtime_entries.len()
                                                                    ));
                                                                }
                                                                for (rt_idx, entry) in runtime_entries.iter().enumerate() {
                                                                    dependency_entries.push(*entry);
                                                                    if (rt_idx & 127) == 0 {
                                                                        self.pump_ui_while_linux_preflight(
                                                                            win_id,
                                                                            rt_idx + 1,
                                                                        );
                                                                    }
                                                                }
                                                            }
                                                            let mut combined_manifest_map: Vec<(String, String, String)> =
                                                                Vec::new();
                                                            if let Some(map) = manifest_map.as_deref() {
                                                                for (map_idx, item) in map.iter().enumerate() {
                                                                    combined_manifest_map.push((
                                                                        item.0.clone(),
                                                                        item.1.clone(),
                                                                        item.2.clone(),
                                                                    ));
                                                                    if (map_idx & 127) == 0 {
                                                                        self.pump_ui_while_linux_preflight(
                                                                            win_id,
                                                                            map_idx + 1,
                                                                        );
                                                                    }
                                                                }
                                                            }
                                                            if !runtime_manifest_map.is_empty() {
                                                                out.push(alloc::format!(
                                                                    "Phase2: runtime /LINUXRT mapeo .LST cargado ({} entradas).",
                                                                    runtime_manifest_map.len()
                                                                ));
                                                                for (rt_map_idx, item) in
                                                                    runtime_manifest_map.iter().enumerate()
                                                                {
                                                                    combined_manifest_map.push((
                                                                        item.0.clone(),
                                                                        item.1.clone(),
                                                                        item.2.clone(),
                                                                    ));
                                                                    if (rt_map_idx & 127) == 0 {
                                                                        self.pump_ui_while_linux_preflight(
                                                                            win_id,
                                                                            rt_map_idx + 1,
                                                                        );
                                                                    }
                                                                }
                                                            }
                                                            let combined_manifest_ref = if combined_manifest_map.is_empty() {
                                                                None
                                                            } else {
                                                                Some(combined_manifest_map.as_slice())
                                                            };
                                                            if manifest_map.is_some() {
                                                                out.push(String::from(
                                                                    "Phase2: manifiesto .LST detectado para resolver rutas originales.",
                                                                ));
                                                            } else if combined_manifest_ref.is_none() {
                                                                out.push(String::from(
                                                                    "Phase2 warning: sin .LST; resolviendo solo por nombres locales.",
                                                                ));
                                                            }

                                                            let mut shim_runtime_index: Vec<(String, u64)> = Vec::new();
                                                            if runreal_mode {
                                                                shim_runtime_index
                                                                    .push((String::from(file_name), entry.size as u64));
                                                                if !target_leaf.is_empty()
                                                                    && !target_leaf.eq_ignore_ascii_case(file_name)
                                                                {
                                                                    shim_runtime_index.push((
                                                                        target_leaf.clone(),
                                                                        entry.size as u64,
                                                                    ));
                                                                }
                                                            }

                                                            let interp_local = dynamic
                                                                .interp_path
                                                                .as_ref()
                                                                .and_then(|interp| {
                                                                    Self::resolve_linux_dependency_name(
                                                                        dependency_entries.as_slice(),
                                                                        combined_manifest_ref,
                                                                        interp.as_str(),
                                                                    )
                                                                });

                                                            let mut issues = 0usize;
                                                            let mut dep_launch_payloads: Vec<(String, Vec<u8>)> = Vec::new();
                                                            if let Some(interp_src) =
                                                                dynamic.interp_path.as_deref()
                                                            {
                                                                match interp_local.as_deref() {
                                                                    Some(local) => {
                                                                        out.push(alloc::format!(
                                                                            "  INTERP {} -> {}",
                                                                            interp_src,
                                                                            local
                                                                        ));
                                                                    }
                                                                    None => {
                                                                        out.push(alloc::format!(
                                                                            "  INTERP missing: {}",
                                                                            interp_src
                                                                        ));
                                                                        issues += 1;
                                                                    }
                                                                }
                                                            }

                                                            for (needed_idx, needed) in dynamic.needed.iter().enumerate() {
                                                                if (needed_idx & 7) == 0 {
                                                                    self.pump_ui_while_linux_preflight(
                                                                        win_id,
                                                                        needed_idx + 1,
                                                                    );
                                                                }
                                                                let resolved = Self::resolve_linux_dependency_name(
                                                                    dependency_entries.as_slice(),
                                                                    combined_manifest_ref,
                                                                    needed.as_str(),
                                                                );
                                                                match resolved {
                                                                    Some(local) => {
                                                                        out.push(alloc::format!(
                                                                            "  NEEDED ok: {} -> {}",
                                                                            needed, local
                                                                        ));
                                                                        if runreal_mode {
                                                                            if let Some(dep_entry) = Self::find_dir_file_entry_by_name(
                                                                                dependency_entries.as_slice(),
                                                                                local.as_str(),
                                                                            ) {
                                                                                shim_runtime_index.push((
                                                                                    needed.clone(),
                                                                                    dep_entry.size as u64,
                                                                                ));
                                                                                shim_runtime_index.push((
                                                                                    local.clone(),
                                                                                    dep_entry.size as u64,
                                                                                ));
                                                                            }
                                                                        }
                                                                        if launch_mode || runreal_mode {
                                                                            let already_loaded = dep_launch_payloads
                                                                                .iter()
                                                                                .any(|(name, _)| {
                                                                                    name.eq_ignore_ascii_case(
                                                                                        needed.as_str(),
                                                                                    )
                                                                                });
                                                                            if !already_loaded {
                                                                                if let Some(dep_entry) =
                                                                                    Self::find_dir_file_entry_by_name(
                                                                                        dependency_entries.as_slice(),
                                                                                        local.as_str(),
                                                                                    )
                                                                                {
                                                                                    if dep_entry.size == 0 {
                                                                                        out.push(alloc::format!(
                                                                                            "  NEEDED read error: {} archivo vacio",
                                                                                            needed
                                                                                        ));
                                                                                        issues += 1;
                                                                                    } else if dep_entry.size as usize
                                                                                        > crate::linux_compat::ELF_MAX_FILE_BYTES
                                                                                    {
                                                                                        out.push(alloc::format!(
                                                                                            "  NEEDED read error: {} demasiado grande",
                                                                                            needed
                                                                                        ));
                                                                                        issues += 1;
                                                                                    } else {
                                                                                        let mut dep_raw = Vec::new();
                                                                                        dep_raw.resize(dep_entry.size as usize, 0);
                                                                                        match fat.read_file_sized(
                                                                                            dep_entry.cluster,
                                                                                            dep_entry.size as usize,
                                                                                            &mut dep_raw,
                                                                                        ) {
                                                                                            Ok(len) => {
                                                                                                dep_raw.truncate(len);
                                                                                                dep_launch_payloads.push((
                                                                                                    needed.clone(),
                                                                                                    dep_raw,
                                                                                                ));
                                                                                            }
                                                                                            Err(err) => {
                                                                                                out.push(alloc::format!(
                                                                                                    "  NEEDED read error: {} ({})",
                                                                                                    needed, err
                                                                                                ));
                                                                                                issues += 1;
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                } else {
                                                                                    out.push(alloc::format!(
                                                                                        "  NEEDED read error: {} sin entry local",
                                                                                        needed
                                                                                    ));
                                                                                    issues += 1;
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                    None => {
                                                                        out.push(alloc::format!(
                                                                            "  NEEDED missing: {}",
                                                                            needed
                                                                        ));
                                                                        issues += 1;
                                                                    }
                                                                }
                                                            }

                                                            let mut main_stage_ok = false;
                                                            self.pump_ui_while_linux_preflight(win_id, dynamic.needed.len() + 1);
                                                            match crate::linux_compat::stage_dyn_elf64(
                                                                raw.as_slice(),
                                                                crate::linux_compat::PHASE2_MAIN_LOAD_BIAS,
                                                            ) {
                                                                Ok(stage) => {
                                                                    main_stage_ok = true;
                                                                    out.push(String::from(
                                                                        "Linux run (fase2): main ET_DYN staged.",
                                                                    ));
                                                                    out.push(alloc::format!(
                                                                        "  Main base={:#x} size={} KiB entry={:#x}",
                                                                        stage.image_start,
                                                                        stage.image_size / 1024,
                                                                        stage.entry_virt
                                                                    ));
                                                                    out.push(alloc::format!(
                                                                        "  Main PT_LOAD={} hash={:#010x}",
                                                                        stage.load_segments,
                                                                        stage.sample_hash
                                                                    ));
                                                                }
                                                                Err(err) => {
                                                                    out.push(alloc::format!(
                                                                        "Linux fase2 main stage error: {}",
                                                                        err
                                                                    ));
                                                                    issues += 1;
                                                                }
                                                            }

                                                            let mut interp_stage_ok = false;
                                                            let mut interp_raw_for_launch: Option<Vec<u8>> = None;
                                                            if let Some(interp_name) = interp_local.as_deref() {
                                                                if let Some(interp_entry) = Self::find_dir_file_entry_by_name(
                                                                    dependency_entries.as_slice(),
                                                                    interp_name,
                                                                ) {
                                                                    if runreal_mode {
                                                                        if let Some(interp_src) = dynamic.interp_path.as_deref() {
                                                                            shim_runtime_index.push((
                                                                                String::from(interp_src),
                                                                                interp_entry.size as u64,
                                                                            ));
                                                                        }
                                                                        shim_runtime_index.push((
                                                                            String::from(interp_name),
                                                                            interp_entry.size as u64,
                                                                        ));
                                                                    }
                                                                    if interp_entry.size == 0 {
                                                                        out.push(String::from(
                                                                            "Linux fase2 interp error: archivo vacio.",
                                                                        ));
                                                                        issues += 1;
                                                                    } else if interp_entry.size as usize
                                                                        > crate::linux_compat::ELF_MAX_FILE_BYTES
                                                                    {
                                                                        out.push(String::from(
                                                                            "Linux fase2 interp error: loader demasiado grande.",
                                                                        ));
                                                                        issues += 1;
                                                                    } else {
                                                                        let mut interp_raw = Vec::new();
                                                                        interp_raw.resize(
                                                                            interp_entry.size as usize,
                                                                            0,
                                                                        );
                                                                        match fat.read_file_sized(
                                                                            interp_entry.cluster,
                                                                            interp_entry.size as usize,
                                                                            &mut interp_raw,
                                                                        ) {
                                                                            Ok(len) => {
                                                                                interp_raw.truncate(len);
                                                                                match crate::linux_compat::stage_dyn_elf64(
                                                                                    interp_raw.as_slice(),
                                                                                    crate::linux_compat::PHASE2_INTERP_LOAD_BIAS,
                                                                                ) {
                                                                                    Ok(stage) => {
                                                                                        interp_stage_ok = true;
                                                                                        out.push(String::from(
                                                                                            "Linux run (fase2): interp staged.",
                                                                                        ));
                                                                                        out.push(alloc::format!(
                                                                                            "  Interp base={:#x} size={} KiB entry={:#x}",
                                                                                            stage.image_start,
                                                                                            stage.image_size / 1024,
                                                                                            stage.entry_virt
                                                                                        ));
                                                                                    }
                                                                                    Err(err) => {
                                                                                        out.push(alloc::format!(
                                                                                            "Linux fase2 interp stage error: {}",
                                                                                            err
                                                                                        ));
                                                                                        issues += 1;
                                                                                    }
                                                                                }
                                                                                if launch_mode || runreal_mode {
                                                                                    interp_raw_for_launch = Some(interp_raw.clone());
                                                                                }
                                                                            }
                                                                            Err(err) => {
                                                                                out.push(alloc::format!(
                                                                                    "Linux fase2 interp read error: {}",
                                                                                    err
                                                                                ));
                                                                                issues += 1;
                                                                            }
                                                                        }
                                                                    }
                                                                } else {
                                                                    out.push(String::from(
                                                                        "Linux fase2 interp error: no encontrado localmente.",
                                                                    ));
                                                                    issues += 1;
                                                                }
                                                            }

                                                            if main_stage_ok && interp_stage_ok && issues == 0 {
                                                                out.push(String::from(
                                                                    "Linux run (fase2): preflight dinamico completo (main+interp+deps).",
                                                                ));
                                                                if launch_mode || runreal_mode {
                                                                    match interp_raw_for_launch.as_deref() {
                                                                        Some(interp_runtime_raw) => {
                                                                            let mut dep_launch_inputs: Vec<
                                                                                crate::linux_compat::LinuxDynDependencyInput<'_>,
                                                                            > = Vec::new();
                                                                            for (soname, dep_raw) in dep_launch_payloads.iter() {
                                                                                dep_launch_inputs.push(
                                                                                    crate::linux_compat::LinuxDynDependencyInput {
                                                                                        soname: soname.as_str(),
                                                                                        raw: dep_raw.as_slice(),
                                                                                    },
                                                                                );
                                                                            }
                                                                            match crate::linux_compat::prepare_phase2_interp_launch_with_deps(
                                                                                raw.as_slice(),
                                                                                interp_runtime_raw,
                                                                                dep_launch_inputs.as_slice(),
                                                                                file_name,
                                                                                file_name,
                                                                            ) {
                                                                                Ok(plan) => {
                                                                                    let mode_label = if runreal_mode {
                                                                                        "Linux runreal"
                                                                                    } else {
                                                                                        "Linux launch"
                                                                                    };
                                                                                    out.push(alloc::format!(
                                                                                        "{}: main base={:#x} entry={:#x}",
                                                                                        mode_label, plan.main_base, plan.main_entry
                                                                                    ));
                                                                                    out.push(alloc::format!(
                                                                                        "{}: interp base={:#x} entry={:#x}",
                                                                                        mode_label, plan.interp_base, plan.interp_entry
                                                                                    ));
                                                                                    out.push(alloc::format!(
                                                                                        "{}: stack={:#x} bytes={} argv={} env={} auxv={}",
                                                                                        mode_label,
                                                                                        plan.stack_ptr,
                                                                                        plan.stack_bytes,
                                                                                        plan.argv_count,
                                                                                        plan.env_count,
                                                                                        plan.aux_pairs
                                                                                    ));
                                                                                    out.push(alloc::format!(
                                                                                        "{}: hashes main={:#010x} interp={:#010x}",
                                                                                        mode_label, plan.main_hash, plan.interp_hash
                                                                                    ));
                                                                                    out.push(alloc::format!(
                                                                                        "{}: reloc main total={} applied={} unsupported={} errors={}",
                                                                                        mode_label,
                                                                                        plan.main_reloc_total,
                                                                                        plan.main_reloc_applied,
                                                                                        plan.main_reloc_unsupported,
                                                                                        plan.main_reloc_errors
                                                                                    ));
                                                                                    out.push(alloc::format!(
                                                                                        "{}: reloc interp total={} applied={} unsupported={} errors={}",
                                                                                        mode_label,
                                                                                        plan.interp_reloc_total,
                                                                                        plan.interp_reloc_applied,
                                                                                        plan.interp_reloc_unsupported,
                                                                                        plan.interp_reloc_errors
                                                                                    ));
                                                                                    out.push(alloc::format!(
                                                                                        "{}: trace enlaces PLT/GOT={}",
                                                                                        mode_label,
                                                                                        plan.symbol_traces.len()
                                                                                    ));
                                                                                    for trace in plan.symbol_traces.iter() {
                                                                                        out.push(alloc::format!(
                                                                                            "  [{}] {} :: {} -> {} slot={:#x} value={:#x}",
                                                                                            trace.reloc_kind,
                                                                                            trace.requestor,
                                                                                            trace.symbol,
                                                                                            trace.provider,
                                                                                            trace.slot_addr,
                                                                                            trace.value_addr
                                                                                        ));
                                                                                    }

                                                                                    if runreal_mode {
                                                                                        let session_id = crate::syscall::linux_shim_begin(
                                                                                            plan.main_entry,
                                                                                            plan.interp_entry,
                                                                                            plan.stack_ptr,
                                                                                            plan.tls_tcb_addr,
                                                                                        );
                                                                                        let mut registered = 0usize;
                                                                                        let mut register_attempts = 0usize;
                                                                                        for (idx, item) in shim_runtime_index.iter().enumerate() {
                                                                                            register_attempts += 1;
                                                                                            if crate::syscall::linux_shim_register_runtime_path(
                                                                                                item.0.as_str(),
                                                                                                item.1,
                                                                                            ) {
                                                                                                registered += 1;
                                                                                            }
                                                                                            if (idx & 31) == 0 {
                                                                                                self.pump_ui_while_linux_preflight(
                                                                                                    win_id,
                                                                                                    idx + 1,
                                                                                                );
                                                                                            }
                                                                                        }
                                                                                        let mut blob_registered = 0usize;
                                                                                        let mut blob_attempts = 0usize;
                                                                                        let mut register_blob = |path_alias: &str, bytes: &[u8]| {
                                                                                            if path_alias.is_empty() || bytes.is_empty() {
                                                                                                return;
                                                                                            }
                                                                                            blob_attempts = blob_attempts.saturating_add(1);
                                                                                            if crate::syscall::linux_shim_register_runtime_blob(path_alias, bytes) {
                                                                                                blob_registered = blob_registered.saturating_add(1);
                                                                                            }
                                                                                        };
                                                                                        register_blob(file_name, raw.as_slice());
                                                                                        if !target_leaf.is_empty()
                                                                                            && !target_leaf.eq_ignore_ascii_case(file_name)
                                                                                        {
                                                                                            register_blob(target_leaf.as_str(), raw.as_slice());
                                                                                        }
                                                                                        if let Some(interp_src) = dynamic.interp_path.as_deref() {
                                                                                            register_blob(interp_src, interp_runtime_raw);
                                                                                        }
                                                                                        if let Some(interp_name) = interp_local.as_deref() {
                                                                                            register_blob(interp_name, interp_runtime_raw);
                                                                                        }
                                                                                        let probe = crate::syscall::linux_shim_probe_baseline();
                                                                                        let shim_status = crate::syscall::linux_shim_status();
                                                                                        let last_sys_name =
                                                                                            crate::syscall::linux_syscall_name(
                                                                                                shim_status.last_sysno,
                                                                                            );
                                                                                        let last_errno_name =
                                                                                            crate::syscall::linux_errno_name(
                                                                                                shim_status.last_errno,
                                                                                            );
                                                                                        let hw_bridge_ready = crate::privilege::syscall_bridge_ready();
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal: session={} shim active={} maps={} fs={:#x} tid={} rt_files={} rt_blobs={} open_fds={}",
                                                                                            session_id,
                                                                                            if shim_status.active {
                                                                                                "yes"
                                                                                            } else {
                                                                                                "no"
                                                                                            },
                                                                                            shim_status.mmap_count,
                                                                                            shim_status.fs_base,
                                                                                            shim_status.tid_value,
                                                                                            shim_status.runtime_file_count,
                                                                                            shim_status.runtime_blob_files,
                                                                                            shim_status.open_file_count
                                                                                        ));
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal: runtime index {} / {} rutas registradas.",
                                                                                            registered,
                                                                                            register_attempts
                                                                                        ));
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal: runtime blobs {} / {} cargados ({} KiB).",
                                                                                            blob_registered,
                                                                                            blob_attempts,
                                                                                            (shim_status.runtime_blob_bytes.saturating_add(1023)) / 1024
                                                                                        ));
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal: diag calls={} last_sys={}({}) last_res={} last_errno={}({}) watchdog={}",
                                                                                            shim_status.syscall_count,
                                                                                            last_sys_name,
                                                                                            shim_status.last_sysno,
                                                                                            shim_status.last_result,
                                                                                            shim_status.last_errno,
                                                                                            last_errno_name,
                                                                                            if shim_status.watchdog_triggered {
                                                                                                "yes"
                                                                                            } else {
                                                                                                "no"
                                                                                            }
                                                                                        ));
                                                                                        if let Some(diag) = Self::linux_shim_path_diag_line(&shim_status) {
                                                                                            out.push(diag);
                                                                                        }
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal: hw syscall gateway {} (phase={})",
                                                                                            if hw_bridge_ready {
                                                                                                "ready"
                                                                                            } else {
                                                                                                "not-ready"
                                                                                            },
                                                                                            crate::privilege::current_phase()
                                                                                        ));
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal: probe baseline attempted={} ok={} unsupported={} failed={}",
                                                                                            probe.attempted,
                                                                                            probe.ok,
                                                                                            probe.unsupported,
                                                                                            probe.failed
                                                                                        ));
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal: brk {} -> {} mmap={} mprotect={} futex={} clock={} random={} uname={}",
                                                                                            probe.brk_before,
                                                                                            probe.brk_after,
                                                                                            probe.mmap_res,
                                                                                            probe.mprotect_res,
                                                                                            probe.futex_res,
                                                                                            probe.clock_res,
                                                                                            probe.random_res,
                                                                                            probe.uname_res
                                                                                        ));
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal: openat={} fstat={} lseek={} read={} close={}",
                                                                                            probe.openat_res,
                                                                                            probe.fstat_res,
                                                                                            probe.lseek_res,
                                                                                            probe.read_res,
                                                                                            probe.close_res
                                                                                        ));
                                                                                        if probe.failed > 0 {
                                                                                            out.push(alloc::format!(
                                                                                                "Linux runreal: probe error first_errno={} (ajustar shim antes de ejecucion real por timeslice).",
                                                                                                probe.first_errno
                                                                                            ));
                                                                                        } else if probe.unsupported > 0 {
                                                                                            out.push(String::from(
                                                                                                "Linux runreal: hay syscalls ENOSYS en shim; completa esa capa antes de ejecucion real por timeslice.",
                                                                                            ));
                                                                                        } else {
                                                                                            out.push(String::from(
                                                                                                "Linux runreal: shim syscall base listo para ejecucion real por timeslice.",
                                                                                            ));
                                                                                        }
                                                                                        out.push(String::from(
                                                                                            "Linux runreal: contenedor de proceso preparado sin bloquear GUI.",
                                                                                        ));
                                                                                        out.push(String::from(
                                                                                            "Siguiente fase: control-transfer del PT_INTERP con retorno seguro al escritorio.",
                                                                                        ));
                                                                                        if runreal_transfer_mode {
                                                                                            if probe.failed == 0
                                                                                                && probe.unsupported == 0
                                                                                                && !shim_status.watchdog_triggered
                                                                                            {
                                                                                                out.push(String::from(
                                                                                                    "Linux runrealx: usa 'linux runloop startx <elf>' para ejecucion real por timeslice con retorno al GUI.",
                                                                                                ));
                                                                                            } else {
                                                                                                out.push(String::from(
                                                                                                    "Linux runrealx: ejecucion real cancelada por probe/watchdog; revisar diag.",
                                                                                                ));
                                                                                            }
                                                                                        }
                                                                                    } else {
                                                                                        out.push(String::from(
                                                                                            "Linux launch: usa 'linux runloop start <elf>' para ejecucion real por timeslice con retorno al GUI.",
                                                                                        ));
                                                                                        out.push(String::from(
                                                                                            "Motivo: falta capa completa Linux (syscalls/procesos/ventanas).",
                                                                                        ));
                                                                                    }
                                                                                }
                                                                                Err(err) => {
                                                                                    if runreal_mode {
                                                                                        out.push(alloc::format!(
                                                                                            "Linux runreal error: {}",
                                                                                            err
                                                                                        ));
                                                                                    } else {
                                                                                        out.push(alloc::format!(
                                                                                            "Linux launch error: {}",
                                                                                            err
                                                                                        ));
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                        None => {
                                                                            if runreal_mode {
                                                                                out.push(String::from(
                                                                                    "Linux runreal error: no se capturo el loader PT_INTERP en memoria.",
                                                                                ));
                                                                            } else {
                                                                                out.push(String::from(
                                                                                    "Linux launch error: no se capturo el loader PT_INTERP en memoria.",
                                                                                ));
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            } else {
                                                                out.push(alloc::format!(
                                                                    "Linux run (fase2): preflight incompleto (issues={}).",
                                                                    issues
                                                                ));
                                                                out.push(String::from(
                                                                    "Tip: faltan librerias Linux de runtime (PT_INTERP/DT_NEEDED). Este paquete .deb no es standalone.",
                                                                ));
                                                            }
                                                            if !launch_mode && !runreal_mode {
                                                                out.push(String::from(
                                                                    "Linux run: usa 'linux runloop start <elf>' para ejecucion real por timeslice.",
                                                                ));
                                                            } else if runreal_safe_mode {
                                                                out.push(String::from(
                                                                    "Linux runreal: usa 'linux runloop start <elf>' para ejecucion real por timeslice con retorno.",
                                                                ));
                                                            } else if runreal_transfer_mode && !self.linux_real_transfer_enabled {
                                                                out.push(String::from(
                                                                    "Linux runrealx: usa 'linux runloop startx <elf>' (real-slice con retorno al GUI).",
                                                                ));
                                                            }
                                                        }
                                                        Err(err) => {
                                                            out.push(alloc::format!(
                                                                "Phase2 check: no compatible ({})",
                                                                err
                                                            ));
                                                        }
                                                    }
                                                }
                                                Err(err) => {
                                                    out.push(alloc::format!(
                                                        "Phase2 inspect: no disponible ({})",
                                                        err
                                                    ));
                                                }
                                            }
                                        }
                                        }
                                        Err(err) => {
                                            out.push(alloc::format!("Linux error: {}", err));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in out.iter() {
                    win.add_output(line.as_str());
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "install" {
            let mut out = Vec::new();
            let arg = arg_raw.trim();
            let mut install_prompt_active = false;

            if arg.is_empty() {
                out.push(String::from(
                    "Usage: install [--autoport] <package.rpx|package.zip|package.tar|package.tar.gz|package.deb|setup.exe> [app_id]",
                ));
                out.push(String::from("Example: install HELLO.RPX HELLO"));
                out.push(String::from("Example: install --autoport APP.DEB TOOL"));
                out.push(String::from("Example: install APPS.ZIP MYAPP"));
                out.push(String::from("Example: install ROOTFS.TAR.GZ LINUX"));
                out.push(String::from("Example: install APP.DEB TOOL"));
                out.push(String::from("Example: install SETUP.EXE WINAPP"));
                out.push(String::from(
                    "Tip: si existe <paquete>.sig (REDUX-SIG-V1), se verifica SHA256 antes de instalar.",
                ));
                out.push(String::from(
                    "Tip: --autoport detecta perfil nativo/compat y genera un manifiesto .PRT.",
                ));
            } else {
                let mut autoport_enabled = false;
                let mut package_name = "";
                let mut app_id_arg: Option<&str> = None;
                let mut extra_args = false;
                for token in arg.split_whitespace() {
                    if token == "--autoport" {
                        autoport_enabled = true;
                        continue;
                    }
                    if package_name.is_empty() {
                        package_name = token;
                    } else if app_id_arg.is_none() {
                        app_id_arg = Some(token);
                    } else {
                        extra_args = true;
                        break;
                    }
                }
                let mut package_is_zip = false;
                let mut package_is_rpx = false;
                let mut package_is_tar = false;
                let mut package_is_targz = false;
                let mut package_is_deb = false;
                let mut package_is_exe = false;
                let mut package_signature_present = false;
                let mut package_signature_verified = false;

                if extra_args {
                    out.push(String::from(
                        "Usage: install [--autoport] <package.rpx|package.zip|package.tar|package.tar.gz|package.deb|setup.exe> [app_id]",
                    ));
                } else if package_name.is_empty() {
                    out.push(String::from(
                        "Usage: install [--autoport] <package.rpx|package.zip|package.tar|package.tar.gz|package.deb|setup.exe> [app_id]",
                    ));
                } else {
                    let package_lower = Self::ascii_lower(package_name);
                    if package_lower.ends_with(".rpx") {
                        package_is_rpx = true;
                    } else if package_lower.ends_with(".zip") {
                        package_is_zip = true;
                    } else if package_lower.ends_with(".tar.gz") || package_lower.ends_with(".tgz") {
                        package_is_targz = true;
                    } else if package_lower.ends_with(".tar") {
                        package_is_tar = true;
                    } else if package_lower.ends_with(".deb") {
                        package_is_deb = true;
                    } else if package_lower.ends_with(".exe") {
                        package_is_exe = true;
                    } else if package_lower.ends_with(".rpm") {
                        out.push(String::from(
                            "Install error: RPM aun no soportado (cpio+xz/zstd pendiente).",
                        ));
                    } else if package_lower.ends_with(".appimage") {
                        out.push(String::from(
                            "Install error: AppImage aun no soportado (ELF/FUSE runtime pendiente).",
                        ));
                    } else if package_lower.ends_with(".msi") {
                        out.push(String::from(
                            "Install error: MSI aun no soportado (Windows Installer engine pendiente).",
                        ));
                    } else {
                        out.push(String::from(
                            "Install error: formato no soportado. Usa .RPX/.ZIP/.TAR/.TAR.GZ/.DEB/.EXE.",
                        ));
                    }
                }

                if out.is_empty()
                    && !package_is_rpx
                    && !package_is_zip
                    && !package_is_tar
                    && !package_is_targz
                    && !package_is_deb
                    && !package_is_exe
                {
                    out.push(String::from(
                        "Install error: no se pudo determinar tipo de paquete.",
                    ));
                }

                if out.is_empty() {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.add_output(
                            alloc::format!("Install: preparando {}...", package_name).as_str(),
                        );
                        if autoport_enabled {
                            win.add_output("Install: autoport habilitado.");
                        }
                        win.render_terminal();
                    }
                    self.paint();
                    if fat.bytes_per_sector == 0 {
                        if self.manual_unmount_lock {
                            out.push(String::from(
                                "Install error: volumen desmontado. Usa 'mount <n>' primero.",
                            ));
                        } else if !fat.init() {
                            out.push(String::from(
                                "Install error: FAT32 no disponible. Usa 'disks' y 'mount <n>'.",
                            ));
                        }
                    } else {
                        let current_cluster = match self.windows.iter().find(|w| w.id == win_id) {
                            Some(win) => {
                                if win.current_dir_cluster == 0 {
                                    fat.root_cluster
                                } else {
                                    win.current_dir_cluster
                                }
                            }
                            None => fat.root_cluster,
                        };

                        let mut package_entry = None;
                        let mut package_signature_entry = None;
                        let signature_name = alloc::format!("{}.sig", package_name);
                        let signature_name_lower = Self::ascii_lower(signature_name.as_str());
                        match fat.read_dir_entries(current_cluster) {
                            Ok(entries) => {
                                for entry in entries.iter() {
                                    if !entry.valid || entry.file_type != FileType::File {
                                        continue;
                                    }
                                    if package_signature_entry.is_none() {
                                        let entry_name = entry.full_name();
                                        if Self::ascii_lower(entry_name.as_str()) == signature_name_lower {
                                            package_signature_entry = Some(*entry);
                                        }
                                    }
                                    if entry.matches_name(package_name) {
                                        package_entry = Some(*entry);
                                    }
                                }
                            }
                            Err(_) => {
                                out.push(String::from("Install error: no se pudo leer el directorio actual."));
                            }
                        }

                        if out.is_empty() {
                            let Some(entry) = package_entry else {
                                out.push(String::from("Install error: paquete no encontrado en directorio actual."));
                                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                    for line in out.iter() {
                                        win.add_output(line.as_str());
                                    }
                                    win.render_terminal();
                                }
                                return;
                            };

                            if entry.size == 0 {
                                out.push(String::from("Install error: paquete vacio."));
                            } else if entry.size as usize > INSTALL_MAX_PACKAGE_BYTES {
                                out.push(alloc::format!(
                                    "Install error: paquete demasiado grande (max {} bytes).",
                                    INSTALL_MAX_PACKAGE_BYTES
                                ));
                            } else if package_is_deb && entry.size as usize > INSTALL_MAX_DEB_PACKAGE_BYTES {
                                out.push(alloc::format!(
                                    "Install error: .DEB demasiado grande para modo seguro (max {} bytes).",
                                    INSTALL_MAX_DEB_PACKAGE_BYTES
                                ));
                                out.push(String::from(
                                    "Tip: para .DEB grandes se requiere loader chunked (streaming) aun no habilitado.",
                                ));
                            } else if entry.cluster < 2 {
                                out.push(String::from("Install error: cluster de paquete invalido."));
                            } else {
                                if package_is_deb {
                                    install_prompt_active = self.begin_install_progress_prompt(package_name);
                                    if install_prompt_active {
                                        self.install_progress_set_target(40, Some("Preflight .deb..."));
                                    }
                                }
                                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                    win.add_output(
                                        alloc::format!(
                                            "Install: preflight {} ({} bytes)...",
                                            package_name, entry.size
                                        )
                                        .as_str(),
                                    );
                                    win.render_terminal();
                                }
                                self.paint();
                                let heap_total_dbg = crate::allocator::heap_size_bytes();
                                self.install_debug_log(
                                    win_id,
                                    alloc::format!(
                                        "Install DBG: heap={} MiB, file={} bytes",
                                        heap_total_dbg / (1024 * 1024),
                                        entry.size
                                    )
                                    .as_str(),
                                );
                                let estimated_working_set = Self::estimate_install_working_set_bytes(
                                    entry.size as usize,
                                    package_is_zip,
                                    package_is_targz,
                                    package_is_deb,
                                    package_is_exe,
                                );
                                let task_budget = Self::install_task_budget_bytes();
                                if estimated_working_set > task_budget && !package_is_deb {
                                    out.push(alloc::format!(
                                        "Install error: paquete excede RAM por tarea (estimado {} MiB, limite {} MiB).",
                                        estimated_working_set / (1024 * 1024),
                                        task_budget / (1024 * 1024)
                                    ));
                                    out.push(String::from(
                                        "Tip: usa paquete mas pequeno o incrementa heap disponible.",
                                    ));
                                }

                                let mut _install_heap_reservation: Option<crate::allocator::HeapReservation> = None;
                                if out.is_empty() && !package_is_deb {
                                    // For non-DEB packages, do full heap reservation check.
                                    match crate::allocator::try_reserve_heap(
                                        estimated_working_set,
                                        INSTALL_HEAP_HEADROOM_BYTES,
                                    ) {
                                        Some(token) => {
                                            _install_heap_reservation = Some(token);
                                        }
                                        None => {
                                            let heap_total = crate::allocator::heap_size_bytes();
                                            let heap_reserved = crate::allocator::heap_reserved_bytes();
                                            out.push(alloc::format!(
                                                "Install error: RAM insuficiente para reservar tarea (heap {} MiB, reservado {} MiB, solicitud {} MiB, headroom {} MiB).",
                                                heap_total / (1024 * 1024),
                                                heap_reserved / (1024 * 1024),
                                                estimated_working_set / (1024 * 1024),
                                                INSTALL_HEAP_HEADROOM_BYTES / (1024 * 1024),
                                            ));
                                            out.push(String::from(
                                                "Tip: cierra tareas activas o usa un paquete mas pequeno.",
                                            ));
                                        }
                                    }
                                }
                                if out.is_empty() && package_is_deb {
                                    self.install_debug_log(
                                        win_id,
                                        "Install DBG: DEB skip heap reservation, proceeding...",
                                    );
                                    if install_prompt_active {
                                        self.install_progress_set_target(80, Some("Inspeccion de paquete .deb..."));
                                    }
                                }

                                let mut deb_member_hint: Option<String> = None;
                                let mut deb_preflight_ok = !package_is_deb;
                                if out.is_empty() && package_is_deb {
                                    let preflight_len = (entry.size as usize).min(INSTALL_DEB_PREFLIGHT_BYTES);
                                    self.install_debug_log(
                                        win_id,
                                        alloc::format!(
                                            "Install DBG: DEB preflight alloc {} bytes...",
                                            preflight_len
                                        )
                                        .as_str(),
                                    );
                                    if preflight_len >= 8 {
                                        let mut preflight = match Self::try_alloc_zeroed(preflight_len) {
                                            Ok(v) => v,
                                            Err(err) => {
                                                out.push(alloc::format!(
                                                    "Install error: {}",
                                                    err
                                                ));
                                                Vec::new()
                                            }
                                        };
                                        if out.is_empty() {
                                            self.install_debug_log(
                                                win_id,
                                                alloc::format!(
                                                    "Install DBG: DEB preflight read cluster={} len={}...",
                                                    entry.cluster,
                                                    preflight_len
                                                )
                                                .as_str(),
                                            );
                                            match fat.read_file_sized(entry.cluster, preflight_len, &mut preflight) {
                                            Ok(pre_len) => {
                                                preflight.truncate(pre_len);
                                                match Self::probe_deb_data_member_name(preflight.as_slice()) {
                                                    Ok(Some(member_name)) => {
                                                        let member_lower = Self::ascii_lower(member_name.as_str());
                                                        deb_member_hint = Some(member_name.clone());
                                                        if !member_lower.ends_with(".tar")
                                                            && !member_lower.ends_with(".tar.gz")
                                                        {
                                                            out.push(alloc::format!(
                                                                "Install error: DEB usa {} (solo data.tar o data.tar.gz soportado).",
                                                                member_name
                                                            ));
                                                            out.push(String::from(
                                                                "Tip: usa un .deb con data.tar.gz o extrae/convierte el paquete.",
                                                            ));
                                                        } else {
                                                            deb_preflight_ok = true;
                                                            if install_prompt_active {
                                                                self.install_progress_set_target(
                                                                    160,
                                                                    Some("Paquete .deb validado."),
                                                                );
                                                            }
                                                        }
                                                    }
                                                    Ok(None) => {
                                                        out.push(alloc::format!(
                                                            "Install error: DEB preflight incompleto (data.tar* no visible en primeros {} bytes).",
                                                            preflight_len
                                                        ));
                                                        out.push(String::from(
                                                            "Tip: mueve el paquete a USB/FAT limpio o usa .tar/.tar.gz para evitar cuelgue.",
                                                        ));
                                                    }
                                                    Err(err) => out.push(alloc::format!(
                                                        "Install error: {}",
                                                        err
                                                    )),
                                                }
                                            }
                                            Err(err) => {
                                                out.push(alloc::format!("Install error: {}", err));
                                            }
                                        }
                                        }
                                    }
                                }

                                if out.is_empty() && package_is_deb && !deb_preflight_ok {
                                    out.push(String::from(
                                        "Install error: DEB no paso preflight de seguridad.",
                                    ));
                                }

                                if out.is_empty() {
                                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                        win.add_output(
                                            alloc::format!(
                                                "Install: leyendo {} ({} bytes), puede tardar...",
                                                package_name, entry.size
                                            )
                                            .as_str(),
                                        );
                                        if let Some(member_name) = deb_member_hint.as_ref() {
                                            win.add_output(
                                                alloc::format!("Install: DEB member detectado: {}", member_name)
                                                    .as_str(),
                                            );
                                        }
                                        win.render_terminal();
                                    }
                                }

                                let mut package_raw = Vec::new();
                                if out.is_empty() {
                                    self.install_debug_log(
                                        win_id,
                                        alloc::format!(
                                            "Install DBG: alloc {} bytes for full read...",
                                            entry.size
                                        )
                                        .as_str(),
                                    );
                                    package_raw = match Self::try_alloc_zeroed(entry.size as usize) {
                                        Ok(v) => {
                                            self.install_debug_log(
                                                win_id,
                                                "Install DBG: alloc OK, starting full read...",
                                            );
                                            v
                                        }
                                        Err(err) => {
                                            out.push(alloc::format!("Install error: {}", err));
                                            Vec::new()
                                        }
                                    };
                                }
                                if out.is_empty() {
                                    if install_prompt_active {
                                        self.install_progress_set_target(
                                            200,
                                            Some("Leyendo paquete .deb..."),
                                        );
                                    }
                                    if install_prompt_active {
                                        let read_start = 200usize;
                                        let read_span = 500usize;
                                        match fat.read_file_sized_with_progress(
                                            entry.cluster,
                                            entry.size as usize,
                                            &mut package_raw,
                                            |copied, total| {
                                                if self.copy_progress_cancel_requested() {
                                                    return false;
                                                }
                                                let pct_units = if total == 0 {
                                                    read_start + read_span
                                                } else {
                                                    read_start
                                                        + copied
                                                            .saturating_mul(read_span)
                                                            .saturating_div(total.max(1))
                                                };
                                                self.install_progress_set_target(pct_units, None);
                                                !self.copy_progress_cancel_requested()
                                            },
                                        ) {
                                            Ok(len) => {
                                                package_raw.truncate(len);
                                                self.install_progress_set_target(
                                                    read_start + read_span,
                                                    Some("Leyendo paquete .deb... OK"),
                                                );
                                            }
                                            Err(err) => {
                                                if Self::is_copy_cancel_error(err) {
                                                    out.push(String::from(
                                                        "Install cancelado por usuario.",
                                                    ));
                                                } else {
                                                    out.push(alloc::format!("Install error: {}", err));
                                                }
                                            }
                                        }
                                    } else {
                                        match fat.read_file_sized(
                                            entry.cluster,
                                            entry.size as usize,
                                            &mut package_raw,
                                        ) {
                                            Ok(len) => {
                                                package_raw.truncate(len);
                                            }
                                            Err(err) => {
                                                out.push(alloc::format!("Install error: {}", err));
                                            }
                                        }
                                    }
                                }

                                if out.is_empty() {
                                    if let Some(sig_entry) = package_signature_entry {
                                        package_signature_present = true;
                                        if sig_entry.cluster < 2 {
                                            out.push(String::from(
                                                "Install error: firma .sig invalida (cluster).",
                                            ));
                                        } else if sig_entry.size == 0 {
                                            out.push(String::from(
                                                "Install error: firma .sig vacia.",
                                            ));
                                        } else if sig_entry.size as usize > INSTALL_MAX_SIGNATURE_BYTES {
                                            out.push(alloc::format!(
                                                "Install error: firma .sig demasiado grande (max {} bytes).",
                                                INSTALL_MAX_SIGNATURE_BYTES
                                            ));
                                        } else {
                                            let mut sig_raw = match Self::try_alloc_zeroed(sig_entry.size as usize) {
                                                Ok(v) => v,
                                                Err(err) => {
                                                    out.push(alloc::format!("Install error: {}", err));
                                                    Vec::new()
                                                }
                                            };
                                            if out.is_empty() {
                                                match fat.read_file_sized(
                                                    sig_entry.cluster,
                                                    sig_entry.size as usize,
                                                    &mut sig_raw,
                                                ) {
                                                    Ok(sig_len) => {
                                                        sig_raw.truncate(sig_len);
                                                        match core::str::from_utf8(sig_raw.as_slice()) {
                                                            Ok(sig_text) => {
                                                                match Self::verify_install_package_signature(
                                                                    package_name,
                                                                    package_raw.as_slice(),
                                                                    sig_text,
                                                                ) {
                                                                    Ok(()) => {
                                                                        package_signature_verified = true;
                                                                    }
                                                                    Err(err) => {
                                                                        out.push(alloc::format!(
                                                                            "Install error: {}",
                                                                            err
                                                                        ));
                                                                    }
                                                                }
                                                            }
                                                            Err(_) => {
                                                                out.push(String::from(
                                                                    "Install error: firma .sig no es UTF-8.",
                                                                ));
                                                            }
                                                        }
                                                    }
                                                    Err(err) => {
                                                        out.push(alloc::format!("Install error: {}", err));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if out.is_empty() {
                                    if package_is_targz {
                                        match Self::extract_gzip_payload(package_raw.as_slice()) {
                                            Ok(inflated_tar) => {
                                                package_raw = inflated_tar;
                                                package_is_tar = true;
                                            }
                                            Err(err) => {
                                                out.push(alloc::format!("Install error: {}", err));
                                            }
                                        }
                                    }

                                    if out.is_empty() && package_is_deb {
                                        if install_prompt_active {
                                            self.install_progress_set_target(
                                                760,
                                                Some("Extrayendo data.tar desde .deb..."),
                                            );
                                        }
                                        self.install_debug_log(
                                            win_id,
                                            alloc::format!(
                                                "Install DBG: DEB inplace extract ({} bytes)...",
                                                package_raw.len()
                                            )
                                            .as_str(),
                                        );
                                        match Self::extract_deb_data_member_inplace(&mut package_raw) {
                                            Ok((member_name, is_gz)) => {
                                                if install_prompt_active {
                                                    self.install_progress_set_target(
                                                        810,
                                                        Some("data.tar detectado."),
                                                    );
                                                }
                                                self.install_debug_log(
                                                    win_id,
                                                    alloc::format!(
                                                        "Install DBG: member '{}' extracted inplace ({} bytes, gz={}).",
                                                        member_name,
                                                        package_raw.len(),
                                                        is_gz
                                                    )
                                                    .as_str(),
                                                );
                                                let member_lower = Self::ascii_lower(member_name.as_str());
                                                if !is_gz && member_lower.ends_with(".tar") {
                                                    // Already in-place as raw tar
                                                    package_is_tar = true;
                                                    if install_prompt_active {
                                                        self.install_progress_set_target(
                                                            860,
                                                            Some("Preparando instalacion de archivos..."),
                                                        );
                                                    }
                                                } else if is_gz {
                                                    if install_prompt_active {
                                                        self.install_progress_set_target(
                                                            840,
                                                            Some("Descomprimiendo data.tar.gz..."),
                                                        );
                                                    }
                                                    self.install_debug_log(
                                                        win_id,
                                                        alloc::format!(
                                                            "Install DBG: gzip inflate {} bytes (limit {} MiB)...",
                                                            package_raw.len(),
                                                            INSTALL_DEB_MAX_INFLATED_BYTES / (1024 * 1024)
                                                        )
                                                        .as_str(),
                                                    );
                                                    match Self::extract_gzip_payload_with_limit(
                                                        package_raw.as_slice(),
                                                        INSTALL_DEB_MAX_INFLATED_BYTES,
                                                    ) {
                                                        Ok(inflated_tar) => {
                                                            let inflated_len = inflated_tar.len();
                                                            // Drop compressed buffer before assigning inflated
                                                            drop(core::mem::replace(&mut package_raw, inflated_tar));
                                                            package_is_tar = true;
                                                            if install_prompt_active {
                                                                self.install_progress_set_target(
                                                                    900,
                                                                    Some(
                                                                        "Descompresion lista. Instalando archivos...",
                                                                    ),
                                                                );
                                                            }
                                                            self.install_debug_log(
                                                                win_id,
                                                                alloc::format!(
                                                                    "Install DBG: inflated OK ({} bytes).",
                                                                    inflated_len
                                                                )
                                                                .as_str(),
                                                            );
                                                        }
                                                        Err(err) => {
                                                            let heap_free = crate::allocator::heap_size_bytes()
                                                                .saturating_sub(crate::allocator::heap_reserved_bytes());
                                                            out.push(alloc::format!(
                                                                "Install error: data.tar.gz invalido: {} (compressed={} bytes, heap_total={} MiB, heap_free~={} MiB)",
                                                                err,
                                                                package_raw.len(),
                                                                crate::allocator::heap_size_bytes() / (1024 * 1024),
                                                                heap_free / (1024 * 1024),
                                                            ));
                                                        }
                                                    }
                                                } else {
                                                    out.push(alloc::format!(
                                                        "Install error: DEB usa {} (solo data.tar o data.tar.gz soportado).",
                                                        member_name
                                                    ));
                                                }
                                            }
                                            Err(err) => out.push(alloc::format!("Install error: {}", err)),
                                        }
                                    }

                                    if out.is_empty() && package_is_exe {
                                        if package_raw.len() < 2 || &package_raw[0..2] != b"MZ" {
                                            out.push(String::from(
                                                "Install error: EXE invalido (firma MZ).",
                                            ));
                                        } else if let Some(zip_offset) =
                                            Self::find_bytes(package_raw.as_slice(), b"PK\x03\x04")
                                        {
                                            match Self::try_copy_slice(&package_raw[zip_offset..]) {
                                                Ok(copied) => {
                                                    package_raw = copied;
                                                    package_is_zip = true;
                                                }
                                                Err(err) => {
                                                    out.push(alloc::format!(
                                                        "Install error: {}",
                                                        err
                                                    ));
                                                }
                                            }
                                        } else {
                                            out.push(String::from(
                                                "Install error: EXE nativo no soportado aun (solo EXE self-extracting ZIP).",
                                            ));
                                        }
                                    }
                                }

                                if out.is_empty() {
                                    let app_seed = app_id_arg.unwrap_or_else(|| Self::filename_stem(package_name));
                                    let app_tag8 = Self::sanitize_short_component(app_seed, 8, "APP");
                                    let app_tag4 = Self::sanitize_short_component(app_seed, 4, "APP");
                                    let mut manifest = String::new();
                                    let mut files_written = 0usize;
                                    let mut shortcut_layout_name: Option<String> = None;
                                    let mut shortcut_linux_candidate: Option<LinuxInstallShortcutCandidate> = None;
                                    let mut autoport_profile: Option<String> = None;
                                    let mut autoport_target: Option<String> = None;
                                    let mut autoport_command: Option<String> = None;
                                    let mut runtime_targets: Option<LinuxRuntimeTargets> = None;
                                    let mut runtime_manifest = String::new();
                                    let mut runtime_files_written = 0usize;
                                    let mut runtime_stage_warned = false;
                                    let mut target_cluster = current_cluster;

                                    if out.is_empty() {
                                        let subdir_name = app_tag8.as_str();
                                        match fat.ensure_subdirectory(current_cluster, subdir_name) {
                                            Ok(c) => {
                                                target_cluster = c;
                                                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                                    win.add_output(
                                                        alloc::format!(
                                                            "Install: Descomprimiendo en ./{}/ ...",
                                                            subdir_name
                                                        ).as_str()
                                                    );
                                                    win.render_terminal();
                                                }
                                                self.paint();
                                            }
                                            Err(e) => {
                                                out.push(alloc::format!("Install error creando subdirectorio: {}", e));
                                            }
                                        }
                                    }

                                    if package_is_rpx {
                                        if package_raw.len() < 8 || &package_raw[0..4] != b"RPX1" {
                                            out.push(String::from("Install error: RPX invalido (magic)."));
                                        } else {
                                            let mut cursor = 4usize;
                                            let Some(file_count_u32) = Self::read_u32_le(package_raw.as_slice(), &mut cursor) else {
                                                out.push(String::from("Install error: RPX corrupto (header)."));
                                                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                                    for line in out.iter() {
                                                        win.add_output(line.as_str());
                                                    }
                                                    win.render_terminal();
                                                }
                                                return;
                                            };
                                            let file_count = file_count_u32 as usize;
                                            manifest = alloc::format!(
                                                "RPX INSTALL\nPACKAGE={}\nFILES={}\n",
                                                package_name, file_count
                                            );

                                            for idx in 0..file_count {
                                                let Some(path_len_u16) =
                                                    Self::read_u16_le(package_raw.as_slice(), &mut cursor)
                                                else {
                                                    out.push(String::from(
                                                        "Install error: RPX corrupto (path_len).",
                                                    ));
                                                    break;
                                                };
                                                let Some(size_u32) =
                                                    Self::read_u32_le(package_raw.as_slice(), &mut cursor)
                                                else {
                                                    out.push(String::from("Install error: RPX corrupto (size)."));
                                                    break;
                                                };

                                                let path_len = path_len_u16 as usize;
                                                let file_size = size_u32 as usize;
                                                if cursor + path_len > package_raw.len() {
                                                    out.push(String::from("Install error: RPX corrupto (path data)."));
                                                    break;
                                                }
                                                let path_bytes = &package_raw[cursor..cursor + path_len];
                                                cursor += path_len;
                                                if cursor + file_size > package_raw.len() {
                                                    out.push(String::from("Install error: RPX corrupto (file data)."));
                                                    break;
                                                }
                                                let payload = &package_raw[cursor..cursor + file_size];
                                                cursor += file_size;

                                                let path_text =
                                                    String::from_utf8_lossy(path_bytes).into_owned();
                                                let out_name = Self::short_install_name(
                                                    app_tag4.as_str(),
                                                    path_text.as_str(),
                                                    idx + 1,
                                                );

                                                match fat.write_text_file_in_dir(
                                                    target_cluster,
                                                    out_name.as_str(),
                                                    payload,
                                                ) {
                                                    Ok(()) => {
                                                        files_written += 1;
                                                        if shortcut_layout_name.is_none()
                                                            && (Self::is_rml_file_name(path_text.as_str())
                                                                || Self::is_rml_file_name(out_name.as_str()))
                                                        {
                                                            shortcut_layout_name = Some(out_name.clone());
                                                        }
                                                        Self::consider_linux_shortcut_candidate(
                                                            path_text.as_str(),
                                                            out_name.as_str(),
                                                            payload,
                                                            &mut shortcut_linux_candidate,
                                                        );
                                                        manifest.push_str(
                                                            alloc::format!(
                                                                "{:04} {} <- {}\n",
                                                                idx + 1,
                                                                out_name,
                                                                path_text
                                                            )
                                                            .as_str(),
                                                        );
                                                        if let Err(err) = Self::maybe_stage_linux_runtime_file(
                                                            fat,
                                                            path_text.as_str(),
                                                            out_name.as_str(),
                                                            payload,
                                                            &mut runtime_targets,
                                                            &mut runtime_manifest,
                                                            &mut runtime_files_written,
                                                        ) {
                                                            if !runtime_stage_warned {
                                                                out.push(alloc::format!("Install runtime warning: {}", err));
                                                                runtime_stage_warned = true;
                                                            }
                                                        }
                                                        self.pump_ui_while_installing(win_id, files_written);
                                                    }
                                                    Err(err) => {
                                                        out.push(alloc::format!(
                                                            "Install error writing {}: {}",
                                                            out_name,
                                                            err
                                                        ));
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    } else if package_is_zip {
                                        if package_raw.len() < 4 || &package_raw[0..2] != b"PK" {
                                            out.push(String::from("Install error: ZIP invalido (firma)."));
                                        } else {
                                            let (mut central_entries, central_offset) =
                                                match Self::parse_zip_central_directory(
                                                    package_raw.as_slice(),
                                                ) {
                                                    Some(v) => v,
                                                    None => {
                                                        out.push(String::from(
                                                            "Install error: ZIP corrupto (central directory).",
                                                        ));
                                                        (Vec::new(), 0usize)
                                                    }
                                                };
                                            central_entries.sort_by_key(|e| e.0);

                                            let mut cursor = 0usize;
                                            let mut zip_index = 0usize;
                                            let mut parsed_files = 0usize;
                                            manifest = alloc::format!("ZIP INSTALL\nPACKAGE={}\n", package_name);

                                            while out.is_empty() && cursor + 4 <= package_raw.len() {
                                                let local_offset = cursor;
                                                let sig = u32::from_le_bytes([
                                                    package_raw[cursor],
                                                    package_raw[cursor + 1],
                                                    package_raw[cursor + 2],
                                                    package_raw[cursor + 3],
                                                ]);

                                                if sig == 0x0201_4B50 || sig == 0x0605_4B50 {
                                                    break;
                                                }
                                                if sig != 0x0403_4B50 {
                                                    out.push(String::from(
                                                        "Install error: ZIP corrupto (local header).",
                                                    ));
                                                    break;
                                                }

                                                cursor += 4;
                                                let _version = match Self::read_u16_le(package_raw.as_slice(), &mut cursor) {
                                                    Some(v) => v,
                                                    None => {
                                                        out.push(String::from("Install error: ZIP corrupto (version)."));
                                                        break;
                                                    }
                                                };
                                                let flags = match Self::read_u16_le(package_raw.as_slice(), &mut cursor) {
                                                    Some(v) => v,
                                                    None => {
                                                        out.push(String::from("Install error: ZIP corrupto (flags)."));
                                                        break;
                                                    }
                                                };
                                                let mut method = match Self::read_u16_le(package_raw.as_slice(), &mut cursor) {
                                                    Some(v) => v,
                                                    None => {
                                                        out.push(String::from("Install error: ZIP corrupto (method)."));
                                                        break;
                                                    }
                                                };
                                                let _mod_time = Self::read_u16_le(package_raw.as_slice(), &mut cursor);
                                                let _mod_date = Self::read_u16_le(package_raw.as_slice(), &mut cursor);
                                                let _crc32 = Self::read_u32_le(package_raw.as_slice(), &mut cursor);
                                                let mut comp_size =
                                                    match Self::read_u32_le(package_raw.as_slice(), &mut cursor) {
                                                    Some(v) => v as usize,
                                                    None => {
                                                        out.push(String::from("Install error: ZIP corrupto (comp size)."));
                                                        break;
                                                    }
                                                };
                                                let mut uncomp_size =
                                                    match Self::read_u32_le(package_raw.as_slice(), &mut cursor) {
                                                        Some(v) => v as usize,
                                                        None => {
                                                            out.push(String::from(
                                                                "Install error: ZIP corrupto (uncomp size).",
                                                            ));
                                                            break;
                                                        }
                                                    };
                                                let name_len = match Self::read_u16_le(package_raw.as_slice(), &mut cursor) {
                                                    Some(v) => v as usize,
                                                    None => {
                                                        out.push(String::from("Install error: ZIP corrupto (name len)."));
                                                        break;
                                                    }
                                                };
                                                let extra_len = match Self::read_u16_le(package_raw.as_slice(), &mut cursor) {
                                                    Some(v) => v as usize,
                                                    None => {
                                                        out.push(String::from("Install error: ZIP corrupto (extra len)."));
                                                        break;
                                                    }
                                                };

                                                if (flags & 0x0008) != 0 {
                                                    let Some((_, cd_comp, cd_uncomp, cd_method)) =
                                                        central_entries
                                                            .iter()
                                                            .find(|entry| entry.0 == local_offset)
                                                    else {
                                                        out.push(String::from(
                                                            "Install error: ZIP descriptor sin entrada central.",
                                                        ));
                                                        break;
                                                    };
                                                    comp_size = *cd_comp;
                                                    uncomp_size = *cd_uncomp;
                                                    method = *cd_method;
                                                }

                                                if cursor + name_len > package_raw.len() {
                                                    out.push(String::from("Install error: ZIP corrupto (file name)."));
                                                    break;
                                                }
                                                let name_bytes = &package_raw[cursor..cursor + name_len];
                                                cursor += name_len;
                                                if cursor + extra_len > package_raw.len() {
                                                    out.push(String::from("Install error: ZIP corrupto (extra data)."));
                                                    break;
                                                }
                                                cursor += extra_len;

                                                if cursor + comp_size > package_raw.len() {
                                                    out.push(String::from("Install error: ZIP corrupto (file payload)."));
                                                    break;
                                                }
                                                let payload = &package_raw[cursor..cursor + comp_size];
                                                cursor += comp_size;

                                                if (flags & 0x0008) != 0 {
                                                    let mut next_local_offset = central_offset;
                                                    for entry in central_entries.iter() {
                                                        if entry.0 > local_offset
                                                            && (next_local_offset == central_offset
                                                                || entry.0 < next_local_offset)
                                                        {
                                                            next_local_offset = entry.0;
                                                        }
                                                    }

                                                    if next_local_offset < cursor
                                                        || next_local_offset > package_raw.len()
                                                    {
                                                        out.push(String::from(
                                                            "Install error: ZIP descriptor fuera de rango.",
                                                        ));
                                                        break;
                                                    }
                                                    cursor = next_local_offset;
                                                }

                                                let path_text =
                                                    String::from_utf8_lossy(name_bytes).into_owned();
                                                if path_text.ends_with('/') || path_text.ends_with('\\') {
                                                    continue;
                                                }
                                                if !Self::is_installable_zip_path(path_text.as_str()) {
                                                    continue;
                                                }

                                                let payload_buf: Option<Vec<u8>> = match method {
                                                    0 => {
                                                        if comp_size != uncomp_size {
                                                            out.push(String::from(
                                                                "Install error: ZIP STORE con tamano inconsistente.",
                                                            ));
                                                            break;
                                                        }
                                                        None
                                                    }
                                                    8 => {
                                                        if uncomp_size > INSTALL_MAX_EXPANDED_FILE_BYTES {
                                                            out.push(alloc::format!(
                                                                "Install error: ZIP entry descomprimida demasiado grande (max {} bytes, entry {}).",
                                                                INSTALL_MAX_EXPANDED_FILE_BYTES, path_text
                                                            ));
                                                            break;
                                                        }
                                                        let inflate_limit = if uncomp_size == 0 {
                                                            INSTALL_MAX_EXPANDED_FILE_BYTES
                                                        } else {
                                                            uncomp_size
                                                        };
                                                        match decompress_to_vec_with_limit(payload, inflate_limit) {
                                                            Ok(raw) => {
                                                                if uncomp_size != 0 && raw.len() != uncomp_size {
                                                                    out.push(alloc::format!(
                                                                        "Install error: ZIP DEFLATE con tamano inconsistente (esperado {}, real {}, entry {}).",
                                                                        uncomp_size,
                                                                        raw.len(),
                                                                        path_text
                                                                    ));
                                                                    break;
                                                                }
                                                                Some(raw)
                                                            }
                                                            Err(_) => {
                                                                out.push(alloc::format!(
                                                                    "Install error: ZIP DEFLATE invalido (entry {}).",
                                                                    path_text
                                                                ));
                                                                break;
                                                            }
                                                        }
                                                    }
                                                    _ => {
                                                        out.push(alloc::format!(
                                                            "Install error: ZIP compression method {} no soportado aun (entry {}).",
                                                            method, path_text
                                                        ));
                                                        break;
                                                    }
                                                };
                                                let payload_out: &[u8] = match payload_buf.as_ref() {
                                                    Some(v) => v.as_slice(),
                                                    None => payload,
                                                };

                                                zip_index += 1;
                                                parsed_files += 1;
                                                let out_name = Self::short_install_name(
                                                    app_tag4.as_str(),
                                                    path_text.as_str(),
                                                    zip_index,
                                                );

                                                match fat.write_text_file_in_dir(
                                                    target_cluster,
                                                    out_name.as_str(),
                                                    payload_out,
                                                ) {
                                                    Ok(()) => {
                                                        files_written += 1;
                                                        if shortcut_layout_name.is_none()
                                                            && (Self::is_rml_file_name(path_text.as_str())
                                                                || Self::is_rml_file_name(out_name.as_str()))
                                                        {
                                                            shortcut_layout_name = Some(out_name.clone());
                                                        }
                                                        Self::consider_linux_shortcut_candidate(
                                                            path_text.as_str(),
                                                            out_name.as_str(),
                                                            payload_out,
                                                            &mut shortcut_linux_candidate,
                                                        );
                                                        manifest.push_str(
                                                            alloc::format!(
                                                                "{:04} {} <- {}\n",
                                                                zip_index,
                                                                out_name,
                                                                path_text
                                                            )
                                                            .as_str(),
                                                        );
                                                        if let Err(err) = Self::maybe_stage_linux_runtime_file(
                                                            fat,
                                                            path_text.as_str(),
                                                            out_name.as_str(),
                                                            payload_out,
                                                            &mut runtime_targets,
                                                            &mut runtime_manifest,
                                                            &mut runtime_files_written,
                                                        ) {
                                                            if !runtime_stage_warned {
                                                                out.push(alloc::format!("Install runtime warning: {}", err));
                                                                runtime_stage_warned = true;
                                                            }
                                                        }
                                                        self.pump_ui_while_installing(win_id, files_written);
                                                    }
                                                    Err(err) => {
                                                        out.push(alloc::format!(
                                                            "Install error writing {}: {}",
                                                            out_name,
                                                            err
                                                        ));
                                                        break;
                                                    }
                                                }
                                            }

                                            if out.is_empty() && parsed_files == 0 {
                                                out.push(String::from(
                                                    "Install error: ZIP sin archivos instalables.",
                                                ));
                                            }
                                        }
                                    } else if package_is_tar {
                                        if install_prompt_active && package_is_deb {
                                            self.install_progress_set_target(
                                                920,
                                                Some("Instalando archivos del paquete..."),
                                            );
                                        }
                                        self.install_tar_archive(
                                            win_id,
                                            fat,
                                            target_cluster,
                                            app_tag4.as_str(),
                                            package_name,
                                            package_raw.as_slice(),
                                            &mut manifest,
                                            &mut files_written,
                                            &mut shortcut_layout_name,
                                            &mut shortcut_linux_candidate,
                                            &mut runtime_targets,
                                            &mut runtime_manifest,
                                            &mut runtime_files_written,
                                            &mut runtime_stage_warned,
                                            &mut out,
                                        );
                                    } else {
                                        out.push(String::from("Install error: tipo de paquete interno invalido."));
                                    }

                                    if out.is_empty() {
                                        let manifest_name = alloc::format!("{}.LST", app_tag8);
                                        let _ = fat.write_text_file_in_dir(
                                            target_cluster,
                                            manifest_name.as_str(),
                                            manifest.as_bytes(),
                                        );
                                        if runtime_files_written > 0 {
                                            if let Some(targets) = runtime_targets.as_ref() {
                                                let runtime_manifest_name =
                                                    alloc::format!("RT{}.LST", app_tag4.as_str());
                                                if fat
                                                    .write_text_file_in_dir(
                                                        targets.root_cluster,
                                                        runtime_manifest_name.as_str(),
                                                        runtime_manifest.as_bytes(),
                                                    )
                                                    .is_ok()
                                                {
                                                    out.push(alloc::format!(
                                                        "Linux runtime: {} libs staged in /LINUXRT.",
                                                        runtime_files_written
                                                    ));
                                                    out.push(alloc::format!(
                                                        "Linux runtime manifest: /LINUXRT/{}",
                                                        runtime_manifest_name
                                                    ));
                                                } else {
                                                    out.push(String::from(
                                                        "Linux runtime warning: no se pudo guardar manifiesto /LINUXRT.",
                                                    ));
                                                }
                                            }
                                        }
                                        if package_signature_present && package_signature_verified {
                                            out.push(String::from(
                                                "Install signature: verificada (.sig SHA256).",
                                            ));
                                        } else if !package_signature_present {
                                            out.push(String::from(
                                                "Install signature: no se encontro .sig (sin verificacion).",
                                            ));
                                        }
                                        out.push(alloc::format!(
                                            "Install: {} archivos instalados.",
                                            files_written
                                        ));
                                        out.push(alloc::format!("Manifest: {}", manifest_name));
                                        out.push(String::from(
                                            "Tip: usa 'ls' y 'cat <manifest>' para ver el mapeo.",
                                        ));

                                        if let Some(layout_name) = shortcut_layout_name.as_ref() {
                                            let layout_path = if target_cluster != current_cluster {
                                                alloc::format!("/{}/{}", app_tag8, layout_name)
                                            } else {
                                                alloc::format!("/{}", layout_name)
                                            };
                                            match Self::write_install_shortcut_file(
                                                fat,
                                                app_tag4.as_str(),
                                                package_name,
                                                app_id_arg,
                                                layout_path.as_str(),
                                            ) {
                                                Ok((shortcut_file, shortcut_label)) => {
                                                    out.push(alloc::format!(
                                                        "Start shortcut: {} -> {}",
                                                        shortcut_label, shortcut_file
                                                    ));
                                                    let shortcut_cmd = alloc::format!(
                                                        "runapp {}",
                                                        layout_path.as_str()
                                                    );
                                                    Self::push_unique_start_app_shortcut(
                                                        &mut self.start_app_shortcuts,
                                                        shortcut_label.as_str(),
                                                        shortcut_cmd.as_str(),
                                                    );
                                                    if autoport_enabled {
                                                        autoport_profile =
                                                            Some(String::from("native-redux-rml"));
                                                        autoport_target = Some(layout_path.clone());
                                                        autoport_command = Some(shortcut_cmd.clone());
                                                    }
                                                }
                                                Err(err) => {
                                                    out.push(alloc::format!(
                                                        "Start shortcut warning: {}",
                                                        err
                                                    ));
                                                }
                                            }
                                        } else if let Some(candidate) = shortcut_linux_candidate.as_ref() {
                                            let target_path = if target_cluster != current_cluster {
                                                alloc::format!("/{}/{}", app_tag8, candidate.exec_name)
                                            } else {
                                                alloc::format!("/{}", candidate.exec_name)
                                            };
                                            let runloop_action =
                                                if autoport_enabled
                                                    && candidate.mode
                                                        == LinuxInstallLaunchMode::Phase2Dynamic
                                                {
                                                    "startx"
                                                } else {
                                                    "start"
                                                };
                                            let shortcut_cmd = alloc::format!(
                                                "linux runloop {} {}",
                                                runloop_action,
                                                target_path.as_str()
                                            );
                                            match Self::write_install_shortcut_command(
                                                fat,
                                                app_tag4.as_str(),
                                                package_name,
                                                app_id_arg,
                                                shortcut_cmd.as_str(),
                                                Some(candidate.mode.suffix()),
                                            ) {
                                                Ok((shortcut_file, shortcut_label)) => {
                                                    out.push(alloc::format!(
                                                        "Start shortcut: {} -> {}",
                                                        shortcut_label, shortcut_file
                                                    ));
                                                    Self::push_unique_start_app_shortcut(
                                                        &mut self.start_app_shortcuts,
                                                        shortcut_label.as_str(),
                                                        shortcut_cmd.as_str(),
                                                    );
                                                    out.push(alloc::format!(
                                                        "Linux launch target: {} ({})",
                                                        candidate.exec_name,
                                                        candidate.mode.descriptor()
                                                    ));
                                                    if let Some(interp) = candidate.interp_path.as_deref() {
                                                        out.push(alloc::format!(
                                                            "Linux metadata: PT_INTERP={}",
                                                            interp
                                                        ));
                                                    }
                                                    if !candidate.needed.is_empty() {
                                                        out.push(alloc::format!(
                                                            "Linux metadata: DT_NEEDED={} (ejemplo: {}).",
                                                            candidate.needed.len(),
                                                            candidate.needed[0]
                                                        ));
                                                    }
                                                    match Self::write_install_linux_launch_metadata(
                                                        fat,
                                                        target_cluster,
                                                        app_tag8.as_str(),
                                                        package_name,
                                                        app_id_arg,
                                                        target_path.as_str(),
                                                        shortcut_cmd.as_str(),
                                                        candidate,
                                                    ) {
                                                        Ok(metadata_name) => {
                                                            if target_cluster != current_cluster {
                                                                out.push(alloc::format!(
                                                                    "Linux launch manifest: /{}/{}",
                                                                    app_tag8,
                                                                    metadata_name
                                                                ));
                                                            } else {
                                                                out.push(alloc::format!(
                                                                    "Linux launch manifest: /{}",
                                                                    metadata_name
                                                                ));
                                                            }
                                                        }
                                                        Err(err) => {
                                                            out.push(alloc::format!(
                                                                "Linux launch manifest warning: {}",
                                                                err
                                                            ));
                                                        }
                                                    }
                                                    if autoport_enabled {
                                                        let profile = match candidate.mode {
                                                            LinuxInstallLaunchMode::Phase1Static => {
                                                                "linux-compat-phase1"
                                                            }
                                                            LinuxInstallLaunchMode::Phase2Dynamic => {
                                                                if runloop_action == "startx" {
                                                                    "linux-compat-phase2-transfer"
                                                                } else {
                                                                    "linux-compat-phase2-safe"
                                                                }
                                                            }
                                                        };
                                                        autoport_profile = Some(String::from(profile));
                                                        autoport_target = Some(target_path.clone());
                                                        autoport_command = Some(shortcut_cmd.clone());
                                                    }
                                                }
                                                Err(err) => {
                                                    out.push(alloc::format!(
                                                        "Start shortcut warning: {}",
                                                        err
                                                    ));
                                                }
                                            }
                                        } else {
                                            out.push(String::from(
                                                "Start shortcut: no se encontro .RML ni ELF Linux instalable (omitido).",
                                            ));
                                        }

                                        if autoport_enabled {
                                            if let Some(profile) = autoport_profile.as_ref() {
                                                out.push(alloc::format!(
                                                    "AutoPort: perfil detectado -> {}",
                                                    profile
                                                ));
                                                if let Some(target) = autoport_target.as_ref() {
                                                    out.push(alloc::format!(
                                                        "AutoPort: target -> {}",
                                                        target
                                                    ));
                                                }
                                                if let Some(command) = autoport_command.as_ref() {
                                                    out.push(alloc::format!(
                                                        "AutoPort: comando -> {}",
                                                        command
                                                    ));
                                                }

                                                let autoport_manifest_name =
                                                    alloc::format!("{}.PRT", app_tag8.as_str());
                                                let mut autoport_manifest = String::new();
                                                autoport_manifest.push_str("AUTOPORT V1\n");
                                                autoport_manifest.push_str(
                                                    alloc::format!("PACKAGE={}\n", package_name).as_str(),
                                                );
                                                if let Some(app_id) = app_id_arg {
                                                    let app_id_trim = app_id.trim();
                                                    if !app_id_trim.is_empty() {
                                                        autoport_manifest.push_str(
                                                            alloc::format!("APP_ID={}\n", app_id_trim)
                                                                .as_str(),
                                                        );
                                                    }
                                                }
                                                autoport_manifest.push_str(
                                                    alloc::format!("PROFILE={}\n", profile).as_str(),
                                                );
                                                if let Some(target) = autoport_target.as_ref() {
                                                    autoport_manifest.push_str(
                                                        alloc::format!("TARGET={}\n", target).as_str(),
                                                    );
                                                }
                                                if let Some(command) = autoport_command.as_ref() {
                                                    autoport_manifest.push_str(
                                                        alloc::format!("COMMAND={}\n", command).as_str(),
                                                    );
                                                }
                                                match fat.write_text_file_in_dir(
                                                    target_cluster,
                                                    autoport_manifest_name.as_str(),
                                                    autoport_manifest.as_bytes(),
                                                ) {
                                                    Ok(()) => {
                                                        if target_cluster != current_cluster {
                                                            out.push(alloc::format!(
                                                                "AutoPort manifest: /{}/{}",
                                                                app_tag8, autoport_manifest_name
                                                            ));
                                                        } else {
                                                            out.push(alloc::format!(
                                                                "AutoPort manifest: /{}",
                                                                autoport_manifest_name
                                                            ));
                                                        }
                                                    }
                                                    Err(err) => {
                                                        out.push(alloc::format!(
                                                            "AutoPort warning: {}",
                                                            err
                                                        ));
                                                    }
                                                }
                                            } else {
                                                out.push(String::from(
                                                    "AutoPort: no se detecto entrypoint nativo/compat en este paquete.",
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if install_prompt_active {
                let install_failed = out.iter().any(|line| {
                    let lower = Self::ascii_lower(line.as_str());
                    lower.starts_with("install error:") || lower.starts_with("install cancelado")
                });
                self.finish_install_progress_prompt(!install_failed);
            }

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in out.iter() {
                    win.add_output(line.as_str());
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "ruby" || verb == "rb" {
            let mut out = Vec::new();
            let mut script: Option<String> = None;

            let arg = arg_raw.trim();
            if arg.is_empty() || arg == "-h" || arg == "--help" || arg == "help" {
                out.push(String::from("Ruby runtime (subset)"));
                out.push(String::from("Usage:"));
                out.push(String::from("  ruby -e <code>"));
                out.push(String::from("  ruby eval <code>"));
                out.push(String::from("  ruby <file.rb>"));
                out.push(String::from("Examples:"));
                out.push(String::from("  ruby -e puts 2 + 3"));
                out.push(String::from("  ruby -e a = 10; puts a * 3"));
                out.push(String::from("  ruby hello.rb"));
                out.push(String::from("Supported: variables, puts, print, strings, + - * / %."));
            } else if let Some(code) = arg.strip_prefix("-e ") {
                let code = Self::trim_wrapping_quotes(code.trim());
                script = Some(String::from(code));
            } else if let Some(code) = arg.strip_prefix("eval ") {
                let code = Self::trim_wrapping_quotes(code.trim());
                script = Some(String::from(code));
            } else {
                if fat.bytes_per_sector == 0 {
                    if self.manual_unmount_lock {
                        out.push(String::from(
                            "Ruby: volume unmounted. Use 'mount <n>' first.",
                        ));
                    } else if !fat.init() {
                        out.push(String::from(
                            "Ruby: filesystem unavailable. Use 'disks' and 'mount <n>'.",
                        ));
                    }
                } else {
                    let current_cluster = match self.windows.iter().find(|w| w.id == win_id) {
                        Some(win) => {
                            if win.current_dir_cluster == 0 {
                                fat.root_cluster
                            } else {
                                win.current_dir_cluster
                            }
                        }
                        None => return,
                    };

                    let filename = arg;
                    match fat.read_dir_entries(current_cluster) {
                        Ok(entries) => {
                            let mut found = false;
                            for entry in entries.iter() {
                                if !entry.valid || entry.file_type != FileType::File {
                                    continue;
                                }
                                if !entry.matches_name(filename) {
                                    continue;
                                }
                                found = true;

                                if entry.size as usize > RUBY_SCRIPT_MAX_BYTES {
                                    out.push(alloc::format!(
                                        "Ruby: script too large (max {} bytes).",
                                        RUBY_SCRIPT_MAX_BYTES
                                    ));
                                    break;
                                }

                                let target = entry.size as usize;
                                let mut buffer = Vec::new();
                                buffer.resize(target, 0);

                                match fat.read_file_sized(entry.cluster, target, &mut buffer) {
                                    Ok(len) => match core::str::from_utf8(&buffer[..len]) {
                                        Ok(text) => {
                                            script = Some(String::from(text));
                                        }
                                        Err(_) => {
                                            out.push(String::from(
                                                "Ruby: script must be UTF-8 text.",
                                            ));
                                        }
                                    },
                                    Err(_) => {
                                        out.push(String::from("Ruby: failed to read script file."));
                                    }
                                }
                                break;
                            }

                            if !found {
                                out.push(String::from("Ruby: file not found in current directory."));
                            }
                        }
                        Err(_) => {
                            out.push(String::from("Ruby: failed to read current directory."));
                        }
                    }
                }
            }

            if out.is_empty() {
                if let Some(source) = script {
                    match crate::ruby_runtime::eval(source.as_str()) {
                        Ok(lines) => {
                            if lines.is_empty() {
                                out.push(String::from("Ruby: OK"));
                            } else {
                                for line in lines {
                                    out.push(line);
                                }
                            }
                        }
                        Err(err) => {
                            out.push(alloc::format!("Ruby error: {}", err));
                        }
                    }
                }
            }

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in out.iter() {
                    win.add_output(line.as_str());
                }
                win.render_terminal();
            }
            return;
        }

        if verb == "runapp" {
            let mut out = Vec::new();
            let arg = arg_raw.trim();

            if arg.is_empty() {
                out.push(String::from("Usage: runapp <layout.rml>"));
                out.push(String::from("Example: runapp MAIN.RML"));
            } else {
                let lower_name = Self::ascii_lower(arg);
                if !lower_name.ends_with(".rml") {
                    out.push(String::from("RunApp error: expected a .RML file."));
                } else if fat.bytes_per_sector == 0 {
                    if self.manual_unmount_lock {
                        out.push(String::from(
                            "RunApp error: volumen desmontado. Usa 'mount <n>' primero.",
                        ));
                    } else if !fat.init() {
                        out.push(String::from(
                            "RunApp error: FAT32 no disponible. Usa 'disks' y 'mount <n>'.",
                        ));
                    }
                } else {
                    let current_cluster = match self.windows.iter().find(|w| w.id == win_id) {
                        Some(win) => {
                            if win.current_dir_cluster == 0 {
                                fat.root_cluster
                            } else {
                                win.current_dir_cluster
                            }
                        }
                        None => fat.root_cluster,
                    };
                    self.run_app_layout_from_cluster(fat, current_cluster, arg, &mut out);
                }
            }

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in out.iter() {
                    win.add_output(line.as_str());
                }
                win.render_terminal();
            }
            return;
        }

        if is_fs_cmd {
            if fat.bytes_per_sector == 0 {
                if self.manual_unmount_lock {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.add_output("Volume unmounted. Use 'mount <n>' to mount again.");
                    }
                    return;
                }
                if !fat.init() {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                        win.add_output("Error: Filesystem initialization failed.");
                        win.add_output("Could not auto-mount FAT32. Use 'disks' then 'mount <n>'.");
                    }
                    return;
                }
            }
        }

        if fat.bytes_per_sector == 0 {
            if verb == "help" {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.add_output("Available commands:");
                    win.add_output("  ls - List files");
                    win.add_output("  cd <dir> - Change dir");
                    win.add_output("  cat <file> - Read file");
                    win.add_output("  cp <src> <dst> - Copy file (supports simple paths)");
                    win.add_output("  mv <src> <dst> - Move/rename file");
                    win.add_output("  disks - List USB/NVMe/HDD BlockIO devices");
                    win.add_output("  vols - List mountable FAT32 volumes");
                    win.add_output("  mount <n> - Mount FAT32 from 'disks' index");
                    win.add_output("  unmount - Unmount active volume");
                    win.add_output("  cpdev <src_dev> <src_path> <dst_dev> <dst_path> - Copy file between devices");
                    win.add_output("  net - Show transport/IP/failover status");
                    win.add_output("  net dhcp - Request dynamic IP via DHCP");
                    win.add_output("  net static - Apply default static IP");
                    win.add_output("  net static <ip> <prefijo> <gateway> - Apply custom static IP");
                    win.add_output("  net mode - Show current IP mode");
                    win.add_output("  net https <on|off|status> - HTTPS compatibility");
                    win.add_output("  net diag - Dump Intel Ethernet RX/TX registers");
                    win.add_output("  wifi - Show WiFi status");
                    win.add_output("  wifi scan - Scan WiFi networks");
                    win.add_output("  wifi connect <ssid> <clave> - Save profile/connect");
                    win.add_output("  wifi disconnect - Disconnect WiFi");
                    win.add_output("  wifi failover <ethernet|wifi|status> - Auto priority");
                    win.add_output("  fetch <url> [file_8_3] - Download file from network");
                    win.add_output("  web backend <builtin|vaev|webkit|cef|status> - Browser renderer");
                    win.add_output("  web vaev status - Embedded Vaev bridge diagnostics");
                    win.add_output("  web vaev input <click x y|scroll d|key K|text T|back|forward|reload>");
                    win.add_output("  web native <on|off|status> - Native DOM/layout/raster engine");
                    win.add_output("  web webkit <status|endpoint|ping|open|frame|input> - Host WebKit bridge");
                    win.add_output("  wry ... - alias de web webkit");
                    win.add_output("  mem - Show memory statistics");
                    win.add_output("  install [--autoport] <package.rpx|package.zip|package.tar|package.tar.gz|package.deb|setup.exe> [app_id] - Install package");
                    win.add_output("  entry <archivo> [app_id] - Generic installer entry point");
                    win.add_output("  linux inspect <elf> | linux run <elf> [args...] | linux runreal <elf> [args...] | linux runrealx <elf> [args...] | linux launch <elf> [args...] | linux launchmeta [--strict] <elf> | linux transfer <on|off|status> | linux runtime <quick|deep|status> | linux proc <start|startm|status|step|stop> | linux runloop <start|startx|startm|startmx|status|step|stop> | linux bridge <open|close|status|test>");
                    win.add_output("  host newlib porting - scripts/newlib_port.sh (scaffold/build/doctor)");
                    win.add_output("  ruby -e <code> | ruby <file.rb> - Ruby subset runtime");
                    win.add_output("  runapp <layout.rml> - Open .RML app in App Runner");
                    win.add_output("  clear - Clear screen");
                    win.add_output("  help - Show this help");
                    win.add_output("  doom - Launch external UEFI Doom image");
                    win.add_output("  shell - Launch external UEFI Shell image");
                }
                return;
            } else if verb == "clear" {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.output_lines.clear();
                    win.render_terminal();
                }
                return;
            } else {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    win.add_output("Filesystem not ready. Type 'help' for info.");
                }
                return;
            }
        }

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            if win.current_dir_cluster == 0 && fat.root_cluster != 0 {
                win.current_dir_cluster = fat.root_cluster;
                if win.current_path == "REDUX/" {
                    if let Some(label) = Self::volume_label_text(fat) {
                        win.current_path = alloc::format!("{}/", label);
                    }
                }
            }
        }

        let mut output = String::new();
        let mut special_handled = false;

        if verb == "help" {
            output = String::from(
                "Available commands:\n  ls - List files\n  cd <dir> - Change dir\n  cat <file> - Read file\n  cp <src> <dst> - Copy file\n  mv <src> <dst> - Move/rename file\n  disks - List USB/NVMe/HDD BlockIO devices\n  vols - List mountable FAT32 volumes\n  mount <n> - Mount FAT32 from 'disks' index\n  unmount - Unmount active volume\n  cpdev <src_dev> <src_path> <dst_dev> <dst_path> - Copy file between devices\n  net - Show transport/IP/failover status\n  net dhcp - Request dynamic IP via DHCP\n  net static - Apply default static IP\n  net static <ip> <prefijo> <gateway> - Apply custom static IP\n  net mode - Show current IP mode\n  net https <on|off|status> - HTTPS compatibility\n  net diag - Dump Intel Ethernet RX/TX registers\n  wifi - Show WiFi status\n  wifi scan - Scan WiFi networks\n  wifi connect <ssid> <clave> - Save profile/connect\n  wifi disconnect - Disconnect WiFi\n  wifi failover <ethernet|wifi|status> - Auto priority\n  fetch <url> [file_8_3] - Download file from network\n  web backend <builtin|vaev|webkit|cef|status> - Browser renderer\n  web vaev status - Embedded Vaev bridge diagnostics\n  web vaev input <click x y|scroll d|key K|text T|back|forward|reload>\n  web native <on|off|status> - Native DOM/layout/raster engine\n  web webkit <status|endpoint|ping|open|frame|input> - Host WebKit bridge\n  wry ... - alias de web webkit\n  mem - Show memory statistics\n  install [--autoport] <package.rpx|package.zip|package.tar|package.tar.gz|package.deb|setup.exe> [app_id] - Install package\n  entry <archivo> [app_id] - Generic installer entry point\n  linux inspect <elf> | linux run <elf> [args...] | linux runreal <elf> [args...] | linux runrealx <elf> [args...] | linux launch <elf> [args...] | linux launchmeta [--strict] <elf> | linux transfer <on|off|status> | linux runtime <quick|deep|status> | linux proc <start|startm|status|step|stop> | linux runloop <start|startx|startm|startmx|status|step|stop> | linux bridge <open|close|status|test>\n  host newlib porting - scripts/newlib_port.sh (scaffold/build/doctor)\n  ruby -e <code> | ruby <file.rb> - Ruby subset runtime\n  runapp <layout.rml> - Open .RML app in App Runner\n  clear - Clear screen\n  help - Show this help\n  doom - Launch external UEFI Doom image\n  shell - Launch external UEFI Shell image",
            );
        } else if verb == "clear" {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                win.output_lines.clear();
                win.render_terminal();
            }
            special_handled = true;
        } else if verb == "ls" {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                if let Ok(entries) = fat.read_dir_entries(win.current_dir_cluster) {
                    if win.current_dir_cluster == fat.root_cluster {
                        if let Some(label) = Self::volume_label_text(fat) {
                            output.push_str(&alloc::format!("[VOL] {}\n", label));
                        }
                    }

                    for entry in entries.iter() {
                        if entry.valid {
                            let type_tag = if entry.file_type == FileType::Directory {
                                "DIR"
                            } else {
                                "FILE"
                            };
                            output.push_str(&alloc::format!(
                                "[{}] {} ({} bytes)\n",
                                type_tag,
                                entry.full_name(),
                                entry.size
                            ));
                        }
                    }
                    if output.is_empty() {
                        output = String::from("(Empty directory)");
                    }
                } else {
                    output = String::from("Error reading directory. Use 'help' to check FS status.");
                }
            }
        } else if verb == "cd" {
            if arg_raw.is_empty() {
                output = String::from("Usage: cd <dir>");
            } else if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                let target = arg_raw;
                let target_lower = Self::ascii_lower(target);

                if let Some(label) = Self::volume_label_text(fat) {
                    if target_lower == Self::ascii_lower(&label) {
                        win.current_dir_cluster = fat.root_cluster;
                        win.current_path = alloc::format!("{}/", label);
                        return;
                    }
                }

                if target == ".." {
                    if win.current_dir_cluster == fat.root_cluster {
                        output = String::from("Already at root.");
                    } else if let Ok(entries) = fat.read_dir_entries(win.current_dir_cluster) {
                        let mut found = false;
                        for entry in entries.iter() {
                            if entry.matches_name("..") {
                                win.current_dir_cluster = if entry.cluster == 0 {
                                    fat.root_cluster
                                } else {
                                    entry.cluster
                                };
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            win.current_dir_cluster = fat.root_cluster;
                        }

                        if win.current_dir_cluster == fat.root_cluster {
                            if let Some(label) = Self::volume_label_text(fat) {
                                win.current_path = alloc::format!("{}/", label);
                            } else {
                                win.current_path = String::from("REDUX/");
                            }
                        } else {
                            if win.current_path.ends_with('/') {
                                win.current_path.pop();
                            }
                            if let Some(idx) = win.current_path.rfind('/') {
                                win.current_path.truncate(idx + 1);
                            } else {
                                win.current_path = String::from("REDUX/");
                            }
                        }
                    }
                } else if let Ok(entries) = fat.read_dir_entries(win.current_dir_cluster) {
                    let mut found = false;
                    for entry in entries.iter() {
                        if entry.valid && entry.file_type == FileType::Directory {
                            let entry_name = entry.full_name();
                            if entry.matches_name(target) {
                                win.current_dir_cluster = if entry.cluster == 0 {
                                    fat.root_cluster
                                } else {
                                    entry.cluster
                                };

                                if !win.current_path.ends_with('/') {
                                    win.current_path.push('/');
                                }
                                win.current_path.push_str(entry_name.as_str());
                                win.current_path.push('/');
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found {
                        output = String::from("Directory not found.");
                    }
                }
            }
        } else if verb == "cat" {
            if arg_raw.is_empty() {
                output = String::from("Usage: cat <file>");
            } else {
                let filename = arg_raw;
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    if let Ok(entries) = fat.read_dir_entries(win.current_dir_cluster) {
                        let mut found = false;
                        for entry in entries.iter() {
                            if entry.valid && entry.matches_name(filename) {
                                let target = (entry.size as usize).min(16 * 1024);
                                let mut buffer = Vec::new();
                                buffer.resize(target, 0);

                                match fat.read_file_sized(entry.cluster, target, &mut buffer) {
                                    Ok(len) => {
                                        output = String::from(
                                            core::str::from_utf8(&buffer[0..len])
                                                .unwrap_or("<binary content>"),
                                        );
                                        if (entry.size as usize) > target {
                                            output.push_str("\n[output truncated]");
                                        }
                                    }
                                    Err(_) => {
                                        output = String::from("Error reading file.");
                                    }
                                }
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            output = String::from("File not found.");
                        }
                    }
                }
            }
        } else if verb == "cp" || verb == "mv" {
            let do_move = verb == "mv";
            if arg_raw.is_empty() {
                output = if do_move {
                    String::from("Usage: mv <origen> <destino>")
                } else {
                    String::from("Usage: cp <origen> <destino>")
                };
            } else {
                let mut args = arg_raw.split_whitespace();
                let src_arg = args.next().unwrap_or("");
                let dst_arg = args.next().unwrap_or("");
                if src_arg.is_empty() || dst_arg.is_empty() || args.next().is_some() {
                    output = if do_move {
                        String::from("Usage: mv <origen> <destino>")
                    } else {
                        String::from("Usage: cp <origen> <destino>")
                    };
                } else if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    let current_cluster = if win.current_dir_cluster == 0 {
                        fat.root_cluster
                    } else {
                        win.current_dir_cluster
                    };

                    let (src_dir, src_leaf) = match Self::resolve_terminal_parent_and_leaf(
                        fat,
                        current_cluster,
                        src_arg,
                    ) {
                        Ok(v) => v,
                        Err(err) => {
                            output = alloc::format!("CP/MV error (origen): {}", err);
                            (0, String::new())
                        }
                    };
                    if !output.is_empty() {
                        // keep the first error
                    } else {
                        let (dst_dir, dst_leaf) = match Self::resolve_terminal_parent_and_leaf(
                            fat,
                            current_cluster,
                            dst_arg,
                        ) {
                            Ok(v) => v,
                            Err(err) => {
                                output = alloc::format!("CP/MV error (destino): {}", err);
                                (0, String::new())
                            }
                        };

                        if output.is_empty() {
                            if src_dir == dst_dir && src_leaf.eq_ignore_ascii_case(dst_leaf.as_str()) {
                                output = String::from("CP/MV: origen y destino son iguales.");
                            } else {
                                match fat.read_dir_entries(src_dir) {
                                    Ok(entries) => {
                                        let mut src_entry: Option<crate::fs::DirEntry> = None;
                                        for entry in entries.iter() {
                                            if !entry.valid || entry.file_type != FileType::File {
                                                continue;
                                            }
                                            if entry.matches_name(src_leaf.as_str())
                                                || entry.full_name().eq_ignore_ascii_case(src_leaf.as_str())
                                            {
                                                src_entry = Some(*entry);
                                                break;
                                            }
                                        }

                                        if let Some(source) = src_entry {
                                            if source.size as usize > COPY_MAX_FILE_BYTES {
                                                output = alloc::format!(
                                                    "CP/MV: archivo demasiado grande (max {} bytes).",
                                                    COPY_MAX_FILE_BYTES
                                                );
                                            } else {
                                                let mut raw = Vec::new();
                                                raw.resize(source.size as usize, 0);
                                                match fat.read_file_sized(
                                                    source.cluster,
                                                    source.size as usize,
                                                    &mut raw,
                                                ) {
                                                    Ok(len) => {
                                                        raw.truncate(len);
                                                        match fat.write_text_file_in_dir(
                                                            dst_dir,
                                                            dst_leaf.as_str(),
                                                            raw.as_slice(),
                                                        ) {
                                                            Ok(()) => {
                                                                if do_move {
                                                                    match fat.delete_file_in_dir(
                                                                        src_dir,
                                                                        src_leaf.as_str(),
                                                                    ) {
                                                                        Ok(()) => {
                                                                            output = alloc::format!(
                                                                                "Moved {} -> {}",
                                                                                src_arg, dst_arg
                                                                            );
                                                                        }
                                                                        Err(err) => {
                                                                            output = alloc::format!(
                                                                                "MV warning: copiado pero no se pudo borrar origen ({}).",
                                                                                err
                                                                            );
                                                                        }
                                                                    }
                                                                } else {
                                                                    output = alloc::format!(
                                                                        "Copied {} -> {}",
                                                                        src_arg, dst_arg
                                                                    );
                                                                }
                                                            }
                                                            Err(err) => {
                                                                output = alloc::format!(
                                                                    "CP/MV error writing destino: {}",
                                                                    err
                                                                );
                                                            }
                                                        }
                                                    }
                                                    Err(err) => {
                                                        output = alloc::format!(
                                                            "CP/MV error leyendo origen: {}",
                                                            err
                                                        );
                                                    }
                                                }
                                            }
                                        } else {
                                            output = String::from("CP/MV: archivo origen no encontrado.");
                                        }
                                    }
                                    Err(err) => {
                                        output = alloc::format!("CP/MV error leyendo directorio: {}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            output = alloc::format!("Unknown command: {}", trimmed);
        }

        if !special_handled {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                for line in output.lines() {
                    win.add_output(line);
                }
                win.render_terminal();
            }
        }
    }
}
