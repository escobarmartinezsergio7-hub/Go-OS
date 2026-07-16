use alloc::string::String;
use alloc::vec::Vec;

use super::{Color, Rect, SpecialKey};

pub const TITLE_BAR_H: i32 = 22;
pub const WINDOW_TITLE_BAR_H: i32 = TITLE_BAR_H;
pub const WINDOW_RESIZE_GRIP: i32 = 16;
const TERMINAL_TOP_PADDING: i32 = 10;
const TERMINAL_BOTTOM_PADDING: i32 = 10;
const TERMINAL_LINE_HEIGHT: i32 = 12;
const TERMINAL_HISTORY_MAX_LINES: usize = 4096;
const TERMINAL_TEXT_X: usize = 10;
const TERMINAL_CHAR_W: usize = 6;

pub const EXPLORER_TOP_H: i32 = 30;
const EXPLORER_STATUS_H: i32 = 58;
const EXPLORER_CELL_W: i32 = 108;
const EXPLORER_CELL_H: i32 = 98;
const EXPLORER_GAP_X: i32 = 14;
const EXPLORER_GAP_Y: i32 = 16;
const EXPLORER_MARGIN_X: i32 = 16;
const EXPLORER_MARGIN_Y: i32 = 38;
const EXPLORER_SEARCH_FIELD_MIN_W: i32 = 96;
const EXPLORER_SEARCH_FIELD_MAX_W: i32 = 220;
const EXPLORER_SEARCH_BUTTON_W: i32 = 62;

const NOTEPAD_TOP_H: i32 = 36;
const NOTEPAD_STATUS_H: i32 = 28;
const SEARCH_TOP_H: i32 = 44;
const SEARCH_STATUS_H: i32 = 24;
const SEARCH_RESULT_ROW_H: i32 = 28;

const BROWSER_TOP_H: i32 = 64;
const BROWSER_STATUS_H: i32 = 24;
const IMAGE_VIEWER_TOP_H: i32 = 52;
const IMAGE_VIEWER_STATUS_H: i32 = 28;
const APP_RUNNER_TOP_H: i32 = 52;
const APP_RUNNER_STATUS_H: i32 = 28;
const IDE_STUDIO_TOP_H: i32 = 62;
const IDE_STUDIO_STATUS_H: i32 = 40;
const IDE_STUDIO_EDITOR_CHAR_W: i32 = 6;
const IDE_STUDIO_EDITOR_LINE_H: i32 = 9;
const IDE_STUDIO_EDITOR_TEXT_X: i32 = 6;
const IDE_STUDIO_EDITOR_TEXT_Y: i32 = 16;
const DOOM_LAUNCHER_TOP_H: i32 = 52;
const DOOM_LAUNCHER_STATUS_H: i32 = 28;
const DOOM_NATIVE_FP_SHIFT: i32 = 10;
const DOOM_NATIVE_FP_ONE: i32 = 1 << DOOM_NATIVE_FP_SHIFT;
const DOOM_NATIVE_DIR_COUNT: usize = 64;
const DOOM_NATIVE_ANGLE_SUBDIV: i32 = 16;
const DOOM_NATIVE_ANGLE_UNITS: i32 = (DOOM_NATIVE_DIR_COUNT as i32) * DOOM_NATIVE_ANGLE_SUBDIV;
const DOOM_NATIVE_FOV_UNITS: i32 = 176;
const DOOM_NATIVE_RAY_STEP_FP: i32 = 48;
const DOOM_NATIVE_MAX_RAY_STEPS: i32 = 256;
const DOOM_NATIVE_MOVE_STEP_FP: i32 = 116;
const DOOM_NATIVE_STRAFE_STEP_FP: i32 = 92;
const DOOM_NATIVE_TURN_UNITS: i32 = 8;
const DOOM_NATIVE_COLLISION_RADIUS_FP: i32 = DOOM_NATIVE_FP_ONE / 5;
const DOOM_NATIVE_MAP_W: usize = 16;
const DOOM_NATIVE_MAP_H: usize = 16;
const DOOM_NATIVE_ENEMY_COUNT: usize = 6;
const DOOM_NATIVE_ENEMY_POS_CELLS: [(i32, i32); DOOM_NATIVE_ENEMY_COUNT] = [
    (6, 3),
    (11, 4),
    (4, 7),
    (12, 8),
    (6, 11),
    (10, 13),
];
const DOOM_NATIVE_MAP: [&[u8; DOOM_NATIVE_MAP_W]; DOOM_NATIVE_MAP_H] = [
    b"################",
    b"#....B.....T...#",
    b"#..##..BB......#",
    b"#..T......B....#",
    b"#..####..###...#",
    b"#...B......T...#",
    b"#..##..TT......#",
    b"#......###..B..#",
    b"#..B...........#",
    b"#..###..####...#",
    b"#....T.........#",
    b"#....##....B...#",
    b"#..B......T....#",
    b"#..####....##..#",
    b"#.....B........#",
    b"################",
];
const DOOM_NATIVE_DIR_TABLE: [(i16, i16); DOOM_NATIVE_DIR_COUNT] = [
    (256, 0),      // 0
    (255, 25),     // 1
    (251, 50),     // 2
    (245, 74),     // 3
    (237, 98),     // 4
    (226, 121),    // 5
    (213, 142),    // 6
    (198, 162),    // 7
    (181, 181),    // 8
    (162, 198),    // 9
    (142, 213),    // 10
    (121, 226),    // 11
    (98, 237),     // 12
    (74, 245),     // 13
    (50, 251),     // 14
    (25, 255),     // 15
    (0, 256),      // 16
    (-25, 255),    // 17
    (-50, 251),    // 18
    (-74, 245),    // 19
    (-98, 237),    // 20
    (-121, 226),   // 21
    (-142, 213),   // 22
    (-162, 198),   // 23
    (-181, 181),   // 24
    (-198, 162),   // 25
    (-213, 142),   // 26
    (-226, 121),   // 27
    (-237, 98),    // 28
    (-245, 74),    // 29
    (-251, 50),    // 30
    (-255, 25),    // 31
    (-256, 0),     // 32
    (-255, -25),   // 33
    (-251, -50),   // 34
    (-245, -74),   // 35
    (-237, -98),   // 36
    (-226, -121),  // 37
    (-213, -142),  // 38
    (-198, -162),  // 39
    (-181, -181),  // 40
    (-162, -198),  // 41
    (-142, -213),  // 42
    (-121, -226),  // 43
    (-98, -237),   // 44
    (-74, -245),   // 45
    (-50, -251),   // 46
    (-25, -255),   // 47
    (0, -256),     // 48
    (25, -255),    // 49
    (50, -251),    // 50
    (74, -245),    // 51
    (98, -237),    // 52
    (121, -226),   // 53
    (142, -213),   // 54
    (162, -198),   // 55
    (181, -181),   // 56
    (198, -162),   // 57
    (213, -142),   // 58
    (226, -121),   // 59
    (237, -98),    // 60
    (245, -74),    // 61
    (251, -50),    // 62
    (255, -25),    // 63
];
const LINUX_BRIDGE_TOP_H: i32 = 52;
const LINUX_BRIDGE_STATUS_H: i32 = 28;
const IDE_STUDIO_MAX_TEXT_BYTES: usize = 128 * 1024;
const IDE_STUDIO_UNDO_STACK_LIMIT: usize = 256;
const TASK_MGR_HEADER_H: i32 = 42;
const TASK_MGR_FOOTER_H: i32 = 68;
const TASK_MGR_ROW_H: i32 = 18;

#[derive(Copy, Clone, PartialEq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
    Closed,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum WindowKind {
    Terminal,
    Explorer,
    Notepad,
    Search,
    Browser,
    ImageViewer,
    AppRunner,
    IdeStudio,
    DoomLauncher,
    LinuxBridge,
    Settings,
    MediaPlayer,
    WifiManager,
    TaskManager,
    VideoPlayer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExplorerItemKind {
    ShortcutDesktop,
    ShortcutDownloads,
    ShortcutDocuments,
    ShortcutImages,
    ShortcutVideos,
    ShortcutUsb,
    ShortcutVolume,
    ShortcutRecycleBin,
    ShortcutReduxStudio,
    Home,
    Up,
    Directory,
    File,
    FileExecutable,
    FileImage,
    FileAudio,
    FileVideo,
    FileArchive,
    FileCode,
    FileText,
}

#[derive(Clone)]
pub struct ExplorerItem {
    pub label: String,
    pub kind: ExplorerItemKind,
    pub cluster: u32,
    pub size: u32,
    pub create_date: u16,
    pub create_time: u16,
    pub write_date: u16,
    pub write_time: u16,
}

impl ExplorerItem {
    pub fn new(label: &str, kind: ExplorerItemKind, cluster: u32, size: u32) -> Self {
        Self {
            label: String::from(label),
            kind,
            cluster,
            size,
            create_date: 0,
            create_time: 0,
            write_date: 0,
            write_time: 0,
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(
            self.kind,
            ExplorerItemKind::File
                | ExplorerItemKind::FileExecutable
                | ExplorerItemKind::FileImage
                | ExplorerItemKind::FileAudio
                | ExplorerItemKind::FileVideo
                | ExplorerItemKind::FileArchive
                | ExplorerItemKind::FileCode
                | ExplorerItemKind::FileText
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotepadClickAction {
    New,
    Save,
    Delete,
    FilenameField,
    EditorArea,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchClickAction {
    QueryField,
    SearchButton,
    Result(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExplorerSearchClickAction {
    QueryField,
    SearchButton,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TaskManagerClickAction {
    Select(usize),
    CancelInstall,
    CancelFs,
    CancelPaste,
    CancelAll,
}

#[derive(Clone)]
pub struct SearchResultEntry {
    pub label: String,
    pub subtitle: String,
    pub command: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IdeStudioClickAction {
    TabRust,
    TabRuby,
    TabRml,
    TabRdx,
    TabDocs,
    ViewInput,
    ViewGo,
    Preview,
    Link,
    RubyRun,
    RustCheck,
    Load,
    Build,
    Export,
    Restart,
    PreviewButton,
    EditorArea,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PreviewElementKind {
    Header,
    Text,
    Button,
}

#[derive(Clone)]
pub struct PreviewElement {
    pub kind: PreviewElementKind,
    pub text: String,
    pub id: String,
    pub color: u32,
    pub size: i32,
    pub margin_top: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub margin_right: i32,
}

pub struct WindowControls {
    pub close_btn: Rect,
    pub minimize_btn: Rect,
    pub maximize_btn: Rect,
}

impl WindowControls {
    pub fn new(win_x: i32, win_y: i32, win_width: u32) -> Self {
        let btn_y = win_y + 3;
        Self {
            close_btn: Rect::new(win_x + win_width as i32 - 18, btn_y, 16, 16),
            minimize_btn: Rect::new(win_x + win_width as i32 - 58, btn_y, 16, 16),
            maximize_btn: Rect::new(win_x + win_width as i32 - 38, btn_y, 16, 16),
        }
    }
}

#[derive(Clone)]
struct IdeEditorSnapshot {
    tab: u8,
    text: String,
    cursor: usize,
    sel_start: usize,
    sel_end: usize,
}

pub struct Window {
    pub id: usize,
    pub desktop_id: u8,
    pub rect: Rect,
    pub title: String,
    pub buffer: Vec<u32>,
    pub state: WindowState,
    pub kind: WindowKind,
    pub controls: WindowControls,
    pub saved_rect: Rect,

    // Terminal state
    pub input_buffer: String,
    pub output_lines: Vec<String>,
    pub terminal_scroll: usize,
    pub cursor_x: usize,
    pub current_dir_cluster: u32,
    pub current_path: String,

    // Explorer state
    pub explorer_items: Vec<ExplorerItem>,
    pub explorer_current_cluster: u32,
    pub explorer_device_index: Option<usize>,
    pub explorer_path: String,
    pub explorer_status: String,
    pub explorer_preview_lines: Vec<String>,
    pub explorer_scroll: usize,
    pub explorer_search_query: String,
    pub explorer_search_input_active: bool,
    pub explorer_search_active: bool,
    pub explorer_search_source_items: Vec<ExplorerItem>,
    pub explorer_side_panel_open: bool,
    pub explorer_side_panel_item: Option<ExplorerItem>,
    pub explorer_side_panel_dir_size: Option<u64>,

    // Notepad state
    pub notepad_file_name: String,
    pub notepad_text: String,
    pub notepad_status: String,
    pub notepad_dir_cluster: u32,
    pub notepad_dir_path: String,
    pub notepad_edit_name: bool,

    // Search state
    pub search_query: String,
    pub search_status: String,
    pub search_results: Vec<SearchResultEntry>,
    pub search_input_active: bool,

    // Browser state
    pub browser_url: String,
    pub browser_status: String,
    pub browser_content_lines: Vec<String>,
    pub browser_scroll: usize,
    pub browser_surface_source: String,
    pub browser_surface_width: u32,
    pub browser_surface_height: u32,
    pub browser_surface_pixels: Vec<u32>,

    // Image Viewer state
    pub image_viewer_file_name: String,
    pub image_viewer_status: String,
    pub image_viewer_width: u32,
    pub image_viewer_height: u32,
    pub image_viewer_pixels: Vec<u32>,

    // App Runner state
    pub app_runner_source_file: String,
    pub app_runner_rml_source: String,
    pub app_runner_active_view_id: String,
    pub app_runner_theme: String,
    pub app_runner_header_text: String,
    pub app_runner_body_text: String,
    pub app_runner_button_label: String,
    pub app_runner_button_id: String,
    pub app_runner_rdx_source: String,
    pub app_runner_rust_source: String,
    pub app_runner_status: String,
    pub app_runner_background_color: u32,
    pub app_runner_header_color: u32,
    pub app_runner_body_color: u32,
    pub app_runner_button_color: u32,
    pub app_runner_padding: i32,
    pub app_runner_elements: Vec<PreviewElement>,
    pub app_runner_button_targets: Vec<(Rect, String)>,
    pub app_runner_button_rect_cached: Rect,
    pub app_runner_button_rect_valid: bool,

    // Video Player state
    pub video_player_file_name: String,
    pub video_player_file_cluster: u32,
    pub video_player_file_size: u32,
    pub video_player_width: u32,
    pub video_player_height: u32,
    pub video_player_fps: u32,
    pub video_player_current_frame: usize,
    pub video_player_data_offset: usize,
    pub video_player_last_tick: u64,
    pub video_player_status: String,
    pub video_player_frame_buf: Vec<u8>,
    pub video_player_cached_payload: Vec<u8>,

    // Redux Studio state
    pub ide_project_name: String,
    pub ide_active_tab: u8,
    pub ide_rust_text: String,
    pub ide_ruby_text: String,
    pub ide_rml_text: String,
    pub ide_rdx_text: String,
    ide_last_export_rust: String,
    ide_last_export_ruby: String,
    ide_last_export_rml: String,
    ide_last_export_rdx: String,
    pub ide_docs_text: String,
    pub ide_cursor_rust: usize,
    pub ide_cursor_ruby: usize,
    pub ide_cursor_rml: usize,
    pub ide_cursor_rdx: usize,
    pub ide_cursor_docs: usize,
    pub ide_sel_start_rust: usize,
    pub ide_sel_end_rust: usize,
    pub ide_sel_start_ruby: usize,
    pub ide_sel_end_ruby: usize,
    pub ide_sel_start_rml: usize,
    pub ide_sel_end_rml: usize,
    pub ide_sel_start_rdx: usize,
    pub ide_sel_end_rdx: usize,
    pub ide_sel_start_docs: usize,
    pub ide_sel_end_docs: usize,
    ide_undo_stack: Vec<IdeEditorSnapshot>,
    ide_redo_stack: Vec<IdeEditorSnapshot>,
    pub ide_status: String,
    pub ide_preview_event: String,
    pub ide_preview_active_view_id: String,
    pub ide_preview_view_input: String,
    pub ide_preview_view_input_active: bool,
    pub ide_preview_theme: String,
    pub ide_preview_header_text: String,
    pub ide_preview_body_text: String,
    pub ide_preview_button_label: String,
    pub ide_preview_button_id: String,
    pub ide_preview_background_color: u32,
    pub ide_preview_header_color: u32,
    pub ide_preview_body_color: u32,
    pub ide_preview_button_color: u32,
    pub ide_preview_padding: i32,
    pub ide_preview_elements: Vec<PreviewElement>,
    pub ide_preview_button_targets: Vec<(Rect, String)>,
    pub ide_preview_button_rect_cached: Rect,
    pub ide_preview_button_rect_valid: bool,

    // CPP-DOOM Launcher state
    pub doom_status: String,
    pub doom_native_running: bool,
    pub doom_native_player_x_fp: i32,
    pub doom_native_player_y_fp: i32,
    pub doom_native_angle_units: i16,
    pub doom_native_steps: u32,
    pub doom_native_shots: u32,
    pub doom_native_kills: u16,
    pub doom_native_enemy_alive_mask: u16,
    pub doom_native_flash_ticks: u8,

    // Linux Bridge state
    pub linux_bridge_status: String,
    pub linux_bridge_source: String,
    pub linux_bridge_width: u32,
    pub linux_bridge_height: u32,
    pub linux_bridge_pixels: Vec<u32>,

    // WiFi Manager state
    pub wifi_scan_entries: Vec<(String, i8, u8, bool)>, // (ssid, rssi, channel, secure)
    pub wifi_selected_index: usize,
    pub wifi_password_input: String,
    pub wifi_password_editing: bool,
    pub wifi_scroll: usize,
    pub wifi_status_msg: String,
    pub wifi_mode_active: bool,

    // Task Manager state
    pub task_manager_lines: Vec<String>,
    pub task_manager_scroll: usize,
    pub task_manager_selected: Option<usize>,
    pub task_manager_status: String,
}

impl Window {
    fn new_base(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let buffer_size = (width * height) as usize;
        Self {
            id,
            desktop_id: 1,
            rect: Rect::new(x, y, width, height),
            title: String::from(title),
            buffer: alloc::vec![0xFFFFFFFF; buffer_size],
            state: WindowState::Normal,
            kind: WindowKind::Terminal,
            controls: WindowControls::new(x, y, width),
            saved_rect: Rect::new(x, y, width, height),

            input_buffer: String::new(),
            output_lines: alloc::vec![],
            terminal_scroll: 0,
            cursor_x: 0,
            current_dir_cluster: unsafe { crate::fat32::GLOBAL_FAT.root_cluster },
            current_path: String::from("REDUX/"),

            explorer_items: alloc::vec![],
            explorer_current_cluster: 0,
            explorer_device_index: None,
            explorer_path: String::from("Quick Access"),
            explorer_status: String::new(),
            explorer_preview_lines: alloc::vec![],
            explorer_scroll: 0,
            explorer_search_query: String::new(),
            explorer_search_input_active: false,
            explorer_search_active: false,
            explorer_search_source_items: alloc::vec![],
            explorer_side_panel_open: false,
            explorer_side_panel_item: None,
            explorer_side_panel_dir_size: None,

            notepad_file_name: String::from("NOTE.TXT"),
            notepad_text: String::new(),
            notepad_status: String::from("Ready."),
            notepad_dir_cluster: unsafe { crate::fat32::GLOBAL_FAT.root_cluster },
            notepad_dir_path: String::from("/"),
            notepad_edit_name: false,

            search_query: String::new(),
            search_status: String::from("Escribe y pulsa Buscar."),
            search_results: Vec::new(),
            search_input_active: true,

            browser_url: String::from("redux://welcome"),
            browser_status: String::from("Ready"),
            browser_content_lines: alloc::vec![],
            browser_scroll: 0,
            browser_surface_source: String::new(),
            browser_surface_width: 0,
            browser_surface_height: 0,
            browser_surface_pixels: alloc::vec![],

            image_viewer_file_name: String::new(),
            image_viewer_status: String::from("No image loaded."),
            image_viewer_width: 0,
            image_viewer_height: 0,
            image_viewer_pixels: alloc::vec![],

            app_runner_source_file: String::new(),
            app_runner_rml_source: String::new(),
            app_runner_active_view_id: String::new(),
            app_runner_theme: String::from("light"),
            app_runner_header_text: String::from("App"),
            app_runner_body_text: String::from("No layout loaded."),
            app_runner_button_label: String::from("Run"),
            app_runner_button_id: String::new(),
            app_runner_rdx_source: String::new(),
            app_runner_rust_source: String::new(),
            app_runner_status: String::from("Ready."),
            app_runner_background_color: 0xF4F8FC,
            app_runner_header_color: 0x1F4D78,
            app_runner_body_color: 0x203345,
            app_runner_button_color: 0x2D89D6,
            app_runner_padding: 10,
            app_runner_elements: Self::ide_default_preview_elements(
                "App",
                "No layout loaded.",
                "Run",
                "action",
                0x1F4D78,
                0x203345,
                0x2D89D6,
            ),
            app_runner_button_targets: alloc::vec![],
            app_runner_button_rect_cached: Rect::new(0, 0, 0, 0),
            app_runner_button_rect_valid: false,
            video_player_file_name: String::new(),
            video_player_file_cluster: 0,
            video_player_file_size: 0,
            video_player_width: 0,
            video_player_height: 0,
            video_player_fps: 60,
            video_player_current_frame: 0,
            video_player_data_offset: 16,
            video_player_last_tick: 0,
            video_player_status: String::new(),
            video_player_frame_buf: alloc::vec![],
            video_player_cached_payload: alloc::vec![],
            ide_project_name: String::from("IDEAPP"),
            ide_active_tab: 2,
            ide_rust_text: String::from("fn main() {\n  // TODO: Rust code\n}\n"),
            ide_ruby_text: String::from("puts \"Hello from Redux IDE\"\n"),
            ide_rml_text: String::from(
                "<App title=\"IDE App\" theme=\"dark\">\n  <View padding=\"16\" background=\"#0F172A\">\n    <Header text=\"IDE App\" color=\"#22D3EE\" size=\"24\" />\n    <Text id=\"status\" value=\"Edit RML and click PREVIEW.\" color=\"#E5E7EB\" />\n    <Button id=\"action\" label=\"Run\" color=\"#0EA5E9\" />\n  </View>\n</App>\n",
            ),
            ide_rdx_text: String::from(
                "fn on_start() {\n  log(\"IDE app started\");\n}\n\nfn on_click_action() {\n  puts \"Action clicked\";\n}\n",
            ),
            ide_last_export_rust: String::from("fn main() {\n  // TODO: Rust code\n}\n"),
            ide_last_export_ruby: String::from("puts \"Hello from Redux IDE\"\n"),
            ide_last_export_rml: String::from(
                "<App title=\"IDE App\" theme=\"dark\">\n  <View padding=\"16\" background=\"#0F172A\">\n    <Header text=\"IDE App\" color=\"#22D3EE\" size=\"24\" />\n    <Text id=\"status\" value=\"Edit RML and click PREVIEW.\" color=\"#E5E7EB\" />\n    <Button id=\"action\" label=\"Run\" color=\"#0EA5E9\" />\n  </View>\n</App>\n",
            ),
            ide_last_export_rdx: String::from(
                "fn on_start() {\n  log(\"IDE app started\");\n}\n\nfn on_click_action() {\n  puts \"Action clicked\";\n}\n",
            ),
            ide_docs_text: String::from(
                "RDX + RML Spec (Redux Studio)\n\n\
Version: preview runtime in Redux IDE\n\n\
1) Execution model\n\
- RML defines the preview UI tree.\n\
- RDX defines lifecycle and click handlers.\n\
- LINK scans <Button id=\"...\"> and binds each to fn on_click_<id>().\n\
- If callback is missing, click is detected but no handler body is executed.\n\n\
2) RDX syntax (implemented subset)\n\
Program:\n\
  { function }\n\
Function:\n\
  fn <ident>() { <statements> }\n\n\
Common handlers:\n\
- fn on_start() { ... }\n\
- fn on_click_<button_id>() { ... }\n\n\
Statements commonly used in IDE:\n\
- puts \"text\"\n\
- log(\"text\")\n\
- SET_TEXT(\"target_id\", \"value\")\n\
- delay(0.5) / set_delay(0.5) / delay_ms(500)\n\
- restart()   (reinicia runtime/variables)\n\n\
- string(expr), int(expr), float(expr), double(expr)\n\n\
Notes:\n\
- Callback name match is exact.\n\
- LINK auto-creates missing on_click_<id>() blocks.\n\
- Existing callback bodies are preserved.\n\n\
3) RML syntax\n\
Root:\n\
<App title=\"MyApp\" theme=\"dark\">  o  <App title=\"MyApp\" theme=\"light\">  o  <App title=\"MyApp\" theme.light>\n\
  <View padding=\"16\" background=\"#0F172A\">\n\
    ...elements...\n\
  </View>\n\
</App>\n\n\
Multiple views in same RML (example):\n\
<App title=\"Demo\" theme=\"dark\">\n\
  <View id=\"home\">\n\
    <Header text=\"Home\"/>\n\
    <Button id=\"to_settings\" label=\"Ir Settings\"/>\n\
  </View>\n\
  <View id=\"settings\">\n\
    <Header text=\"Settings\"/>\n\
    <Button id=\"to_home\" label=\"Ir Home\"/>\n\
  </View>\n\
</App>\n\n\
Preview view switch (manual):\n\
1. Escribe el id en el campo \"VIEW ID\" (arriba del preview).\n\
2. Clic en \"GO\" (o Enter).\n\
3. El preview renderiza esa vista dentro de la misma app/IDE.\n\
4. Si no existe, muestra \"No existe view\" y la lista de ids disponibles.\n\n\
RDX methods for view navigation:\n\
- set_view(\"settings\")\n\
- go_to_view(\"settings\")\n\
- aliases compatibles: setview(\"settings\"), goto_view(\"settings\")\n\n\
RDX callback example:\n\
fn on_click_to_settings() {\n\
  set_view(\"settings\");\n\
}\n\
\n\
fn on_click_to_home() {\n\
  go_to_view(\"home\");\n\
}\n\n\
Elements:\n\
- Header: <Header id=\"title\" text=\"Title\" color=\"#22D3EE\" size=\"24\" />\n\
- Text:   <Text id=\"status\" value=\"Ready\" color=\"#E5E7EB\" size=\"14\" />\n\
- Label:  <Label id=\"status2\" text=\"Ready\" color=\"#E5E7EB\" size=\"14\" />\n\
- Button: <Button id=\"action\" label=\"Run\" color=\"#0EA5E9\" size=\"14\" />\n\n\
Spacing attrs:\n\
- margin, margin_top, margin_bottom, margin_left, margin_right\n\
- aliases: margin-top, margin-bottom, margin-left, margin-right\n\n\
4) ScrollView syntax\n\
Container:\n\
<ScrollView id=\"feed\" height=\"140\" background=\"#101A2A\" padding=\"8\">\n\
  <Text id=\"line1\" value=\"Item 1\" />\n\
  <Text id=\"line2\" value=\"Item 2\" />\n\
</ScrollView>\n\n\
ScrollView attrs:\n\
- id (optional)\n\
- height (recommended to define viewport)\n\
- background, padding\n\
- margin / margin_* / margin-*\n\n\
5) SET_TEXT semantics\n\
Accepted forms:\n\
- SET_TEXT(\"id\", \"text\")\n\
- set_text(\"id\", \"text\")\n\
- settext(\"id\", \"text\")\n\n\
Parser details:\n\
- Name is case-insensitive.\n\
- Arg1 must be quoted id (single, double, or smart quotes).\n\
- Arg2 may be quoted text or literal token.\n\
- Target id match is case-insensitive.\n\n\
Effect by element type:\n\
- Header: updates displayed header text.\n\
- Text/Label: updates displayed text value.\n\
- Button: updates displayed label text.\n\
- SET_TEXT does not rename id.\n\n\
6) Practical flow\n\
1. Edit RML and ensure button has id.\n\
2. Click LINK to generate/attach on_click_<id>().\n\
3. In RDX callback, call SET_TEXT(\"status\", \"Hello\").\n\
4. Click PREVIEW, then click the button in preview.\n\n\
7) Minimal example\n\
RML:\n\
  <Text id=\"STATUS2\" value=\"Init\" />\n\
  <Button id=\"CHTEXT\" label=\"Change\" />\n\n\
RDX:\n\
  fn on_click_CHTEXT() {\n\
    SET_TEXT(\"STATUS2\", \"HOLA\");\n\
  }\n\n\
8) Variables, condiciones y ciclos\n\
Variables:\n\
- let count = 0\n\
- var clicks = 0\n\
- let name = \"Go OS\"\n\
- let active = true\n\n\
Globales (top-level):\n\
- let count = 0  (fuera de fn)\n\
- se inicializa una vez y persiste entre clicks de la app/ventana\n\n\
Mutabilidad:\n\
- let = inmutable (no se reasigna)\n\
- var = mutable (si se reasigna)\n\
\n\
Condiciones:\n\
- if active { ... }\n\
- if count > 3 { ... } else { ... }\n\n\
	Ciclos:\n\
	- while count < 5 { ... }\n\
	- do { ... } while count < 5\n\
	- for (var i = 0; i < 5; i++) { ... }\n\
	- break  (solo dentro de while/do/for)\n\n\
Nota eventos:\n\
- Cada click ejecuta su callback y reevalua if/else/while con el estado actual.\n\
- En on_click_*, un while corre completo en ese click.\n\
- break detiene el ciclo actual (while/do/for) en ese click.\n\
- Con SET_TEXT en ciclos, el runtime aplica delay visual por paso (default 0ms).\n\
- Para subir 1 por click, usa count++ sin while.\n\n\
Ejemplo visible:\n\
  fn on_start() {\n\
    let count = 0;\n\
    while count < 3 {\n\
      puts \"Iter\";\n\
      count = count + 1;\n\
    }\n\
    if count == 3 {\n\
      SET_TEXT(\"STATUS2\", \"LISTO\");\n\
    } else {\n\
      SET_TEXT(\"STATUS2\", \"ERROR\");\n\
    }\n\
  }\n\n\
9) Parsers (runtime subset)\n\
- Parser function: fn <ident>() { ... }\n\
- Parser let: let <ident> = <expr>\n\
- Parser var: var <ident> = <expr>\n\
	- Parser if/else: if <expr> { ... } else { ... }\n\
	- Parser while: while <expr> { ... }\n\
- Parser do/while: do { ... } while (<expr>)\n\
- Parser for: for (<init>; <cond>; <step>) { ... }\n\
- Parser break: break\n\
- Parser calls: puts, log(...), SET_TEXT(...), set_text(...), settext(...), delay(...), set_delay(...), delay_ms(...), restart(), string(...), int(...), float(...), double(...)\n\
- Parser ids/tokens: case-insensitive for SET_TEXT target id match\n\n\
10) Rust UI/Web bridge (boton RUST)\n\
- Rust real (rustc/cargo crates) no embebido en IDE.\n\
- Runtime bridge soportado para UI/web:\n\
  - json_get(\"https://...\")\n\
  - set_text(\"id\", valor)\n\
  - let x = ... (valor literal, variable o json_get)\n\
  - acceso por campo JSON: objeto.campo, objeto.subcampo.campo, lista.0.campo, lista[0].campo\n\
  - callbacks por boton: fn on_click_<id>() (sin presionar boton RUST)\n\
\n\
Ejemplo:\n\
  fn on_start() {\n\
    let todo = json_get(\"https://jsonplaceholder.typicode.com/todos/1\");\n\
    set_text(\"status\", todo.title);\n\
    set_text(\"done\", todo.completed);\n\
  }\n\
\n\
Tab DOCS is read-only.\n"
            ),
            ide_cursor_rust: 0,
            ide_cursor_ruby: 0,
            ide_cursor_rml: 0,
            ide_cursor_rdx: 0,
            ide_cursor_docs: 0,
            ide_sel_start_rust: 0,
            ide_sel_end_rust: 0,
            ide_sel_start_ruby: 0,
            ide_sel_end_ruby: 0,
            ide_sel_start_rml: 0,
            ide_sel_end_rml: 0,
            ide_sel_start_rdx: 0,
            ide_sel_end_rdx: 0,
            ide_sel_start_docs: 0,
            ide_sel_end_docs: 0,
            ide_undo_stack: Vec::new(),
            ide_redo_stack: Vec::new(),
            ide_status: String::from(
                "IDE listo: edita, PREVIEW, LINK, RUBY, LOAD, INSTALL, EXPORT, RESTART.",
            ),
            ide_preview_event: String::from("Preview listo."),
            ide_preview_active_view_id: String::new(),
            ide_preview_view_input: String::new(),
            ide_preview_view_input_active: false,
            ide_preview_theme: String::from("dark"),
            ide_preview_header_text: String::from("IDE App"),
            ide_preview_body_text: String::from("Edit RML and click PREVIEW."),
            ide_preview_button_label: String::from("Run"),
            ide_preview_button_id: String::from("action"),
            ide_preview_background_color: 0x0F172A,
            ide_preview_header_color: 0x22D3EE,
            ide_preview_body_color: 0xE5E7EB,
            ide_preview_button_color: 0x0EA5E9,
            ide_preview_padding: 10,
            ide_preview_elements: alloc::vec![
                PreviewElement {
                    kind: PreviewElementKind::Header,
                    text: String::from("IDE App"),
                    id: String::new(),
                    color: 0x22D3EE,
                    size: 24,
                    margin_top: 0,
                    margin_bottom: 6,
                    margin_left: 0,
                    margin_right: 0,
                },
                PreviewElement {
                    kind: PreviewElementKind::Text,
                    text: String::from("Edit RML and click PREVIEW."),
                    id: String::new(),
                    color: 0xE5E7EB,
                    size: 14,
                    margin_top: 0,
                    margin_bottom: 10,
                    margin_left: 0,
                    margin_right: 0,
                },
                PreviewElement {
                    kind: PreviewElementKind::Button,
                    text: String::from("Run"),
                    id: String::from("action"),
                    color: 0x0EA5E9,
                    size: 14,
                    margin_top: 0,
                    margin_bottom: 0,
                    margin_left: 0,
                    margin_right: 0,
                },
            ],
            ide_preview_button_targets: Vec::new(),
            ide_preview_button_rect_cached: Rect::new(0, 0, 0, 0),
            ide_preview_button_rect_valid: false,
            doom_status: String::from("Listo para iniciar CPP-DOOM."),
            doom_native_running: false,
            doom_native_player_x_fp: DOOM_NATIVE_FP_ONE * 2 + DOOM_NATIVE_FP_ONE / 2,
            doom_native_player_y_fp: DOOM_NATIVE_FP_ONE * 2 + DOOM_NATIVE_FP_ONE / 2,
            doom_native_angle_units: 0,
            doom_native_steps: 0,
            doom_native_shots: 0,
            doom_native_kills: 0,
            doom_native_enemy_alive_mask: ((1u32 << DOOM_NATIVE_ENEMY_COUNT) - 1) as u16,
            doom_native_flash_ticks: 0,
            linux_bridge_status: String::from("Bridge inactivo."),
            linux_bridge_source: String::from("SDL/X11 subset"),
            linux_bridge_width: 0,
            linux_bridge_height: 0,
            linux_bridge_pixels: alloc::vec![],

            wifi_scan_entries: alloc::vec![],
            wifi_selected_index: 0,
            wifi_password_input: String::new(),
            wifi_password_editing: false,
            wifi_scroll: 0,
            wifi_status_msg: String::new(),
            wifi_mode_active: false,

            task_manager_lines: Vec::new(),
            task_manager_scroll: 0,
            task_manager_selected: None,
            task_manager_status: String::new(),
        }
    }

    pub fn new(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);

        win.output_lines.push(String::from("=== Go OS Terminal ==="));
        win.output_lines.push(String::from(""));
        win.output_lines.push(String::from("Available commands:"));
        win.output_lines.push(String::from("  ls        - List files"));
        win.output_lines.push(String::from("  cd <dir>  - Change directory"));
        win.output_lines.push(String::from("  cat <file>- Read file"));
        win.output_lines.push(String::from("  cp <s> <d>- Copy file"));
        win.output_lines.push(String::from("  mv <s> <d>- Move/rename file"));
        win.output_lines.push(String::from("  disks     - List USB/NVMe/HDD devices"));
        win.output_lines.push(String::from("  vols      - List FAT32/exFAT volumes"));
        win.output_lines.push(String::from("  mount <n> - Mount FAT32/exFAT from 'disks' index"));
        win.output_lines.push(String::from("  unmount   - Unmount active volume"));
        win.output_lines.push(String::from("  cpdev     - Copy file between devices (USB/NVMe/HDD)"));
        win.output_lines.push(String::from("  net       - Show transport/IP/failover status"));
        win.output_lines.push(String::from("  net dhcp  - Request dynamic IP"));
        win.output_lines.push(String::from("  net static - Apply default static IP"));
        win.output_lines.push(String::from("  net mode  - Show current IP mode"));
        win.output_lines.push(String::from("  net https <on|off|status> - HTTPS compatibility"));
        win.output_lines.push(String::from("  net diag  - Dump Intel Ethernet registers"));
        win.output_lines.push(String::from("  wifi      - Show WiFi status"));
        win.output_lines.push(String::from("  fetch     - Download file from network"));
        win.output_lines.push(String::from("  web       - Browser backend (builtin/litehtml/vaev/webkit/status)"));
        win.output_lines.push(String::from("  web webkit <status|endpoint|ping|open|frame|input> - Host WebKit bridge"));
        win.output_lines.push(String::from("  web vaev status - Embedded Vaev bridge diagnostics"));
        win.output_lines.push(String::from(
            "  web vaev input <click x y|scroll d|key K|text T|back|forward|reload>",
        ));
        win.output_lines.push(String::from("  install [--autoport] - Install package (.RPX/.ZIP/.TAR/.TAR.GZ/.DEB/.EXE-SFX)"));
        win.output_lines.push(String::from("  entry     - Generic installer entry point"));
        win.output_lines.push(String::from("  linux     - Linux ELF64 phase1 + phase2 dynamic (+ launch experimental)"));
        win.output_lines.push(String::from("  ruby      - Run Ruby subset runtime"));
        win.output_lines.push(String::from("  runapp    - Open .RML app layout in App Runner"));
        win.output_lines.push(String::from("  cppdoom   - Launch CPP-DOOM native app"));
        win.output_lines.push(String::from("  shell     - Launch external UEFI Shell image"));
        win.output_lines.push(String::from("  clear     - Clear screen"));
        win.output_lines.push(String::from("  help      - Show this help"));
        win.output_lines.push(String::from(""));
        // Boot status: privilege layers / syscall bridge
        let phase = crate::privilege::current_phase();
        let bridge = crate::privilege::syscall_bridge_ready();
        let step = crate::privilege::uefi_init_step();
        win.output_lines.push(alloc::format!(
            "PRIV: phase={} bridge_ready={} init_step={}", phase, bridge, step
        ));
        if bridge {
            win.output_lines.push(String::from("REAL-SLICE: gateway HW listo. Usa 'web servort mode real' para activar."));
        } else {
            win.output_lines.push(String::from("REAL-SLICE: gateway HW NO listo (phase<3). Solo compat-shim disponible."));
        }

        win.render();
        win
    }

    pub fn new_explorer(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::Explorer;
        win.set_explorer_home();
        win
    }

    pub fn new_notepad(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::Notepad;
        win.notepad_status = String::from("Use NEW/SAVE/DELETE and edit text area.");
        win.render();
        win
    }

    pub fn new_search(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::Search;
        win.search_status = String::from("Escribe y pulsa Buscar.");
        win.search_input_active = true;
        win.render();
        win
    }

    pub fn new_browser(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::Browser;
        win.browser_url = String::from("redux://welcome");
        
        win.browser_content_lines.push(String::from("Welcome to Go OS Web Browser!"));
        win.browser_content_lines.push(String::from(""));
        win.browser_content_lines.push(String::from("Features:"));
        win.browser_content_lines.push(String::from("- Render grafico local (DOM/layout/raster builtin)"));
        win.browser_content_lines.push(String::from("- HTML/CSS/JS en modo subset (sin host bridge obligatorio)"));
        win.browser_content_lines.push(String::from("- Tip: usa `web native on` para forzar superficie interna"));
        win.browser_content_lines.push(String::from(""));
        win.browser_content_lines.push(String::from("Try visiting: https://example.com"));
        win.browser_content_lines.push(String::from("Also available: redux://about"));

        win.render();
        win
    }

    pub fn new_image_viewer(
        id: usize,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::ImageViewer;
        win.image_viewer_status = String::from("Open a PNG from Explorer.");
        win.render();
        win
    }

    pub fn new_settings(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::Settings;
        win.render();
        win
    }

    pub fn new_wifi_manager(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::WifiManager;
        win.wifi_mode_active = unsafe { crate::net::FAILOVER_POLICY == crate::net::FAILOVER_WIFI_FIRST };
        win.wifi_status_msg = String::from(crate::intel_wifi::get_status());
        win.render();
        win
    }

    pub fn new_task_manager(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::TaskManager;
        win.task_manager_status = String::from("Listo.");
        win.render();
        win
    }

    pub fn new_media_player(id: usize, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::MediaPlayer;
        win.render();
        win
    }

    pub fn new_video_player(id: usize, title: String, x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = Self::new_base(id, &title, x, y, width, height);
        win.kind = WindowKind::VideoPlayer;
        win.doom_native_running = true; // Use this flag as playing/paused state
        win.render();
        win
    }

    pub fn new_app_runner(
        id: usize,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::AppRunner;
        win.app_runner_status = String::from("No .RML layout loaded.");
        win.render();
        win
    }

    pub fn new_ide_studio(
        id: usize,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::IdeStudio;
        win.ide_status = String::from(
            "Redux Studio interno: PREVIEW (RML), LINK (Ruby->RDX), RUBY, RUST CHECK, LOAD, INSTALL/EXPORT RPX.",
        );
        win.render();
        win
    }

    pub fn new_doom_launcher(
        id: usize,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::DoomLauncher;
        win.doom_status = String::from("Listo: click INICIAR (o Enter) para jugar.");
        win.doom_native_running = false;
        win.render();
        win
    }

    pub fn new_linux_bridge(
        id: usize,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Self {
        let mut win = Self::new_base(id, title, x, y, width, height);
        win.kind = WindowKind::LinuxBridge;
        win.linux_bridge_status = String::from("Esperando frames Linux...");
        win.linux_bridge_source = String::from("SDL/X11 subset");
        win.render();
        win
    }

    pub fn content_height(&self) -> i32 {
        self.rect.height as i32 - TITLE_BAR_H
    }

    fn terminal_output_visible_rows(&self) -> usize {
        let available_h =
            (self.content_height() - TERMINAL_TOP_PADDING - TERMINAL_BOTTOM_PADDING).max(TERMINAL_LINE_HEIGHT);
        let total_rows = (available_h / TERMINAL_LINE_HEIGHT).max(1) as usize;
        total_rows.saturating_sub(1).max(1)
    }

    fn terminal_wrap_columns(&self) -> usize {
        let content_w = (self.rect.width as usize).saturating_sub(TERMINAL_TEXT_X.saturating_mul(2));
        (content_w / TERMINAL_CHAR_W).max(1)
    }

    fn terminal_wrapped_line_count(line: &str, cols: usize) -> usize {
        if cols == 0 {
            return 0;
        }
        let bytes = line.as_bytes();
        if bytes.is_empty() {
            return 1;
        }

        let mut count = 0usize;
        let mut start = 0usize;
        while start < bytes.len() {
            count = count.saturating_add(1);
            if bytes.len().saturating_sub(start) <= cols {
                break;
            }

            let mut split = start.saturating_add(cols);
            let mut back = split;
            while back > start && !bytes[back - 1].is_ascii_whitespace() {
                back -= 1;
            }
            if back == start {
                back = split;
            }
            split = back;
            start = split;
            while start < bytes.len() && bytes[start].is_ascii_whitespace() {
                start += 1;
            }
        }
        count
    }

    fn terminal_wrap_line_into(line: &str, cols: usize, out: &mut Vec<String>) {
        if cols == 0 {
            return;
        }
        let bytes = line.as_bytes();
        if bytes.is_empty() {
            out.push(String::new());
            return;
        }

        let mut start = 0usize;
        while start < bytes.len() {
            if bytes.len().saturating_sub(start) <= cols {
                let chunk = core::str::from_utf8(&bytes[start..]).unwrap_or("");
                out.push(String::from(chunk));
                break;
            }

            let mut split = start.saturating_add(cols);
            let mut back = split;
            while back > start && !bytes[back - 1].is_ascii_whitespace() {
                back -= 1;
            }
            if back == start {
                back = split;
            }
            split = back;

            let chunk = core::str::from_utf8(&bytes[start..split]).unwrap_or("");
            out.push(String::from(chunk));

            start = split;
            while start < bytes.len() && bytes[start].is_ascii_whitespace() {
                start += 1;
            }
        }
    }

    fn terminal_wrapped_output_len(&self) -> usize {
        let cols = self.terminal_wrap_columns();
        let mut total = 0usize;
        for line in self.output_lines.iter() {
            total = total.saturating_add(Self::terminal_wrapped_line_count(line.as_str(), cols));
        }
        total
    }

    fn terminal_wrapped_output_lines(&self) -> Vec<String> {
        let cols = self.terminal_wrap_columns();
        let mut wrapped = Vec::new();
        for line in self.output_lines.iter() {
            Self::terminal_wrap_line_into(line.as_str(), cols, &mut wrapped);
        }
        wrapped
    }

    fn terminal_max_scroll(&self) -> usize {
        self.terminal_wrapped_output_len()
            .saturating_sub(self.terminal_output_visible_rows())
    }

    fn terminal_push_line(&mut self, line: String) {
        if self.kind != WindowKind::Terminal {
            return;
        }

        self.output_lines.push(line);
        while self.output_lines.len() > TERMINAL_HISTORY_MAX_LINES {
            self.output_lines.remove(0);
        }
    }

    fn explorer_cols(&self) -> usize {
        let panel_w = if self.explorer_side_panel_open { 210 } else { 0 };
        let usable_w = (self.rect.width as i32 - EXPLORER_MARGIN_X * 2 - panel_w).max(EXPLORER_CELL_W);
        let cols = (usable_w + EXPLORER_GAP_X) / (EXPLORER_CELL_W + EXPLORER_GAP_X);
        cols.max(1) as usize
    }

    fn explorer_icon_rect(&self, index: usize) -> Option<Rect> {
        let cols = self.explorer_cols();
        let col = index % cols;
        let row = index / cols;

        let scroll_rows = self.explorer_scroll;
        if row < scroll_rows {
            return None;
        }

        let relative_row = row - scroll_rows;
        let panel_w = if self.explorer_side_panel_open { 210 } else { 0 };
        let x = EXPLORER_MARGIN_X + panel_w + (col as i32) * (EXPLORER_CELL_W + EXPLORER_GAP_X);
        let y = EXPLORER_MARGIN_Y + (relative_row as i32) * (EXPLORER_CELL_H + EXPLORER_GAP_Y);
        let max_y = self.content_height() - EXPLORER_STATUS_H;

        if max_y <= EXPLORER_MARGIN_Y || y + EXPLORER_CELL_H > max_y {
            return None;
        }

        Some(Rect::new(x, y, EXPLORER_CELL_W as u32, EXPLORER_CELL_H as u32))
    }

    pub fn explorer_max_scroll(&self) -> usize {
        if self.kind != WindowKind::Explorer {
            return 0;
        }
        let cols = self.explorer_cols();
        let total_rows = (self.explorer_items.len() + cols - 1) / cols;
        let max_y = self.content_height() - EXPLORER_STATUS_H;
        let usable_h = (max_y - EXPLORER_MARGIN_Y).max(0);
        let visible_rows = (usable_h / (EXPLORER_CELL_H + EXPLORER_GAP_Y)) as usize;
        
        total_rows.saturating_sub(visible_rows)
    }

    fn explorer_search_button_rect(&self) -> Rect {
        let x = (self.rect.width as i32 - 116).max(146);
        Rect::new(x, 5, EXPLORER_SEARCH_BUTTON_W as u32, 20)
    }

    fn explorer_search_query_rect(&self) -> Rect {
        let button = self.explorer_search_button_rect();
        let right = button.x - 6;
        let left_bound = 126;
        let available = (right - left_bound).max(EXPLORER_SEARCH_FIELD_MIN_W);
        let width = available.min(EXPLORER_SEARCH_FIELD_MAX_W);
        Rect::new(right - width, button.y, width as u32, 20)
    }

    pub fn explorer_scroll_up_rect(&self) -> Rect {
        Rect::new(self.rect.width as i32 - 46, 6, 18, 18)
    }

    pub fn explorer_scroll_down_rect(&self) -> Rect {
        Rect::new(self.rect.width as i32 - 24, 6, 18, 18)
    }

    fn notepad_button_rect(&self, index: usize) -> Rect {
        let x = 10 + (index as i32 * 74);
        Rect::new(x, 7, 68, 20)
    }

    fn notepad_filename_rect(&self) -> Rect {
        let raw_x = (self.rect.width as i32 - 360).max(10);
        let x = raw_x.min((self.rect.width as i32 - 90).max(10));
        let width = (self.rect.width as i32 - x - 10).max(80) as u32;
        Rect::new(x, 7, width, 20)
    }

    fn notepad_editor_rect(&self) -> Rect {
        let y = NOTEPAD_TOP_H + 6;
        let h = (self.content_height() - y - NOTEPAD_STATUS_H - 6).max(24) as u32;
        Rect::new(8, y, self.rect.width.saturating_sub(16), h)
    }

    fn notepad_status_rect(&self) -> Rect {
        let y = (self.content_height() - NOTEPAD_STATUS_H).max(0);
        Rect::new(0, y, self.rect.width, NOTEPAD_STATUS_H as u32)
    }

    fn search_query_rect(&self) -> Rect {
        let query_w = self.rect.width.saturating_sub(116);
        Rect::new(10, 10, query_w.max(80), 24)
    }

    fn search_button_rect(&self) -> Rect {
        let query = self.search_query_rect();
        Rect::new(query.x + query.width as i32 + 8, query.y, 88, 24)
    }

    fn search_results_rect(&self) -> Rect {
        let y = SEARCH_TOP_H;
        let h = (self.content_height() - y - SEARCH_STATUS_H).max(0) as u32;
        Rect::new(0, y, self.rect.width, h)
    }

    fn search_status_rect(&self) -> Rect {
        let y = (self.content_height() - SEARCH_STATUS_H).max(0);
        Rect::new(0, y, self.rect.width, SEARCH_STATUS_H as u32)
    }

    fn search_visible_rows(&self) -> usize {
        let rect = self.search_results_rect();
        ((rect.height as i32 - 8).max(0) / SEARCH_RESULT_ROW_H).max(0) as usize
    }

    fn search_result_row_rect(&self, index: usize) -> Option<Rect> {
        let area = self.search_results_rect();
        let y = area.y + 4 + (index as i32 * SEARCH_RESULT_ROW_H);
        if y + SEARCH_RESULT_ROW_H > (area.y + area.height as i32 - 2) {
            return None;
        }
        Some(Rect::new(
            area.x + 8,
            y,
            area.width.saturating_sub(16),
            (SEARCH_RESULT_ROW_H - 4) as u32,
        ))
    }

    fn browser_url_rect(&self) -> Rect {
        let x = 70; // Back/Fwd buttons space
        let width = self.rect.width.saturating_sub(x as u32 + 140); // Go + scroll controls
        Rect::new(x, 10, width, 24)
    }

    fn browser_go_rect(&self) -> Rect {
        let url_rect = self.browser_url_rect();
        let x = url_rect.x + url_rect.width as i32 + 10;
        Rect::new(x, 10, 52, 24)
    }

    fn browser_scroll_up_rect(&self) -> Rect {
        let go = self.browser_go_rect();
        let x = go.x + go.width as i32 + 8;
        Rect::new(x, 10, 20, 11)
    }

    fn browser_scroll_down_rect(&self) -> Rect {
        let up = self.browser_scroll_up_rect();
        Rect::new(up.x, up.y + 13, up.width, 11)
    }

    fn browser_viewport_rect(&self) -> Rect {
        let y = BROWSER_TOP_H;
        let h = (self.content_height() - y - BROWSER_STATUS_H).max(0) as u32;
        Rect::new(0, y, self.rect.width, h)
    }

    fn image_viewer_canvas_rect(&self) -> Rect {
        let y = IMAGE_VIEWER_TOP_H;
        let h = (self.content_height() - y - IMAGE_VIEWER_STATUS_H).max(0) as u32;
        Rect::new(0, y, self.rect.width, h)
    }

    fn app_runner_canvas_rect(&self) -> Rect {
        let y = APP_RUNNER_TOP_H;
        let h = (self.content_height() - y - APP_RUNNER_STATUS_H).max(0) as u32;
        Rect::new(0, y, self.rect.width, h)
    }

    fn ide_tab_rect(&self, index: usize) -> Rect {
        let x = 10 + index as i32 * 76;
        Rect::new(x, 8, 70, 20)
    }

    fn ide_action_rect(&self, index: usize) -> Rect {
        let labels = 8i32;
        let gap = 4i32;
        let available_w = (self.rect.width as i32 - 20).max(360);
        let btn_w = ((available_w - (labels - 1) * gap) / labels).clamp(52, 72);
        let total = labels * btn_w + (labels - 1) * gap;
        let start_x = (self.rect.width as i32 - total - 10).max(10);
        let x = start_x + index as i32 * (btn_w + gap);
        Rect::new(x, 32, btn_w as u32, 22)
    }

    fn ide_view_input_rect(&self) -> Rect {
        let preview = self.ide_preview_rect();
        let x = preview.x + 62;
        let y = preview.y + 3;
        let go_w = 34i32;
        let spacing = 6i32;
        let max_w = (preview.width as i32 - 62 - go_w - spacing - 8).max(64);
        Rect::new(x, y, max_w as u32, 14)
    }

    fn ide_view_go_rect(&self) -> Rect {
        let input = self.ide_view_input_rect();
        Rect::new(input.x + input.width as i32 + 6, input.y - 1, 34, 16)
    }

    fn ide_editor_rect(&self) -> Rect {
        let y = IDE_STUDIO_TOP_H + 6;
        let available_w = (self.rect.width as i32).max(320);
        let right_panel = (available_w / 3).clamp(220, 420);
        let mut editor_w = available_w - right_panel - 22;
        if editor_w < 160 {
            editor_w = 160;
        }
        let h = (self.content_height() - y - IDE_STUDIO_STATUS_H - 6).max(32) as u32;
        Rect::new(8, y, editor_w as u32, h)
    }

    fn ide_preview_rect(&self) -> Rect {
        let editor = self.ide_editor_rect();
        let x = editor.x + editor.width as i32 + 8;
        let y = editor.y;
        let w = (self.rect.width as i32 - x - 8).max(120) as u32;
        Rect::new(x, y, w, editor.height)
    }

    fn ide_status_rect(&self) -> Rect {
        let y = (self.content_height() - IDE_STUDIO_STATUS_H).max(0);
        Rect::new(0, y, self.rect.width, IDE_STUDIO_STATUS_H as u32)
    }

    fn ide_preview_button_rect(&self) -> Rect {
        if self.ide_preview_button_rect_valid {
            return self.ide_preview_button_rect_cached;
        }
        let preview = self.ide_preview_rect();
        let panel = Rect::new(
            preview.x + 8,
            preview.y + 18,
            preview.width.saturating_sub(16),
            preview.height.saturating_sub(26),
        );
        let btn_w = ((panel.width as i32 / 2).clamp(80, 220)) as u32;
        let btn_h = 24u32;
        let btn_x = panel.x + ((panel.width as i32 - btn_w as i32) / 2);
        let btn_y = (panel.y + panel.height as i32 - btn_h as i32 - 10).max(panel.y + 30);
        Rect::new(btn_x, btn_y, btn_w, btn_h)
    }

    fn ide_default_preview_elements(
        header_text: &str,
        body_text: &str,
        button_label: &str,
        button_id: &str,
        header_color: u32,
        body_color: u32,
        button_color: u32,
    ) -> Vec<PreviewElement> {
        alloc::vec![
            PreviewElement {
                kind: PreviewElementKind::Header,
                text: String::from(header_text),
                id: String::new(),
                color: header_color,
                size: 24,
                margin_top: 0,
                margin_bottom: 6,
                margin_left: 0,
                margin_right: 0,
            },
            PreviewElement {
                kind: PreviewElementKind::Text,
                text: String::from(body_text),
                id: String::new(),
                color: body_color,
                size: 14,
                margin_top: 0,
                margin_bottom: 10,
                margin_left: 0,
                margin_right: 0,
            },
            PreviewElement {
                kind: PreviewElementKind::Button,
                text: String::from(button_label),
                id: String::from(button_id),
                color: button_color,
                size: 14,
                margin_top: 0,
                margin_bottom: 0,
                margin_left: 0,
                margin_right: 0,
            },
        ]
    }

    fn preview_scale_from_size(size: i32) -> u32 {
        let s = size.clamp(8, 48);
        if s >= 30 {
            3
        } else if s >= 18 {
            2
        } else {
            1
        }
    }

    pub fn draw_char_scaled(&mut self, x: u32, y: u32, ch: char, color: Color, scale: u32) {
        let sc = scale.max(1);
        if sc == 1 {
            self.draw_char(x, y, ch, color);
            return;
        }
        let glyph = crate::font::glyph_5x7(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                let mask = 1 << (4 - col);
                if (bits & mask) != 0 {
                    self.fill_rect(
                        Rect::new(
                            x as i32 + col as i32 * sc as i32,
                            y as i32 + row as i32 * sc as i32,
                            sc,
                            sc,
                        ),
                        color,
                    );
                }
            }
        }
    }

    pub fn draw_text_scaled(&mut self, x: u32, y: u32, text: &[u8], color: Color, scale: u32) {
        let sc = scale.max(1);
        let mut cx = x;
        let mut cy = y;
        let adv_x = 6 * sc;
        let adv_y = 8 * sc;
        for &b in text {
            if b == b'\n' {
                cx = x;
                cy += adv_y;
                continue;
            }
            let ch = if b.is_ascii() { b as char } else { '?' };
            self.draw_char_scaled(cx, cy, ch, color, sc);
            cx += adv_x;
        }
    }

    fn ide_active_text_and_cursor(&self) -> (&str, usize) {
        match self.ide_active_tab {
            0 => (self.ide_rust_text.as_str(), self.ide_cursor_rust),
            1 => (self.ide_ruby_text.as_str(), self.ide_cursor_ruby),
            2 => (self.ide_rml_text.as_str(), self.ide_cursor_rml),
            3 => (self.ide_rdx_text.as_str(), self.ide_cursor_rdx),
            _ => (self.ide_docs_text.as_str(), self.ide_cursor_docs),
        }
    }

    fn ide_active_text_and_cursor_mut(&mut self) -> (&mut String, &mut usize) {
        match self.ide_active_tab {
            0 => (&mut self.ide_rust_text, &mut self.ide_cursor_rust),
            1 => (&mut self.ide_ruby_text, &mut self.ide_cursor_ruby),
            2 => (&mut self.ide_rml_text, &mut self.ide_cursor_rml),
            3 => (&mut self.ide_rdx_text, &mut self.ide_cursor_rdx),
            _ => (&mut self.ide_docs_text, &mut self.ide_cursor_docs),
        }
    }

    fn ide_active_selection(&self) -> (usize, usize) {
        match self.ide_active_tab {
            0 => (self.ide_sel_start_rust, self.ide_sel_end_rust),
            1 => (self.ide_sel_start_ruby, self.ide_sel_end_ruby),
            2 => (self.ide_sel_start_rml, self.ide_sel_end_rml),
            3 => (self.ide_sel_start_rdx, self.ide_sel_end_rdx),
            _ => (self.ide_sel_start_docs, self.ide_sel_end_docs),
        }
    }

    fn ide_active_selection_mut(&mut self) -> (&mut usize, &mut usize) {
        match self.ide_active_tab {
            0 => (&mut self.ide_sel_start_rust, &mut self.ide_sel_end_rust),
            1 => (&mut self.ide_sel_start_ruby, &mut self.ide_sel_end_ruby),
            2 => (&mut self.ide_sel_start_rml, &mut self.ide_sel_end_rml),
            3 => (&mut self.ide_sel_start_rdx, &mut self.ide_sel_end_rdx),
            _ => (&mut self.ide_sel_start_docs, &mut self.ide_sel_end_docs),
        }
    }

    fn ide_active_text_cursor_selection_mut(
        &mut self,
    ) -> (&mut String, &mut usize, &mut usize, &mut usize) {
        match self.ide_active_tab {
            0 => (
                &mut self.ide_rust_text,
                &mut self.ide_cursor_rust,
                &mut self.ide_sel_start_rust,
                &mut self.ide_sel_end_rust,
            ),
            1 => (
                &mut self.ide_ruby_text,
                &mut self.ide_cursor_ruby,
                &mut self.ide_sel_start_ruby,
                &mut self.ide_sel_end_ruby,
            ),
            2 => (
                &mut self.ide_rml_text,
                &mut self.ide_cursor_rml,
                &mut self.ide_sel_start_rml,
                &mut self.ide_sel_end_rml,
            ),
            3 => (
                &mut self.ide_rdx_text,
                &mut self.ide_cursor_rdx,
                &mut self.ide_sel_start_rdx,
                &mut self.ide_sel_end_rdx,
            ),
            _ => (
                &mut self.ide_docs_text,
                &mut self.ide_cursor_docs,
                &mut self.ide_sel_start_docs,
                &mut self.ide_sel_end_docs,
            ),
        }
    }

    fn ide_snapshot_same(a: &IdeEditorSnapshot, b: &IdeEditorSnapshot) -> bool {
        a.tab == b.tab
            && a.text == b.text
            && a.cursor == b.cursor
            && a.sel_start == b.sel_start
            && a.sel_end == b.sel_end
    }

    fn ide_snapshot_for_tab(&self, tab: u8) -> IdeEditorSnapshot {
        match tab {
            0 => {
                let cursor = Self::ide_clamp_cursor_index(self.ide_rust_text.as_str(), self.ide_cursor_rust);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_rust_text.as_str(),
                    self.ide_sel_start_rust,
                    self.ide_sel_end_rust,
                );
                IdeEditorSnapshot {
                    tab: 0,
                    text: self.ide_rust_text.clone(),
                    cursor,
                    sel_start,
                    sel_end,
                }
            }
            1 => {
                let cursor = Self::ide_clamp_cursor_index(self.ide_ruby_text.as_str(), self.ide_cursor_ruby);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_ruby_text.as_str(),
                    self.ide_sel_start_ruby,
                    self.ide_sel_end_ruby,
                );
                IdeEditorSnapshot {
                    tab: 1,
                    text: self.ide_ruby_text.clone(),
                    cursor,
                    sel_start,
                    sel_end,
                }
            }
            2 => {
                let cursor = Self::ide_clamp_cursor_index(self.ide_rml_text.as_str(), self.ide_cursor_rml);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_rml_text.as_str(),
                    self.ide_sel_start_rml,
                    self.ide_sel_end_rml,
                );
                IdeEditorSnapshot {
                    tab: 2,
                    text: self.ide_rml_text.clone(),
                    cursor,
                    sel_start,
                    sel_end,
                }
            }
            3 => {
                let cursor = Self::ide_clamp_cursor_index(self.ide_rdx_text.as_str(), self.ide_cursor_rdx);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_rdx_text.as_str(),
                    self.ide_sel_start_rdx,
                    self.ide_sel_end_rdx,
                );
                IdeEditorSnapshot {
                    tab: 3,
                    text: self.ide_rdx_text.clone(),
                    cursor,
                    sel_start,
                    sel_end,
                }
            }
            _ => {
                let cursor = Self::ide_clamp_cursor_index(self.ide_docs_text.as_str(), self.ide_cursor_docs);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_docs_text.as_str(),
                    self.ide_sel_start_docs,
                    self.ide_sel_end_docs,
                );
                IdeEditorSnapshot {
                    tab: 4,
                    text: self.ide_docs_text.clone(),
                    cursor,
                    sel_start,
                    sel_end,
                }
            }
        }
    }

    fn ide_apply_snapshot(&mut self, snapshot: &IdeEditorSnapshot) {
        match snapshot.tab {
            0 => {
                self.ide_rust_text = snapshot.text.clone();
                let cursor = Self::ide_clamp_cursor_index(self.ide_rust_text.as_str(), snapshot.cursor);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_rust_text.as_str(),
                    snapshot.sel_start,
                    snapshot.sel_end,
                );
                self.ide_cursor_rust = cursor;
                self.ide_sel_start_rust = sel_start;
                self.ide_sel_end_rust = sel_end;
            }
            1 => {
                self.ide_ruby_text = snapshot.text.clone();
                let cursor = Self::ide_clamp_cursor_index(self.ide_ruby_text.as_str(), snapshot.cursor);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_ruby_text.as_str(),
                    snapshot.sel_start,
                    snapshot.sel_end,
                );
                self.ide_cursor_ruby = cursor;
                self.ide_sel_start_ruby = sel_start;
                self.ide_sel_end_ruby = sel_end;
            }
            2 => {
                self.ide_rml_text = snapshot.text.clone();
                let cursor = Self::ide_clamp_cursor_index(self.ide_rml_text.as_str(), snapshot.cursor);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_rml_text.as_str(),
                    snapshot.sel_start,
                    snapshot.sel_end,
                );
                self.ide_cursor_rml = cursor;
                self.ide_sel_start_rml = sel_start;
                self.ide_sel_end_rml = sel_end;
            }
            3 => {
                self.ide_rdx_text = snapshot.text.clone();
                let cursor = Self::ide_clamp_cursor_index(self.ide_rdx_text.as_str(), snapshot.cursor);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_rdx_text.as_str(),
                    snapshot.sel_start,
                    snapshot.sel_end,
                );
                self.ide_cursor_rdx = cursor;
                self.ide_sel_start_rdx = sel_start;
                self.ide_sel_end_rdx = sel_end;
            }
            _ => {
                self.ide_docs_text = snapshot.text.clone();
                let cursor = Self::ide_clamp_cursor_index(self.ide_docs_text.as_str(), snapshot.cursor);
                let (sel_start, sel_end) = Self::ide_clamp_selection_for_text(
                    self.ide_docs_text.as_str(),
                    snapshot.sel_start,
                    snapshot.sel_end,
                );
                self.ide_cursor_docs = cursor;
                self.ide_sel_start_docs = sel_start;
                self.ide_sel_end_docs = sel_end;
            }
        }
    }

    fn ide_push_undo_snapshot(&mut self) {
        if self.kind != WindowKind::IdeStudio || self.ide_active_tab == 4 {
            return;
        }
        let snapshot = self.ide_snapshot_for_tab(self.ide_active_tab);
        if self
            .ide_undo_stack
            .last()
            .map(|last| Self::ide_snapshot_same(last, &snapshot))
            .unwrap_or(false)
        {
            self.ide_redo_stack.clear();
            return;
        }
        self.ide_undo_stack.push(snapshot);
        if self.ide_undo_stack.len() > IDE_STUDIO_UNDO_STACK_LIMIT {
            self.ide_undo_stack.remove(0);
        }
        self.ide_redo_stack.clear();
    }

    fn ide_editor_max_cols(editor: Rect) -> usize {
        ((editor.width as i32 - 12).max(IDE_STUDIO_EDITOR_CHAR_W) / IDE_STUDIO_EDITOR_CHAR_W) as usize
    }

    fn ide_editor_max_lines(editor: Rect) -> usize {
        ((editor.height as i32 - 18).max(IDE_STUDIO_EDITOR_LINE_H) / IDE_STUDIO_EDITOR_LINE_H) as usize
    }

    fn ide_clamp_cursor_index(text: &str, cursor: usize) -> usize {
        let mut c = cursor.min(text.len());
        while c > 0 && !text.is_char_boundary(c) {
            c -= 1;
        }
        c
    }

    fn ide_prev_cursor_index(text: &str, cursor: usize) -> usize {
        let cur = Self::ide_clamp_cursor_index(text, cursor);
        if cur == 0 {
            return 0;
        }
        text[..cur]
            .char_indices()
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }

    fn ide_next_cursor_index(text: &str, cursor: usize) -> usize {
        let cur = Self::ide_clamp_cursor_index(text, cursor);
        if cur >= text.len() {
            return text.len();
        }
        match text[cur..].chars().next() {
            Some(ch) => cur + ch.len_utf8(),
            None => text.len(),
        }
    }

    fn ide_cursor_line_col(text: &str, cursor: usize) -> (usize, usize) {
        let cur = Self::ide_clamp_cursor_index(text, cursor);
        let mut line = 0usize;
        let mut col = 0usize;
        for (idx, ch) in text.char_indices() {
            if idx >= cur {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn ide_line_starts(text: &str) -> Vec<usize> {
        let mut starts = Vec::new();
        starts.push(0);
        for (idx, ch) in text.char_indices() {
            if ch == '\n' {
                starts.push(idx + 1);
            }
        }
        starts
    }

    fn ide_line_bounds(text: &str, starts: &[usize], line_idx: usize) -> (usize, usize) {
        if starts.is_empty() {
            return (0, 0);
        }
        let idx = line_idx.min(starts.len().saturating_sub(1));
        let start = starts[idx];
        let end = if idx + 1 < starts.len() {
            starts[idx + 1].saturating_sub(1)
        } else {
            text.len()
        };
        (start.min(text.len()), end.min(text.len()))
    }

    fn ide_byte_index_for_col(text: &str, char_col: usize) -> usize {
        text.char_indices()
            .nth(char_col)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    fn ide_col_for_byte_index(text: &str, byte_idx: usize) -> usize {
        let idx = Self::ide_clamp_cursor_index(text, byte_idx);
        text[..idx].chars().count()
    }

    fn ide_cursor_from_line_col(text: &str, target_line: usize, target_col: usize) -> usize {
        let starts = Self::ide_line_starts(text);
        if starts.is_empty() {
            return 0;
        }
        let line = target_line.min(starts.len().saturating_sub(1));
        let (start, end) = Self::ide_line_bounds(text, starts.as_slice(), line);
        let line_text = &text[start..end];
        let byte_offset = Self::ide_byte_index_for_col(line_text, target_col);
        (start + byte_offset).min(text.len())
    }

    fn ide_line_comment_supports_hash(tab: u8) -> bool {
        tab == 1 || tab == 3
    }

    fn ide_line_comment_supports_slash(tab: u8) -> bool {
        tab == 0 || tab == 1 || tab == 3
    }

    fn ide_block_comment_tokens(tab: u8) -> Option<(&'static str, &'static str)> {
        match tab {
            0 | 1 | 3 => Some(("/*", "*/")),
            2 => Some(("<!--", "-->")),
            _ => None,
        }
    }

    fn ide_comment_line_segments(line: &str, tab: u8, in_block: &mut bool) -> Vec<(usize, usize, bool)> {
        let mut out: Vec<(usize, usize, bool)> = Vec::new();
        if line.is_empty() {
            return out;
        }

        let mut seg_start = 0usize;
        let mut pos = 0usize;
        let mut quote: Option<char> = None;
        let mut in_block_local = *in_block;
        let block_tokens = Self::ide_block_comment_tokens(tab);

        while pos < line.len() {
            if in_block_local {
                if let Some((_, end_tok)) = block_tokens {
                    if line[pos..].starts_with(end_tok) {
                        let end_pos = pos + end_tok.len();
                        if seg_start < end_pos {
                            out.push((seg_start, end_pos, true));
                        }
                        seg_start = end_pos;
                        pos = end_pos;
                        in_block_local = false;
                        continue;
                    }
                }
                pos = Self::ide_next_cursor_index(line, pos);
                continue;
            }

            let ch = match line[pos..].chars().next() {
                Some(v) => v,
                None => break,
            };

            if let Some(q) = quote {
                if ch == '\\' {
                    let next = Self::ide_next_cursor_index(line, pos);
                    if next >= line.len() {
                        break;
                    }
                    pos = Self::ide_next_cursor_index(line, next);
                    continue;
                }
                if ch == q {
                    quote = None;
                }
                pos = Self::ide_next_cursor_index(line, pos);
                continue;
            }

            if tab != 2 && (ch == '"' || ch == '\'') {
                quote = Some(ch);
                pos = Self::ide_next_cursor_index(line, pos);
                continue;
            }

            if Self::ide_line_comment_supports_hash(tab) && ch == '#' {
                if seg_start < pos {
                    out.push((seg_start, pos, false));
                }
                out.push((pos, line.len(), true));
                *in_block = false;
                return out;
            }

            if Self::ide_line_comment_supports_slash(tab) && line[pos..].starts_with("//") {
                if seg_start < pos {
                    out.push((seg_start, pos, false));
                }
                out.push((pos, line.len(), true));
                *in_block = false;
                return out;
            }

            if let Some((start_tok, _)) = block_tokens {
                if line[pos..].starts_with(start_tok) {
                    if seg_start < pos {
                        out.push((seg_start, pos, false));
                    }
                    seg_start = pos;
                    pos += start_tok.len();
                    in_block_local = true;
                    continue;
                }
            }

            pos = Self::ide_next_cursor_index(line, pos);
        }

        if seg_start < line.len() {
            out.push((seg_start, line.len(), in_block_local));
        }
        *in_block = in_block_local;
        out
    }

    fn ide_comment_block_state_before(
        text: &str,
        starts: &[usize],
        line_start: usize,
        tab: u8,
    ) -> bool {
        if line_start == 0 || starts.is_empty() {
            return false;
        }
        let mut in_block = false;
        for idx in 0..line_start.min(starts.len()) {
            let (beg, end) = Self::ide_line_bounds(text, starts, idx);
            let line = &text[beg..end];
            let _ = Self::ide_comment_line_segments(line, tab, &mut in_block);
        }
        in_block
    }

    fn ide_viewport_origin(text: &str, cursor: usize, max_cols: usize, max_lines: usize) -> (usize, usize, usize, usize) {
        let (cursor_line, cursor_col) = Self::ide_cursor_line_col(text, cursor);
        let line_start = cursor_line.saturating_sub(max_lines.saturating_sub(1));
        let col_start = cursor_col.saturating_sub(max_cols.saturating_sub(1));
        (line_start, col_start, cursor_line, cursor_col)
    }

    fn ide_clamp_selection_for_text(text: &str, start: usize, end: usize) -> (usize, usize) {
        let a = Self::ide_clamp_cursor_index(text, start);
        let b = Self::ide_clamp_cursor_index(text, end);
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    fn ide_delete_selection_only(&mut self) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let (cur, start, end) = {
            let (text, cur_raw) = self.ide_active_text_and_cursor();
            let cur = Self::ide_clamp_cursor_index(text, cur_raw);
            let (sel_start_raw, sel_end_raw) = self.ide_active_selection();
            let (start, end) = Self::ide_clamp_selection_for_text(text, sel_start_raw, sel_end_raw);
            (cur, start, end)
        };
        if start >= end {
            let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
            let clamped = Self::ide_clamp_cursor_index(target.as_str(), cur);
            *cursor = clamped;
            *sel_start = clamped;
            *sel_end = clamped;
            return false;
        }
        self.ide_push_undo_snapshot();
        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        let clamped_start = start.min(target.len());
        let clamped_end = end.min(target.len());
        if clamped_start >= clamped_end {
            *cursor = cur;
            *sel_start = cur;
            *sel_end = cur;
            return false;
        }
        target.replace_range(clamped_start..clamped_end, "");
        *cursor = clamped_start;
        *sel_start = clamped_start;
        *sel_end = clamped_start;
        true
    }

    fn ide_insert_text_at_cursor_or_selection(&mut self, inserted: &str) -> bool {
        if self.kind != WindowKind::IdeStudio || inserted.is_empty() {
            return false;
        }
        if self.ide_active_tab == 4 {
            return false;
        }
        let (cur, start, end, next_len) = {
            let (text, cur_raw) = self.ide_active_text_and_cursor();
            let cur = Self::ide_clamp_cursor_index(text, cur_raw);
            let (sel_start_raw, sel_end_raw) = self.ide_active_selection();
            let (start, end) = Self::ide_clamp_selection_for_text(text, sel_start_raw, sel_end_raw);
            let (replace_start, replace_end) = if start < end { (start, end) } else { (cur, cur) };
            let replaced_len = replace_end.saturating_sub(replace_start);
            let next_len = text
                .len()
                .saturating_sub(replaced_len)
                .saturating_add(inserted.len());
            (cur, start, end, next_len)
        };
        let (replace_start, replace_end) = if start < end { (start, end) } else { (cur, cur) };
        if next_len > IDE_STUDIO_MAX_TEXT_BYTES {
            return false;
        }
        self.ide_push_undo_snapshot();
        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        target.replace_range(replace_start..replace_end, inserted);
        let next_cursor = replace_start + inserted.len();
        *cursor = next_cursor;
        *sel_start = next_cursor;
        *sel_end = next_cursor;
        true
    }

    fn ide_cursor_from_point(&self, global_x: i32, global_y: i32) -> Option<usize> {
        if self.kind != WindowKind::IdeStudio {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        let editor = self.ide_editor_rect();
        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };
        if !editor.contains(p) {
            return None;
        }

        let max_cols = Self::ide_editor_max_cols(editor).max(1);
        let max_lines = Self::ide_editor_max_lines(editor).max(1);

        let (text, cur_raw) = self.ide_active_text_and_cursor();
        let cur = Self::ide_clamp_cursor_index(text, cur_raw);
        let (line_start, col_start, _, _) = Self::ide_viewport_origin(text, cur, max_cols, max_lines);

        let rel_x = (local_x - (editor.x + IDE_STUDIO_EDITOR_TEXT_X)).max(0);
        let rel_y = (local_y - (editor.y + IDE_STUDIO_EDITOR_TEXT_Y)).max(0);
        let row = (rel_y / IDE_STUDIO_EDITOR_LINE_H).max(0) as usize;
        let col = (rel_x / IDE_STUDIO_EDITOR_CHAR_W).max(0) as usize;
        let target_line = line_start + row.min(max_lines.saturating_sub(1));
        let target_col = col_start + col;
        Some(Self::ide_cursor_from_line_col(text, target_line, target_col))
    }

    fn ide_render_preview_elements(&mut self, panel: Rect) -> Option<Rect> {
        let mut first_button: Option<Rect> = None;
        let base_pad = self.ide_preview_padding.clamp(0, 64);
        let mut y = panel.y + base_pad;
        let reserved_event_h = 14;
        let bottom_limit = panel.y + panel.height as i32 - base_pad - reserved_event_h;

        self.ide_preview_button_targets.clear();
        let elements = self.ide_preview_elements.clone();
        if elements.is_empty() {
            return None;
        }

        for element in elements.iter() {
            if y >= bottom_limit {
                break;
            }

            let mt = element.margin_top.clamp(-128, 256);
            let mb = element.margin_bottom.clamp(-128, 256);
            let ml = element.margin_left.clamp(-128, 256);
            let mr = element.margin_right.clamp(-128, 256);
            y = (y + mt).max(panel.y + base_pad);

            let x = panel.x + base_pad + ml;
            let right = panel.x + panel.width as i32 - base_pad - mr;
            let width = (right - x).max(24);
            let scale = Self::preview_scale_from_size(element.size);
            let char_w = 6 * scale as i32;
            let line_h = 8 * scale as i32;
            let row_step = line_h + 2;
            let max_cols = ((width - 2).max(char_w) / char_w.max(1)) as usize;

            match element.kind {
                PreviewElementKind::Header => {
                    let lines = Self::wrap_text_lines(element.text.as_str(), max_cols.max(1), 3);
                    let mut ly = y;
                    for line in lines.iter() {
                        if ly + line_h >= bottom_limit {
                            break;
                        }
                        self.draw_text_scaled(
                            x.max(0) as u32,
                            ly.max(0) as u32,
                            line.as_bytes(),
                            Color(element.color),
                            scale,
                        );
                        ly += row_step;
                    }
                    y = ly + mb;
                }
                PreviewElementKind::Text => {
                    let lines = Self::wrap_text_lines(element.text.as_str(), max_cols.max(1), 12);
                    let mut ly = y;
                    for line in lines.iter() {
                        if ly + line_h >= bottom_limit {
                            break;
                        }
                        self.draw_text_scaled(
                            x.max(0) as u32,
                            ly.max(0) as u32,
                            line.as_bytes(),
                            Color(element.color),
                            scale,
                        );
                        ly += row_step;
                    }
                    y = ly + mb;
                }
                PreviewElementKind::Button => {
                    let label_max = (max_cols.saturating_sub(2)).max(1);
                    let label = Self::trim_label(element.text.as_str(), label_max);
                    let ideal_w = ((label.len() as i32 + 4) * char_w).clamp(72, width);
                    let btn_w = ideal_w.max(48) as u32;
                    let btn_h = (line_h + 12).clamp(22, 42) as u32;
                    let btn_rect = Rect::new(x, y, btn_w, btn_h);
                    if btn_rect.y + (btn_rect.height as i32) < bottom_limit {
                        self.fill_rect(btn_rect, Color(element.color));
                        self.draw_border(btn_rect, Color(0x18314A));
                        let text_y = btn_rect.y + ((btn_h as i32 - line_h) / 2).max(1);
                        self.draw_text_scaled(
                            (btn_rect.x + 8).max(0) as u32,
                            text_y.max(0) as u32,
                            label.as_bytes(),
                            Color(0xFFFFFF),
                            scale,
                        );
                        let target_id = if element.id.trim().is_empty() {
                            if self.ide_preview_button_id.trim().is_empty() {
                                String::new()
                            } else {
                                self.ide_preview_button_id.clone()
                            }
                        } else {
                            String::from(element.id.trim())
                        };
                        self.ide_preview_button_targets.push((btn_rect, target_id));
                        if first_button.is_none() {
                            first_button = Some(btn_rect);
                        }
                    }
                    y += btn_h as i32 + mb;
                }
            }
        }

        first_button
    }

    fn doom_launcher_canvas_rect(&self) -> Rect {
        let y = DOOM_LAUNCHER_TOP_H;
        let h = (self.content_height() - y - DOOM_LAUNCHER_STATUS_H).max(0) as u32;
        Rect::new(0, y, self.rect.width, h)
    }

    fn doom_launch_button_rect(&self) -> Rect {
        let view = self.doom_launcher_canvas_rect();
        let btn_w = if self.doom_native_running {
            144u32.min(view.width.saturating_sub(24).max(110))
        } else {
            180u32.min(view.width.saturating_sub(24).max(120))
        };
        let btn_h = if self.doom_native_running { 28u32 } else { 34u32 };
        let x = if self.doom_native_running {
            view.x + (view.width as i32 - btn_w as i32 - 12).max(0)
        } else {
            view.x + ((view.width as i32 - btn_w as i32) / 2).max(0)
        };
        let y = if self.doom_native_running {
            view.y + 8
        } else {
            view.y + 34
        };
        Rect::new(x, y, btn_w, btn_h)
    }

    fn doom_status_rect(&self) -> Rect {
        let y = (self.content_height() - DOOM_LAUNCHER_STATUS_H).max(0);
        Rect::new(0, y, self.rect.width, DOOM_LAUNCHER_STATUS_H as u32)
    }

    fn doom_native_reset_state(&mut self) {
        self.doom_native_running = true;
        self.doom_native_player_x_fp = DOOM_NATIVE_FP_ONE * 2 + DOOM_NATIVE_FP_ONE / 2;
        self.doom_native_player_y_fp = DOOM_NATIVE_FP_ONE * 2 + DOOM_NATIVE_FP_ONE / 2;
        self.doom_native_angle_units = 0;
        self.doom_native_steps = 0;
        self.doom_native_shots = 0;
        self.doom_native_kills = 0;
        self.doom_native_enemy_alive_mask = ((1u32 << DOOM_NATIVE_ENEMY_COUNT) - 1) as u16;
        self.doom_native_flash_ticks = 0;
    }

    fn doom_native_wall_kind(cell_x: i32, cell_y: i32) -> u8 {
        if cell_x < 0 || cell_y < 0 {
            return b'#';
        }
        if cell_x >= DOOM_NATIVE_MAP_W as i32 || cell_y >= DOOM_NATIVE_MAP_H as i32 {
            return b'#';
        }
        DOOM_NATIVE_MAP[cell_y as usize][cell_x as usize]
    }

    fn doom_native_enemy_position_fp(index: usize) -> (i32, i32) {
        let i = index.min(DOOM_NATIVE_ENEMY_COUNT.saturating_sub(1));
        let (cell_x, cell_y) = DOOM_NATIVE_ENEMY_POS_CELLS[i];
        (
            cell_x * DOOM_NATIVE_FP_ONE + DOOM_NATIVE_FP_ONE / 2,
            cell_y * DOOM_NATIVE_FP_ONE + DOOM_NATIVE_FP_ONE / 2,
        )
    }

    fn doom_native_enemy_alive(&self, index: usize) -> bool {
        if index >= DOOM_NATIVE_ENEMY_COUNT {
            return false;
        }
        (self.doom_native_enemy_alive_mask & (1u16 << index)) != 0
    }

    fn doom_native_enemy_count_alive(&self) -> usize {
        let mut count = 0usize;
        let mut i = 0usize;
        while i < DOOM_NATIVE_ENEMY_COUNT {
            if self.doom_native_enemy_alive(i) {
                count += 1;
            }
            i += 1;
        }
        count
    }

    fn doom_native_relative_angle_units(delta: i32) -> i32 {
        let mut rel = delta.rem_euclid(DOOM_NATIVE_ANGLE_UNITS);
        let half = DOOM_NATIVE_ANGLE_UNITS / 2;
        if rel > half {
            rel -= DOOM_NATIVE_ANGLE_UNITS;
        }
        rel
    }

    fn doom_native_vector_to_angle_units(vec_x: i32, vec_y: i32) -> i32 {
        let mut best_idx = 0usize;
        let mut best_dot = i64::MIN;
        let mut idx = 0usize;
        while idx < DOOM_NATIVE_DIR_COUNT {
            let (dx, dy) = DOOM_NATIVE_DIR_TABLE[idx];
            let dot = (vec_x as i64) * (dx as i64) + (vec_y as i64) * (dy as i64);
            if dot > best_dot {
                best_dot = dot;
                best_idx = idx;
            }
            idx += 1;
        }
        (best_idx as i32) * DOOM_NATIVE_ANGLE_SUBDIV
    }

    fn doom_native_line_of_sight_to(&self, target_x_fp: i32, target_y_fp: i32) -> bool {
        let delta_x = target_x_fp.saturating_sub(self.doom_native_player_x_fp);
        let delta_y = target_y_fp.saturating_sub(self.doom_native_player_y_fp);
        let max_axis = delta_x.abs().max(delta_y.abs()).max(1);
        let steps = (max_axis / (DOOM_NATIVE_FP_ONE / 8).max(1)).clamp(1, 1024);

        let mut i = 1;
        while i < steps {
            let sample_x =
                self.doom_native_player_x_fp.saturating_add((delta_x.saturating_mul(i)) / steps);
            let sample_y =
                self.doom_native_player_y_fp.saturating_add((delta_y.saturating_mul(i)) / steps);
            let cell_x = sample_x >> DOOM_NATIVE_FP_SHIFT;
            let cell_y = sample_y >> DOOM_NATIVE_FP_SHIFT;
            if Self::doom_native_is_wall_cell(cell_x, cell_y) {
                return false;
            }
            i += 1;
        }
        true
    }

    fn doom_native_fire_shot(&mut self) -> Option<usize> {
        let view_angle = self.doom_native_angle_units as i32;
        let (view_dx, view_dy) = Self::doom_native_direction_from_units(view_angle);
        let right_dx = -view_dy;
        let right_dy = view_dx;

        let mut best_idx: Option<usize> = None;
        let mut best_forward = i32::MAX;
        let mut i = 0usize;
        while i < DOOM_NATIVE_ENEMY_COUNT {
            if !self.doom_native_enemy_alive(i) {
                i += 1;
                continue;
            }
            let (ex, ey) = Self::doom_native_enemy_position_fp(i);
            let rel_x = ex.saturating_sub(self.doom_native_player_x_fp);
            let rel_y = ey.saturating_sub(self.doom_native_player_y_fp);
            let forward = ((rel_x * view_dx) + (rel_y * view_dy)) / 256;
            if forward <= DOOM_NATIVE_FP_ONE / 2 {
                i += 1;
                continue;
            }
            let lateral = (((rel_x * right_dx) + (rel_y * right_dy)) / 256).abs();
            if lateral.saturating_mul(5) > forward {
                i += 1;
                continue;
            }
            if !self.doom_native_line_of_sight_to(ex, ey) {
                i += 1;
                continue;
            }
            if forward < best_forward {
                best_forward = forward;
                best_idx = Some(i);
            }
            i += 1;
        }

        if let Some(idx) = best_idx {
            self.doom_native_enemy_alive_mask &= !(1u16 << idx);
            self.doom_native_kills = self.doom_native_kills.saturating_add(1);
        }
        best_idx
    }

    fn doom_native_scale_rgb(color: u32, factor: u32) -> u32 {
        let f = factor.min(255);
        let r = ((color >> 16) & 0xFF) * f / 255;
        let g = ((color >> 8) & 0xFF) * f / 255;
        let b = (color & 0xFF) * f / 255;
        (r << 16) | (g << 8) | b
    }

    fn doom_native_wall_base_color(kind: u8) -> u32 {
        match kind {
            b'B' => 0x8A8A8A,
            b'T' => 0x9C3F2A,
            _ => 0x9A7A4D,
        }
    }

    fn doom_native_wall_texel(kind: u8, tex_u: i32, tex_v: i32, shade: u32, side_vertical: bool) -> u32 {
        let mut factor = 255u32;
        if ((tex_u >> 3) ^ (tex_v >> 3)) & 1 == 1 {
            factor = factor.saturating_sub(34);
        }
        if (tex_u & 7) == 0 || (tex_v & 7) == 0 {
            factor = factor.saturating_sub(22);
        }
        if side_vertical {
            factor = factor.saturating_sub(18);
        }
        let lit = (shade * factor / 255).clamp(24, 255);
        Self::doom_native_scale_rgb(Self::doom_native_wall_base_color(kind), lit)
    }

    fn doom_native_enemy_texel(tex_u: i32, tex_v: i32, shade: u32) -> Option<u32> {
        let dx = tex_u - 16;
        let dy = tex_v - 16;
        if dx * dx + dy * dy > 250 {
            return None;
        }

        let mut color = if dy < -2 { 0xA13E2E } else { 0x7C2A20 };
        if tex_v > 22 {
            color = 0x2A1410;
        }
        if tex_v > 10 && tex_v < 15 && ((tex_u > 9 && tex_u < 13) || (tex_u > 19 && tex_u < 23)) {
            color = 0xF5E29C;
        }
        if tex_v > 15 && tex_v < 19 && tex_u > 13 && tex_u < 19 {
            color = 0x4D1010;
        }
        Some(Self::doom_native_scale_rgb(color, shade))
    }

    fn doom_native_draw_crosshair(&mut self, view: Rect) {
        let cx = view.x + view.width as i32 / 2;
        let cy = view.y + view.height as i32 / 2;
        self.fill_rect(Rect::new(cx - 5, cy, 11, 1), Color(0xE6E6E6));
        self.fill_rect(Rect::new(cx, cy - 5, 1, 11), Color(0xE6E6E6));
    }

    fn doom_native_draw_weapon(&mut self, view: Rect) {
        let bob = (self.doom_native_steps as i32 & 0x7) - 3;
        let center_x = view.x + view.width as i32 / 2;
        let base_y = view.y + view.height as i32 - 36 + bob;

        self.fill_rect(Rect::new(center_x - 26, base_y + 8, 52, 20), Color(0x3D352A));
        self.fill_rect(Rect::new(center_x - 16, base_y, 32, 14), Color(0x655746));
        self.fill_rect(Rect::new(center_x - 6, base_y - 7, 12, 10), Color(0x8B7A63));
        self.fill_rect(Rect::new(center_x - 2, base_y - 16, 4, 10), Color(0xA89A84));

        if self.doom_native_flash_ticks > 0 {
            self.fill_rect(Rect::new(center_x - 9, base_y - 26, 18, 10), Color(0xFFDD66));
            self.fill_rect(Rect::new(center_x - 5, base_y - 33, 10, 7), Color(0xFFAA33));
        }
    }

    fn doom_native_draw_enemies(&mut self, view: Rect, depth_by_col: &[i32]) {
        let draw_w = view.width.max(1) as i32;
        let draw_h = view.height.max(1) as i32;
        let view_angle = self.doom_native_angle_units as i32;
        let (view_dx, view_dy) = Self::doom_native_direction_from_units(view_angle);

        let mut projected: Vec<(i32, i32, i32)> = Vec::new(); // (forward, center_x, sprite_h)
        let mut i = 0usize;
        while i < DOOM_NATIVE_ENEMY_COUNT {
            if self.doom_native_enemy_alive(i) {
                let (enemy_x, enemy_y) = Self::doom_native_enemy_position_fp(i);
                let rel_x = enemy_x.saturating_sub(self.doom_native_player_x_fp);
                let rel_y = enemy_y.saturating_sub(self.doom_native_player_y_fp);
                let forward = ((rel_x * view_dx) + (rel_y * view_dy)) / 256;
                if forward > DOOM_NATIVE_FP_ONE / 2 {
                    let enemy_angle = Self::doom_native_vector_to_angle_units(rel_x, rel_y);
                    let rel_angle = Self::doom_native_relative_angle_units(enemy_angle - view_angle);
                    let half_fov = DOOM_NATIVE_FOV_UNITS / 2;
                    if rel_angle.abs() <= half_fov + 12 {
                        let center_x = view.x + ((rel_angle + half_fov) * draw_w / DOOM_NATIVE_FOV_UNITS);
                        let sprite_h = ((draw_h * 740) / (forward + 220)).clamp(10, draw_h - 4);
                        projected.push((forward, center_x, sprite_h));
                    }
                }
            }
            i += 1;
        }

        projected.sort_by(|a, b| b.0.cmp(&a.0)); // far to near

        for (forward, center_x, sprite_h) in projected.iter() {
            let sprite_w = ((*sprite_h * 3) / 5).max(8);
            let left = *center_x - sprite_w / 2;
            let top = view.y + draw_h / 2 - *sprite_h / 2 + 2;
            let mut sx = 0;
            while sx < sprite_w {
                let screen_x = left + sx;
                if screen_x < view.x || screen_x >= view.x + draw_w {
                    sx += 1;
                    continue;
                }
                let depth_idx = (screen_x - view.x) as usize;
                let wall_depth = depth_by_col
                    .get(depth_idx)
                    .copied()
                    .unwrap_or(i32::MAX);
                if *forward >= wall_depth {
                    sx += 1;
                    continue;
                }
                let tex_u = (sx * 32 / sprite_w).clamp(0, 31);
                let mut sy = 0;
                while sy < *sprite_h {
                    let screen_y = top + sy;
                    if screen_y < view.y || screen_y >= view.y + draw_h {
                        sy += 1;
                        continue;
                    }
                    let tex_v = (sy * 32 / *sprite_h).clamp(0, 31);
                    let shade = (280 - (*forward / 8)).clamp(70, 255) as u32;
                    if let Some(color) = Self::doom_native_enemy_texel(tex_u, tex_v, shade) {
                        self.draw_pixel(screen_x.max(0) as u32, screen_y.max(0) as u32, Color(color));
                    }
                    sy += 1;
                }
                sx += 1;
            }
        }
    }

    fn doom_native_is_wall_cell(cell_x: i32, cell_y: i32) -> bool {
        Self::doom_native_wall_kind(cell_x, cell_y) != b'.'
    }

    fn doom_native_hits_wall_fp(&self, x_fp: i32, y_fp: i32) -> bool {
        let r = DOOM_NATIVE_COLLISION_RADIUS_FP;
        let probes = [
            (x_fp, y_fp),
            (x_fp - r, y_fp),
            (x_fp + r, y_fp),
            (x_fp, y_fp - r),
            (x_fp, y_fp + r),
            (x_fp - r, y_fp - r),
            (x_fp + r, y_fp - r),
            (x_fp - r, y_fp + r),
            (x_fp + r, y_fp + r),
        ];
        for (px, py) in probes {
            let cx = px >> DOOM_NATIVE_FP_SHIFT;
            let cy = py >> DOOM_NATIVE_FP_SHIFT;
            if Self::doom_native_is_wall_cell(cx, cy) {
                return true;
            }
        }
        false
    }

    fn doom_native_try_move(&mut self, delta_x_fp: i32, delta_y_fp: i32) -> bool {
        let mut moved = false;
        let target_x = self.doom_native_player_x_fp.saturating_add(delta_x_fp);
        if !self.doom_native_hits_wall_fp(target_x, self.doom_native_player_y_fp) {
            self.doom_native_player_x_fp = target_x;
            moved = true;
        }
        let target_y = self.doom_native_player_y_fp.saturating_add(delta_y_fp);
        if !self.doom_native_hits_wall_fp(self.doom_native_player_x_fp, target_y) {
            self.doom_native_player_y_fp = target_y;
            moved = true;
        }
        if moved {
            self.doom_native_steps = self.doom_native_steps.saturating_add(1);
        }
        moved
    }

    fn doom_native_turn(&mut self, delta: i32) {
        let next = (self.doom_native_angle_units as i32 + delta).rem_euclid(DOOM_NATIVE_ANGLE_UNITS);
        self.doom_native_angle_units = next as i16;
    }

    fn doom_native_direction_from_units(angle_units: i32) -> (i32, i32) {
        let angle = angle_units.rem_euclid(DOOM_NATIVE_ANGLE_UNITS);
        let base = (angle / DOOM_NATIVE_ANGLE_SUBDIV) as usize;
        let next = (base + 1) % DOOM_NATIVE_DIR_COUNT;
        let frac = angle % DOOM_NATIVE_ANGLE_SUBDIV;
        let inv = DOOM_NATIVE_ANGLE_SUBDIV - frac;
        let (base_dx, base_dy) = DOOM_NATIVE_DIR_TABLE[base];
        let (next_dx, next_dy) = DOOM_NATIVE_DIR_TABLE[next];
        let dx = ((base_dx as i32) * inv + (next_dx as i32) * frac) / DOOM_NATIVE_ANGLE_SUBDIV;
        let dy = ((base_dy as i32) * inv + (next_dy as i32) * frac) / DOOM_NATIVE_ANGLE_SUBDIV;
        (dx, dy)
    }

    fn doom_native_cast_hit(&self, dir_x: i32, dir_y: i32) -> (i32, i32, u8, bool) {
        let mut step_x_fp = (dir_x * DOOM_NATIVE_RAY_STEP_FP) / 256;
        let mut step_y_fp = (dir_y * DOOM_NATIVE_RAY_STEP_FP) / 256;
        if step_x_fp == 0 && dir_x != 0 {
            step_x_fp = if dir_x > 0 { 1 } else { -1 };
        }
        if step_y_fp == 0 && dir_y != 0 {
            step_y_fp = if dir_y > 0 { 1 } else { -1 };
        }

        let mut ray_x_fp = self.doom_native_player_x_fp;
        let mut ray_y_fp = self.doom_native_player_y_fp;
        let mut dist_fp = DOOM_NATIVE_RAY_STEP_FP.max(1);
        let step_len = ((step_x_fp.abs() + step_y_fp.abs()) / 2).max(1);

        let mut step = 0;
        while step < DOOM_NATIVE_MAX_RAY_STEPS {
            let prev_x_fp = ray_x_fp;
            let prev_y_fp = ray_y_fp;
            ray_x_fp = ray_x_fp.saturating_add(step_x_fp);
            ray_y_fp = ray_y_fp.saturating_add(step_y_fp);
            let cell_x = ray_x_fp >> DOOM_NATIVE_FP_SHIFT;
            let cell_y = ray_y_fp >> DOOM_NATIVE_FP_SHIFT;
            let wall_kind = Self::doom_native_wall_kind(cell_x, cell_y);
            if wall_kind != b'.' {
                let prev_cell_x = prev_x_fp >> DOOM_NATIVE_FP_SHIFT;
                let prev_cell_y = prev_y_fp >> DOOM_NATIVE_FP_SHIFT;
                let side_vertical = cell_x != prev_cell_x;
                let tex_src_fp = if side_vertical && cell_y == prev_cell_y {
                    ray_y_fp
                } else {
                    ray_x_fp
                };
                let tex_u = (((tex_src_fp & (DOOM_NATIVE_FP_ONE - 1)) * 64) / DOOM_NATIVE_FP_ONE)
                    .clamp(0, 63);
                return (dist_fp.max(1), tex_u, wall_kind, side_vertical);
            }
            dist_fp = dist_fp.saturating_add(step_len);
            step += 1;
        }

        (
            (DOOM_NATIVE_MAX_RAY_STEPS * DOOM_NATIVE_RAY_STEP_FP).max(1),
            0,
            b'#',
            false,
        )
    }

    fn doom_native_draw_scene(&mut self, canvas: Rect) {
        let mut view =
            Rect::new(canvas.x + 8, canvas.y + 8, canvas.width.saturating_sub(16), canvas.height.saturating_sub(16));
        if view.width < 32 || view.height < 24 {
            view = canvas;
        }

        let draw_w = view.width.max(1) as i32;
        let draw_h = view.height.max(1) as i32;
        let view_angle_units = self.doom_native_angle_units as i32;
        let (view_dx, view_dy) = Self::doom_native_direction_from_units(view_angle_units);
        let fov_half = DOOM_NATIVE_FOV_UNITS / 2;
        let mid = draw_h / 2;
        let mut depth_by_col: Vec<i32> = alloc::vec![i32::MAX; draw_w as usize];

        let mut y = 0;
        while y < draw_h {
            if y < mid {
                let fog = (y * 120 / mid.max(1)).clamp(0, 120) as u32;
                let base = 0x1E1D2A + ((fog / 4) << 8) + (fog / 5);
                let line = if (y & 7) == 0 {
                    Self::doom_native_scale_rgb(base, 112)
                } else {
                    base
                };
                self.fill_rect(Rect::new(view.x, view.y + y, view.width, 1), Color(line));
            } else {
                let rel = ((y - mid) * 255 / (draw_h - mid).max(1)).clamp(0, 255) as u32;
                let base = 0x3B281D;
                let shade = (190u32.saturating_sub(rel / 2)).clamp(70, 210);
                let line = if (y & 3) == 0 {
                    Self::doom_native_scale_rgb(base, shade.saturating_sub(16))
                } else {
                    Self::doom_native_scale_rgb(base, shade)
                };
                self.fill_rect(Rect::new(view.x, view.y + y, view.width, 1), Color(line));
            }
            y += 1;
        }

        let mut col = 0;
        while col < draw_w {
            let rel_units = ((col * DOOM_NATIVE_FOV_UNITS) / draw_w) - fov_half;
            let ray_angle_units = view_angle_units + rel_units;
            let (ray_dx, ray_dy) = Self::doom_native_direction_from_units(ray_angle_units);
            let (raw_dist_fp, tex_u, wall_kind, side_vertical) =
                self.doom_native_cast_hit(ray_dx, ray_dy);
            let mut dist_fp = raw_dist_fp.max(1);

            // Fisheye correction using the cosine between forward dir and ray dir.
            let dot = (((view_dx * ray_dx) + (view_dy * ray_dy)) / 256).clamp(24, 256);
            dist_fp = (dist_fp * dot / 256).max(1);
            depth_by_col[col as usize] = dist_fp;

            let wall_h = ((draw_h * 960) / (dist_fp + 180)).clamp(2, draw_h);
            let wall_top = view.y + ((draw_h - wall_h) / 2);
            let shade = (260 - dist_fp / 12).clamp(36, 255) as u32;
            let mut py = 0;
            while py < wall_h {
                let tex_v = (py * 64 / wall_h).clamp(0, 63);
                let color = Self::doom_native_wall_texel(wall_kind, tex_u, tex_v, shade, side_vertical);
                self.draw_pixel(
                    (view.x + col).max(0) as u32,
                    (wall_top + py).max(0) as u32,
                    Color(color),
                );
                py += 1;
            }
            col += 1;
        }

        self.doom_native_draw_enemies(view, depth_by_col.as_slice());
        self.doom_native_draw_crosshair(view);
        self.doom_native_draw_weapon(view);

        if self.doom_native_flash_ticks > 0 {
            self.doom_native_flash_ticks -= 1;
        }

        self.draw_border(view, Color(0x4E687C));
        self.fill_rect(Rect::new(view.x, view.y + (view.height as i32 / 2), view.width, 1), Color(0x71503C));
    }

    fn doom_native_draw_minimap(&mut self, canvas: Rect) {
        let cell_px = 5;
        let map_w_px = (DOOM_NATIVE_MAP_W as i32) * cell_px;
        let map_h_px = (DOOM_NATIVE_MAP_H as i32) * cell_px;
        let map_x = canvas.x + canvas.width as i32 - map_w_px - 10;
        let map_y = canvas.y + 10;
        let map_rect = Rect::new(map_x - 2, map_y - 2, (map_w_px + 4) as u32, (map_h_px + 4) as u32);
        self.fill_rect(map_rect, Color(0x111820));
        self.draw_border(map_rect, Color(0x55708A));

        let mut y = 0usize;
        while y < DOOM_NATIVE_MAP_H {
            let mut x = 0usize;
            while x < DOOM_NATIVE_MAP_W {
                let kind = DOOM_NATIVE_MAP[y][x];
                let color = if kind == b'.' {
                    0x203442
                } else {
                    Self::doom_native_scale_rgb(Self::doom_native_wall_base_color(kind), 200)
                };
                self.fill_rect(
                    Rect::new(map_x + (x as i32) * cell_px, map_y + (y as i32) * cell_px, cell_px as u32, cell_px as u32),
                    Color(color),
                );
                x += 1;
            }
            y += 1;
        }

        let px = map_x
            + (((self.doom_native_player_x_fp * cell_px) / DOOM_NATIVE_FP_ONE) as i32)
                .clamp(0, map_w_px - 1);
        let py = map_y
            + (((self.doom_native_player_y_fp * cell_px) / DOOM_NATIVE_FP_ONE) as i32)
                .clamp(0, map_h_px - 1);
        self.fill_rect(Rect::new(px - 1, py - 1, 3, 3), Color(0xFF4040));

        let (dx, dy) = Self::doom_native_direction_from_units(self.doom_native_angle_units as i32);
        let line_x = px + (((dx as i32) * 6) / 256);
        let line_y = py + (((dy as i32) * 6) / 256);
        self.fill_rect(Rect::new(line_x - 1, line_y - 1, 3, 3), Color(0xFFD68A));

        let mut i = 0usize;
        while i < DOOM_NATIVE_ENEMY_COUNT {
            if self.doom_native_enemy_alive(i) {
                let (enemy_x_fp, enemy_y_fp) = Self::doom_native_enemy_position_fp(i);
                let ex = map_x
                    + (((enemy_x_fp * cell_px) / DOOM_NATIVE_FP_ONE) as i32)
                        .clamp(0, map_w_px - 1);
                let ey = map_y
                    + (((enemy_y_fp * cell_px) / DOOM_NATIVE_FP_ONE) as i32)
                        .clamp(0, map_h_px - 1);
                self.fill_rect(Rect::new(ex - 1, ey - 1, 3, 3), Color(0xFF7A4A));
            }
            i += 1;
        }
    }

    fn doom_native_draw_hud(&mut self, canvas: Rect) {
        let pos_x = self.doom_native_player_x_fp >> DOOM_NATIVE_FP_SHIFT;
        let pos_y = self.doom_native_player_y_fp >> DOOM_NATIVE_FP_SHIFT;
        let angle_deg = ((self.doom_native_angle_units as i32 * 360) / DOOM_NATIVE_ANGLE_UNITS).rem_euclid(360);
        let hud = alloc::format!(
            "POS {}:{} DIR {}deg SHOTS {} KILLS {} LEFT {}",
            pos_x,
            pos_y,
            angle_deg,
            self.doom_native_shots,
            self.doom_native_kills,
            self.doom_native_enemy_count_alive()
        );
        let trimmed = Self::trim_label(hud.as_str(), 58);
        self.draw_text(
            (canvas.x + 10).max(0) as u32,
            (canvas.y + canvas.height as i32 - 18).max(0) as u32,
            trimmed.as_bytes(),
            Color(0xE5F2FF),
        );
    }

    pub fn doom_native_running(&self) -> bool {
        self.kind == WindowKind::DoomLauncher && self.doom_native_running
    }

    pub fn start_doom_native_session(&mut self) {
        if self.kind != WindowKind::DoomLauncher {
            return;
        }
        self.doom_native_reset_state();
        self.doom_status = String::from("CPP-DOOM nativo activo. WASD/flechas, Q/E, SPACE, ESC.");
        self.render();
    }

    pub fn stop_doom_native_session(&mut self) {
        if self.kind != WindowKind::DoomLauncher {
            return;
        }
        self.doom_native_running = false;
        self.doom_status = String::from("Sesion CPP-DOOM cerrada. Click INICIAR para volver.");
        self.render();
    }

    pub fn doom_native_handle_input(&mut self, key: Option<char>, special: Option<SpecialKey>) -> bool {
        if self.kind != WindowKind::DoomLauncher {
            return false;
        }

        if matches!(key, Some('\x1b')) {
            self.stop_doom_native_session();
            return true;
        }

        if !self.doom_native_running {
            if matches!(key, Some('\n') | Some('\r') | Some(' ')) {
                self.start_doom_native_session();
                return true;
            }
            return false;
        }

        let mut handled = false;
        let mut moved = false;
        let mut status_override: Option<String> = None;

        if let Some(sp) = special {
            match sp {
                SpecialKey::Up => {
                    let (dx, dy) = Self::doom_native_direction_from_units(self.doom_native_angle_units as i32);
                    moved = self.doom_native_try_move(
                        (dx * DOOM_NATIVE_MOVE_STEP_FP) / 256,
                        (dy * DOOM_NATIVE_MOVE_STEP_FP) / 256,
                    );
                    handled = true;
                }
                SpecialKey::Down => {
                    let (dx, dy) = Self::doom_native_direction_from_units(self.doom_native_angle_units as i32);
                    moved = self.doom_native_try_move(
                        -(dx * DOOM_NATIVE_MOVE_STEP_FP) / 256,
                        -(dy * DOOM_NATIVE_MOVE_STEP_FP) / 256,
                    );
                    handled = true;
                }
                SpecialKey::Left => {
                    self.doom_native_turn(-DOOM_NATIVE_TURN_UNITS);
                    handled = true;
                }
                SpecialKey::Right => {
                    self.doom_native_turn(DOOM_NATIVE_TURN_UNITS);
                    handled = true;
                }
            }
        }

        if let Some(ch) = key {
            match ch {
                'w' | 'W' => {
                    let (dx, dy) = Self::doom_native_direction_from_units(self.doom_native_angle_units as i32);
                    moved = self.doom_native_try_move(
                        (dx * DOOM_NATIVE_MOVE_STEP_FP) / 256,
                        (dy * DOOM_NATIVE_MOVE_STEP_FP) / 256,
                    );
                    handled = true;
                }
                's' | 'S' => {
                    let (dx, dy) = Self::doom_native_direction_from_units(self.doom_native_angle_units as i32);
                    moved = self.doom_native_try_move(
                        -(dx * DOOM_NATIVE_MOVE_STEP_FP) / 256,
                        -(dy * DOOM_NATIVE_MOVE_STEP_FP) / 256,
                    );
                    handled = true;
                }
                'a' | 'A' => {
                    let (dx, dy) = Self::doom_native_direction_from_units(
                        self.doom_native_angle_units as i32 - (DOOM_NATIVE_ANGLE_UNITS / 4),
                    );
                    moved = self.doom_native_try_move(
                        (dx * DOOM_NATIVE_STRAFE_STEP_FP) / 256,
                        (dy * DOOM_NATIVE_STRAFE_STEP_FP) / 256,
                    );
                    handled = true;
                }
                'd' | 'D' => {
                    let (dx, dy) = Self::doom_native_direction_from_units(
                        self.doom_native_angle_units as i32 + (DOOM_NATIVE_ANGLE_UNITS / 4),
                    );
                    moved = self.doom_native_try_move(
                        (dx * DOOM_NATIVE_STRAFE_STEP_FP) / 256,
                        (dy * DOOM_NATIVE_STRAFE_STEP_FP) / 256,
                    );
                    handled = true;
                }
                'q' | 'Q' => {
                    self.doom_native_turn(-DOOM_NATIVE_TURN_UNITS);
                    handled = true;
                }
                'e' | 'E' => {
                    self.doom_native_turn(DOOM_NATIVE_TURN_UNITS);
                    handled = true;
                }
                'r' | 'R' => {
                    self.start_doom_native_session();
                    return true;
                }
                ' ' => {
                    self.doom_native_shots = self.doom_native_shots.saturating_add(1);
                    self.doom_native_flash_ticks = 2;
                    if self.doom_native_fire_shot().is_some() {
                        if self.doom_native_enemy_count_alive() == 0 {
                            status_override = Some(alloc::format!(
                                "Impacto confirmado. Area limpia! kills={} shots={}",
                                self.doom_native_kills,
                                self.doom_native_shots
                            ));
                        } else {
                            status_override = Some(alloc::format!(
                                "Impacto confirmado. Enemigos restantes {}",
                                self.doom_native_enemy_count_alive()
                            ));
                        }
                    } else {
                        status_override = Some(alloc::format!(
                            "BANG! sin impacto (shots={})",
                            self.doom_native_shots
                        ));
                    }
                    handled = true;
                }
                _ => {}
            }
        }

        if !handled {
            return false;
        }

        if let Some(msg) = status_override {
            self.doom_status = msg;
        } else if moved {
            self.doom_status = String::from("CPP-DOOM nativo activo. WASD/flechas, Q/E, SPACE, ESC.");
        } else {
            self.doom_status = String::from("CPP-DOOM nativo activo.");
        }
        self.render();
        true
    }

    fn linux_bridge_canvas_rect(&self) -> Rect {
        let y = LINUX_BRIDGE_TOP_H;
        let h = (self.content_height() - y - LINUX_BRIDGE_STATUS_H).max(0) as u32;
        Rect::new(0, y, self.rect.width, h)
    }

    fn linux_bridge_status_rect(&self) -> Rect {
        let y = (self.content_height() - LINUX_BRIDGE_STATUS_H).max(0);
        Rect::new(0, y, self.rect.width, LINUX_BRIDGE_STATUS_H as u32)
    }

    fn browser_text_max_cols(&self) -> usize {
        let view = self.browser_viewport_rect();
        let usable = (view.width as i32 - 28).max(6);
        (usable / 6).max(1) as usize
    }

    fn browser_visible_rows(&self) -> usize {
        let view = self.browser_viewport_rect();
        ((view.height as i32 - 8) / 10).max(1) as usize
    }

    fn browser_flat_lines(&self) -> Vec<String> {
        let max_cols = self.browser_text_max_cols();
        let mut flat: Vec<String> = Vec::new();
        for line in self.browser_content_lines.iter() {
            if line.trim().is_empty() {
                if !flat
                    .last()
                    .map(|s: &String| s.is_empty())
                    .unwrap_or(false)
                {
                    flat.push(String::new());
                }
                continue;
            }

            let wrapped = Self::wrap_text_lines(line.as_str(), max_cols, 256);
            if wrapped.is_empty() {
                if !flat
                    .last()
                    .map(|s: &String| s.is_empty())
                    .unwrap_or(false)
                {
                    flat.push(String::new());
                }
                continue;
            }

            for w in wrapped {
                flat.push(w);
            }
        }

        if flat.is_empty() {
            flat.push(String::from("(sin contenido)"));
        }

        flat
    }

    fn browser_max_scroll(&self) -> usize {
        let flat = self.browser_flat_lines();
        flat.len().saturating_sub(self.browser_visible_rows())
    }

    fn normalize_link_candidate(token: &str) -> Option<String> {
        let clean = token.trim_matches(|c: char| {
            matches!(c, '<' | '>' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';')
        });
        if clean.is_empty() {
            return None;
        }

        let lower = clean
            .bytes()
            .map(|b| (b as char).to_ascii_lowercase())
            .collect::<String>();

        if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("redux://") {
            return Some(String::from(clean));
        }
        None
    }

    fn extract_first_url_from_text(text: &str) -> Option<String> {
        if let Some(start) = text.find('<') {
            if let Some(end_rel) = text[start + 1..].find('>') {
                let end = start + 1 + end_rel;
                if end > start + 1 {
                    let inside = &text[start + 1..end];
                    if let Some(url) = Self::normalize_link_candidate(inside) {
                        return Some(url);
                    }
                }
            }
        }

        for token in text.split_whitespace() {
            if let Some(url) = Self::normalize_link_candidate(token) {
                return Some(url);
            }
        }
        None
    }

    fn trim_label(text: &str, max_chars: usize) -> String {
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

    pub fn draw_border(&mut self, rect: Rect, color: Color) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }

        self.fill_rect(Rect::new(rect.x, rect.y, rect.width, 1), color);
        self.fill_rect(
            Rect::new(rect.x, rect.y + rect.height as i32 - 1, rect.width, 1),
            color,
        );
        self.fill_rect(Rect::new(rect.x, rect.y, 1, rect.height), color);
        self.fill_rect(
            Rect::new(rect.x + rect.width as i32 - 1, rect.y, 1, rect.height),
            color,
        );
    }

    fn draw_explorer_icon(&mut self, rect: Rect, kind: ExplorerItemKind) {
        match kind {
            ExplorerItemKind::ShortcutUsb | ExplorerItemKind::ShortcutVolume => {
                self.fill_rect(
                    Rect::new(rect.x + 10, rect.y + 22, rect.width - 20, rect.height - 26),
                    Color(0x7E8C99),
                );
                self.fill_rect(
                    Rect::new(rect.x + 14, rect.y + 26, rect.width - 28, rect.height - 34),
                    Color(0xC3CCD4),
                );
                self.fill_rect(
                    Rect::new(rect.x + rect.width as i32 - 18, rect.y + rect.height as i32 - 14, 4, 4),
                    Color(0x4ACA6D),
                );
            }
            ExplorerItemKind::File 
            | ExplorerItemKind::ShortcutReduxStudio 
            | ExplorerItemKind::FileExecutable
            | ExplorerItemKind::FileImage
            | ExplorerItemKind::FileAudio
            | ExplorerItemKind::FileVideo
            | ExplorerItemKind::FileArchive
            | ExplorerItemKind::FileCode
            | ExplorerItemKind::FileText => {
                // Base document shape
                self.fill_rect(
                    Rect::new(rect.x + 14, rect.y + 8, rect.width - 28, rect.height - 16),
                    Color(0xFFFFFF),
                );
                self.draw_border(
                    Rect::new(rect.x + 14, rect.y + 8, rect.width - 28, rect.height - 16),
                    Color(0xA5B2BF),
                );
                self.fill_rect(
                    Rect::new(rect.x + rect.width as i32 - 24, rect.y + 8, 10, 10),
                    Color(0xE1E7EE),
                );

                // Color accent per file type
                let cx = rect.x + rect.width as i32 / 2;
                let cy = rect.y + rect.height as i32 / 2;
                match kind {
                    ExplorerItemKind::FileExecutable | ExplorerItemKind::FileArchive => {
                        // Package/box accent — brown bar
                        self.fill_rect(Rect::new(cx - 8, cy - 6, 16, 12), Color(0xD4A373));
                        self.fill_rect(Rect::new(cx - 1, cy - 6, 2, 12), Color(0xFAEDCD));
                        self.fill_rect(Rect::new(cx - 8, cy - 1, 16, 2), Color(0xFAEDCD));
                    }
                    ExplorerItemKind::FileImage => {
                        // Mini landscape — sky + mountain
                        self.fill_rect(Rect::new(cx - 8, cy - 6, 16, 12), Color(0x87CEEB));
                        self.fill_rect(Rect::new(cx - 6, cy - 4, 3, 3), Color(0xFFE082)); // Sun
                        self.fill_rect(Rect::new(cx - 6, cy + 2, 12, 4), Color(0x2E7D32)); // Mountain
                    }
                    ExplorerItemKind::FileAudio => {
                        // Music note accent
                        self.fill_rect(Rect::new(cx - 1, cy - 6, 2, 10), Color(0x1A1A1A));
                        self.fill_rect(Rect::new(cx - 4, cy + 1, 5, 4), Color(0x1A1A1A));
                        self.fill_rect(Rect::new(cx - 1, cy - 6, 8, 2), Color(0x1A1A1A));
                    }
                    ExplorerItemKind::FileVideo => {
                        // Film strip accent
                        self.fill_rect(Rect::new(cx - 8, cy - 6, 16, 12), Color(0x333333));
                        self.fill_rect(Rect::new(cx - 5, cy - 3, 10, 6), Color(0x666666));
                        self.fill_rect(Rect::new(cx - 1, cy - 2, 4, 4), Color(0xFFFFFF)); // Play
                    }
                    ExplorerItemKind::FileCode => {
                        // Brackets accent
                        self.fill_rect(Rect::new(cx - 8, cy - 2, 3, 4), Color(0x2196F3)); // <
                        self.fill_rect(Rect::new(cx - 1, cy - 4, 2, 8), Color(0xFF9800)); // /
                        self.fill_rect(Rect::new(cx + 5, cy - 2, 3, 4), Color(0x2196F3)); // >
                    }
                    ExplorerItemKind::FileText => {
                        // Text lines accent
                        self.fill_rect(Rect::new(cx - 6, cy - 5, 12, 2), Color(0xAAAAAA));
                        self.fill_rect(Rect::new(cx - 6, cy - 1, 10, 2), Color(0xAAAAAA));
                        self.fill_rect(Rect::new(cx - 6, cy + 3, 8, 2), Color(0xAAAAAA));
                    }
                    ExplorerItemKind::ShortcutReduxStudio => {
                        // IDE accent — colored circles
                        self.fill_rect(Rect::new(cx - 5, cy - 4, 4, 4), Color(0xEA4C89));
                        self.fill_rect(Rect::new(cx + 1, cy - 4, 4, 4), Color(0xF9C02D));
                        self.fill_rect(Rect::new(cx - 2, cy, 4, 4), Color(0x0288D1));
                    }
                    _ => {} // Plain file — no emblem
                }
            }
            ExplorerItemKind::Home => {
                self.fill_rect(
                    Rect::new(rect.x + 16, rect.y + 22, rect.width - 32, rect.height - 24),
                    Color(0x8DB7E2),
                );
                self.fill_rect(
                    Rect::new(rect.x + 26, rect.y + 32, rect.width - 52, rect.height - 36),
                    Color(0xDCEEFF),
                );
                self.fill_rect(Rect::new(rect.x + 34, rect.y + 40, 12, 14), Color(0x6F95B7));
            }
            ExplorerItemKind::Up => {
                self.fill_rect(
                    Rect::new(rect.x + 10, rect.y + 18, rect.width - 20, rect.height - 22),
                    Color(0xD2DCE7),
                );
                let mid = rect.x + rect.width as i32 / 2;
                self.fill_rect(Rect::new(mid - 2, rect.y + 16, 4, 30), Color(0x2D4A63));
                self.fill_rect(Rect::new(mid - 8, rect.y + 22, 16, 4), Color(0x2D4A63));
            }
            ExplorerItemKind::ShortcutDesktop
            | ExplorerItemKind::ShortcutDownloads
            | ExplorerItemKind::ShortcutDocuments
            | ExplorerItemKind::ShortcutImages
            | ExplorerItemKind::ShortcutVideos
            | ExplorerItemKind::Directory => {
                self.fill_rect(
                    Rect::new(rect.x + 10, rect.y + 20, rect.width - 20, rect.height - 24),
                    Color(0xF0C86E),
                );
                self.fill_rect(Rect::new(rect.x + 16, rect.y + 12, 24, 10), Color(0xF7D88E));
                self.draw_border(
                    Rect::new(rect.x + 10, rect.y + 20, rect.width - 20, rect.height - 24),
                    Color(0xCFA24A),
                );
            }
            ExplorerItemKind::ShortcutRecycleBin => {
                // Draw a recycle bin icon
                self.fill_rect(
                    Rect::new(rect.x + 14, rect.y + 16, rect.width - 28, rect.height - 20),
                    Color(0x9EACBA),
                );
                self.fill_rect(
                    Rect::new(rect.x + 12, rect.y + 12, rect.width - 24, 4),
                    Color(0x76899E),
                );
                self.fill_rect(
                    Rect::new(rect.x + rect.width as i32 / 2 - 4, rect.y + 8, 8, 4),
                    Color(0x76899E),
                );
            }
        }
    }

    fn wrap_text_lines(text: &str, max_cols: usize, max_lines: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut line = String::new();

        for ch in text.chars() {
            if ch == '\r' {
                continue;
            }

            if ch == '\n' {
                out.push(line.clone());
                line.clear();
                if out.len() >= max_lines {
                    return out;
                }
                continue;
            }

            line.push(ch);
            if line.len() >= max_cols {
                out.push(line.clone());
                line.clear();
                if out.len() >= max_lines {
                    return out;
                }
            }
        }

        if out.len() < max_lines {
            out.push(line);
        }

        if out.is_empty() {
            out.push(String::new());
        }

        out
    }

    pub fn draw_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.rect.width && y < self.rect.height {
            let idx = (y * self.rect.width + x) as usize;
            self.buffer[idx] = color.0;
        }
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let start_x = rect.x.max(0) as u32;
        let start_y = rect.y.max(0) as u32;
        let end_x = (rect.x + rect.width as i32).min(self.rect.width as i32) as u32;
        let end_y = (rect.y + rect.height as i32).min(self.rect.height as i32) as u32;

        for dy in start_y..end_y {
            for dx in start_x..end_x {
                self.draw_pixel(dx, dy, color);
            }
        }
    }

    pub fn draw_char(&mut self, x: u32, y: u32, ch: char, color: Color) {
        let glyph = crate::font::glyph_5x7(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                let mask = 1 << (4 - col);
                if (bits & mask) != 0 {
                    self.draw_pixel(x + col as u32, y + row as u32, color);
                }
            }
        }
    }

    pub fn draw_text(&mut self, x: u32, y: u32, text: &[u8], color: Color) {
        let mut cx = x;
        let mut cy = y;
        for &b in text {
            if b == b'\n' {
                cx = x;
                cy += 8;
                continue;
            }
            let ch = if b.is_ascii() { b as char } else { '?' };
            self.draw_char(cx, cy, ch, color);
            cx += 6;
        }
    }

    pub fn minimize(&mut self) {
        self.state = WindowState::Minimized;
    }

    pub fn maximize(&mut self, screen_width: usize, screen_height: usize) {
        if self.state == WindowState::Maximized {
            self.rect = self.saved_rect;
            self.resize_buffer(self.rect.width, self.rect.height);
            self.state = WindowState::Normal;
        } else {
            self.saved_rect = self.rect;
            self.rect = Rect::new(0, 0, screen_width as u32, (screen_height - 40) as u32);
            self.resize_buffer(self.rect.width, self.rect.height);
            self.state = WindowState::Maximized;
        }
        self.controls = WindowControls::new(self.rect.x, self.rect.y, self.rect.width);
        self.render();
    }

    pub fn resize_buffer(&mut self, width: u32, height: u32) {
        let size = (width * height) as usize;
        self.buffer.resize(size, 0xFFFFFFFF);
    }

    pub fn restore(&mut self) {
        if self.state == WindowState::Minimized {
            self.state = WindowState::Normal;
        } else if self.state == WindowState::Maximized {
            self.rect = self.saved_rect;
            self.resize_buffer(self.rect.width, self.rect.height);
            self.controls = WindowControls::new(self.rect.x, self.rect.y, self.rect.width);
            self.state = WindowState::Normal;
            self.render();
        }
    }

    pub fn close(&mut self) {
        self.state = WindowState::Closed;
    }

    pub fn is_terminal(&self) -> bool {
        self.kind == WindowKind::Terminal
    }

    pub fn is_explorer(&self) -> bool {
        self.kind == WindowKind::Explorer
    }

    pub fn is_notepad(&self) -> bool {
        self.kind == WindowKind::Notepad
    }

    pub fn is_search(&self) -> bool {
        self.kind == WindowKind::Search
    }

    pub fn is_browser(&self) -> bool {
        self.kind == WindowKind::Browser
    }

    pub fn is_image_viewer(&self) -> bool {
        self.kind == WindowKind::ImageViewer
    }

    pub fn is_app_runner(&self) -> bool {
        self.kind == WindowKind::AppRunner
    }

    pub fn is_ide_studio(&self) -> bool {
        self.kind == WindowKind::IdeStudio
    }

    pub fn is_doom_launcher(&self) -> bool {
        self.kind == WindowKind::DoomLauncher
    }

    pub fn is_linux_bridge(&self) -> bool {
        self.kind == WindowKind::LinuxBridge
    }

    pub fn is_settings(&self) -> bool {
        self.kind == WindowKind::Settings
    }

    pub fn is_media_player(&self) -> bool {
        self.kind == WindowKind::MediaPlayer || self.kind == WindowKind::VideoPlayer
    }

    pub fn is_wifi_manager(&self) -> bool {
        self.kind == WindowKind::WifiManager
    }

    pub fn is_task_manager(&self) -> bool {
        self.kind == WindowKind::TaskManager
    }

    pub fn title_bar_contains(&self, x: i32, y: i32) -> bool {
        let bar = Rect::new(self.rect.x, self.rect.y, self.rect.width, TITLE_BAR_H as u32);
        bar.contains(crate::gui::Point { x, y })
    }

    pub fn resize_grip_contains(&self, x: i32, y: i32) -> bool {
        let grip = WINDOW_RESIZE_GRIP;
        let gx = (self.rect.x + self.rect.width as i32 - grip).max(self.rect.x);
        let gy = (self.rect.y + self.rect.height as i32 - grip).max(self.rect.y);
        Rect::new(gx, gy, grip as u32, grip as u32).contains(crate::gui::Point { x, y })
    }

    pub fn min_dimensions(&self) -> (u32, u32) {
        match self.kind {
            WindowKind::Terminal => (420, 260),
            WindowKind::Explorer => (620, 380),
            WindowKind::Notepad => (520, 320),
            WindowKind::Search => (560, 360),
            WindowKind::Browser => (600, 400),
            WindowKind::ImageViewer => (520, 360),
            WindowKind::AppRunner => (560, 380),
            WindowKind::IdeStudio => (680, 420),
            WindowKind::DoomLauncher => (520, 340),
            WindowKind::LinuxBridge => (560, 380),
            WindowKind::Settings => (480, 360),
            WindowKind::MediaPlayer => (400, 280),
            WindowKind::VideoPlayer => (640, 480),
            WindowKind::WifiManager => (420, 460),
            WindowKind::TaskManager => (520, 360),
        }
    }

    pub fn move_to(&mut self, x: i32, y: i32) {
        self.rect.x = x;
        self.rect.y = y;
        self.controls = WindowControls::new(self.rect.x, self.rect.y, self.rect.width);
    }

    pub fn resize_to(&mut self, width: u32, height: u32) {
        if self.rect.width == width && self.rect.height == height {
            return;
        }

        self.rect.width = width;
        self.rect.height = height;
        self.resize_buffer(width, height);
        self.controls = WindowControls::new(self.rect.x, self.rect.y, self.rect.width);
        self.render();
    }
    pub fn hit_test_controls(&self, x: i32, y: i32) -> Option<&str> {
        use crate::gui::Point;
        let p = Point { x, y };

        if self.controls.close_btn.contains(p) {
            return Some("close");
        }
        if self.controls.maximize_btn.contains(p) {
            return Some("maximize");
        }
        if self.controls.minimize_btn.contains(p) {
            return Some("minimize");
        }
        None
    }

    pub fn render(&mut self) {
        match self.kind {
            WindowKind::Terminal => self.render_terminal(),
            WindowKind::Explorer => self.render_explorer(),
            WindowKind::Notepad => self.render_notepad(),
            WindowKind::Search => self.render_search(),
            WindowKind::Browser => self.render_browser(),
            WindowKind::ImageViewer => self.render_image_viewer(),
            WindowKind::AppRunner => self.render_app_runner(),
            WindowKind::IdeStudio => self.render_ide_studio(),
            WindowKind::DoomLauncher => self.render_doom_launcher(),
            WindowKind::LinuxBridge => self.render_linux_bridge(),
            WindowKind::Settings => self.render_settings(),
            WindowKind::MediaPlayer => self.render_media_player(),
            WindowKind::VideoPlayer => self.render_video_player(),
            WindowKind::WifiManager => self.render_wifi_manager(),
            WindowKind::TaskManager => self.render_task_manager(),
        }
    }

    pub fn render_terminal(&mut self) {
        if self.kind != WindowKind::Terminal {
            return;
        }

        self.buffer.fill(0xFFFFFFFF);

        let max_scroll = self.terminal_max_scroll();
        self.terminal_scroll = self.terminal_scroll.min(max_scroll);

        let wrapped_lines = self.terminal_wrapped_output_lines();
        let visible_rows = self.terminal_output_visible_rows();
        let end_idx = wrapped_lines.len().saturating_sub(self.terminal_scroll);
        let start_idx = end_idx.saturating_sub(visible_rows);
        let visible = &wrapped_lines[start_idx..end_idx];

        let mut y = TERMINAL_TOP_PADDING;
        for line in visible.iter() {
            self.draw_text(TERMINAL_TEXT_X as u32, y as u32, line.as_bytes(), Color(0x000000));
            y += TERMINAL_LINE_HEIGHT;
        }

        if self.terminal_scroll > 0 {
            let marker = alloc::format!("^ +{} lineas", self.terminal_scroll);
            self.draw_text(10, 1, marker.as_bytes(), Color(0x666666));
        }

        let prompt_path = self.current_path.clone();
        let prompt_tail = "> ";
        let input_clone = self.input_buffer.clone();

        self.draw_text(TERMINAL_TEXT_X as u32, y as u32, prompt_path.as_bytes(), Color(0x0066CC));
        let prompt_w = prompt_path.len() * TERMINAL_CHAR_W;
        self.draw_text((TERMINAL_TEXT_X + prompt_w) as u32, y as u32, prompt_tail.as_bytes(), Color(0x0066CC));

        let total_prompt_w = prompt_w + (prompt_tail.len() * TERMINAL_CHAR_W);
        self.draw_text(
            (TERMINAL_TEXT_X + total_prompt_w) as u32,
            y as u32,
            input_clone.as_bytes(),
            Color(0x000000),
        );

        let cursor_x = TERMINAL_TEXT_X + total_prompt_w + (input_clone.len() * TERMINAL_CHAR_W);
        self.draw_text(cursor_x as u32, y as u32, b"_", Color(0x000000));
        self.cursor_x = cursor_x;
    }

    pub fn render_explorer(&mut self) {
        if self.kind != WindowKind::Explorer {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0xEAF1F8));
        self.fill_rect(Rect::new(0, 0, self.rect.width, EXPLORER_TOP_H as u32), Color(0xD3E1EF));

        self.draw_text(10, 8, b"FILE EXPLORER", Color(0x1D354A));
        let query_rect = self.explorer_search_query_rect();
        let button_rect = self.explorer_search_button_rect();

        self.fill_rect(query_rect, Color(0xFFFFFF));
        self.draw_border(
            query_rect,
            if self.explorer_search_input_active {
                Color(0x2A6FAE)
            } else {
                Color(0x8FA8C4)
            },
        );
        let query_trim = Self::trim_label(
            self.explorer_search_query.as_str(),
            ((query_rect.width as usize) / 6).saturating_sub(2),
        );
        if query_trim.is_empty() {
            self.draw_text(
                (query_rect.x + 6) as u32,
                (query_rect.y + 7) as u32,
                b"buscar...",
                Color(0x6C7C8C),
            );
        } else {
            self.draw_text(
                (query_rect.x + 6) as u32,
                (query_rect.y + 7) as u32,
                query_trim.as_bytes(),
                Color(0x1F3650),
            );
        }
        if self.explorer_search_input_active {
            let caret_x = (query_rect.x + 6 + query_trim.len() as i32 * 6)
                .min(query_rect.x + query_rect.width as i32 - 8)
                .max(query_rect.x + 6);
            self.draw_text(caret_x as u32, (query_rect.y + 7) as u32, b"_", Color(0x1F3650));
        }

        self.fill_rect(
            button_rect,
            if self.explorer_search_query.trim().is_empty() {
                Color(0x4F91C7)
            } else {
                Color(0x2D89D6)
            },
        );
        self.draw_border(button_rect, Color(0x1E5E95));
        self.draw_text(
            (button_rect.x + 11) as u32,
            (button_rect.y + 7) as u32,
            b"Buscar",
            Color(0xFFFFFF),
        );

        let path_max_chars = ((query_rect.x - 10).max(56) as usize / 6).max(9);
        let path_text = Self::trim_label(self.explorer_path.as_str(), path_max_chars);
        self.draw_text(10, 18, path_text.as_bytes(), Color(0x2E668E));

        let max_scroll = self.explorer_max_scroll();
        if max_scroll > 0 {
            let up = self.explorer_scroll_up_rect();
            let down = self.explorer_scroll_down_rect();
            self.fill_rect(up, Color(0xB8C8D8));
            self.fill_rect(down, Color(0xB8C8D8));
            self.draw_border(up, Color(0x1D354A));
            self.draw_border(down, Color(0x1D354A));
            self.draw_text((up.x + 6) as u32, (up.y + 5) as u32, b"^", Color(0x1D354A));
            self.draw_text((down.x + 6) as u32, (down.y + 5) as u32, b"v", Color(0x1D354A));
        }

        let items = self.explorer_items.clone();
        for (idx, item) in items.iter().enumerate() {
            let Some(slot) = self.explorer_icon_rect(idx) else {
                continue;
            };

            self.fill_rect(slot, Color(0xF7FAFF));
            self.draw_border(slot, Color(0xB8C8D8));

            let icon_rect = Rect::new(slot.x + 20, slot.y + 4, 66, 62);
            self.draw_explorer_icon(icon_rect, item.kind);

            let label = Self::trim_label(item.label.as_str(), 14);
            let text_w = (label.len() as i32) * 6;
            let text_x = (slot.x + ((slot.width as i32 - text_w) / 2)).max(2);
            self.draw_text(text_x as u32, (slot.y + 74) as u32, label.as_bytes(), Color(0x1D2A36));
        }

        if self.explorer_side_panel_open {
            let panel_x = 16;
            let panel_y = EXPLORER_TOP_H + 16;
            let panel_w = 200;
            let panel_h = (content_h - EXPLORER_STATUS_H - panel_y).max(0);

            if panel_h > 0 {
                let rect = Rect::new(panel_x, panel_y, panel_w as u32, panel_h as u32);
                self.fill_rect(rect, Color(0xF4F8FC));
                self.draw_border(rect, Color(0x8FA8C4));

                // Draw 'X'
                let close_btn = Rect::new(panel_x + panel_w - 24, panel_y + 4, 20, 20);
                self.fill_rect(close_btn, Color(0xE85C5C));
                self.draw_border(close_btn, Color(0xB84242));
                self.draw_text((close_btn.x + 7) as u32, (close_btn.y + 6) as u32, b"X", Color(0xFFFFFF));

                if let Some(item) = self.explorer_side_panel_item.clone() {
                    let mut py = panel_y + 30;

                    let name_label = Self::trim_label(item.label.as_str(), 28);
                    self.draw_text(panel_x as u32 + 8, py as u32, name_label.as_bytes(), Color(0x1D2A36));
                    py += 20;

                    if item.is_file() {
                        let parts: Vec<&str> = item.label.split('.').collect();
                        let ext = if parts.len() > 1 { parts.last().unwrap() } else { "---" };
                        self.draw_text(panel_x as u32 + 8, py as u32, alloc::format!("Tipo: {}", ext).as_bytes(), Color(0x394C5D));
                        py += 16;

                        let mb = item.size / (1024 * 1024);
                        let mb_frac = (item.size % (1024 * 1024)) * 100 / (1024 * 1024);
                        self.draw_text(panel_x as u32 + 8, py as u32, alloc::format!("Peso: {}.{:02} MB", mb, mb_frac).as_bytes(), Color(0x394C5D));
                        py += 16;
                    } else if item.kind == ExplorerItemKind::Directory {
                        self.draw_text(panel_x as u32 + 8, py as u32, b"Tipo: Carpeta", Color(0x394C5D));
                        py += 16;

                        if let Some(size) = self.explorer_side_panel_dir_size {
                            let mb = size / (1024 * 1024);
                            let mb_frac = (size % (1024 * 1024)) * 100 / (1024 * 1024);
                            self.draw_text(panel_x as u32 + 8, py as u32, alloc::format!("Peso: {}.{:02} MB", mb, mb_frac).as_bytes(), Color(0x394C5D));
                        } else {
                            self.draw_text(panel_x as u32 + 8, py as u32, b"Peso: ---", Color(0x394C5D));
                        }
                        py += 16;
                    }

                    let c_year = (item.create_date >> 9) + 1980;
                    let c_month = (item.create_date >> 5) & 0x0F;
                    let c_day = item.create_date & 0x1F;
                    let c_hour = item.create_time >> 11;
                    let c_min = (item.create_time >> 5) & 0x3F;

                    let w_year = (item.write_date >> 9) + 1980;
                    let w_month = (item.write_date >> 5) & 0x0F;
                    let w_day = item.write_date & 0x1F;
                    let w_hour = item.write_time >> 11;
                    let w_min = (item.write_time >> 5) & 0x3F;

                    let c_str = if item.create_date == 0 {
                        String::from("Creacion: N/A")
                    } else {
                        alloc::format!("Creado: {:02}/{:02}/{} {:02}:{:02}", c_day, c_month, c_year, c_hour, c_min)
                    };
                    self.draw_text(panel_x as u32 + 8, py as u32, c_str.as_bytes(), Color(0x394C5D));
                    py += 16;

                    let w_str = if item.write_date == 0 {
                        String::from("Modif.: N/A")
                    } else {
                        alloc::format!("Modif.: {:02}/{:02}/{} {:02}:{:02}", w_day, w_month, w_year, w_hour, w_min)
                    };
                    self.draw_text(panel_x as u32 + 8, py as u32, w_str.as_bytes(), Color(0x394C5D));
                }
            }
        }

        let status_y = (content_h - EXPLORER_STATUS_H).max(0);
        self.fill_rect(
            Rect::new(0, status_y, self.rect.width, EXPLORER_STATUS_H as u32),
            Color(0xDCE6F0),
        );
        self.fill_rect(Rect::new(0, status_y, self.rect.width, 1), Color(0xA8BCCC));

        let status_text = Self::trim_label(self.explorer_status.as_str(), 72);
        self.draw_text(10, (status_y + 8) as u32, status_text.as_bytes(), Color(0x233A4F));

        let preview = self.explorer_preview_lines.clone();
        let mut py = status_y + 20;
        for line in preview.iter().take(4) {
            let trimmed = Self::trim_label(line.as_str(), 72);
            self.draw_text(10, py as u32, trimmed.as_bytes(), Color(0x394C5D));
            py += 9;
        }
    }

    pub fn render_notepad(&mut self) {
        if self.kind != WindowKind::Notepad {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0xF8FBFF));
        self.fill_rect(Rect::new(0, 0, self.rect.width, NOTEPAD_TOP_H as u32), Color(0xD7E6F8));
        self.fill_rect(Rect::new(0, NOTEPAD_TOP_H, self.rect.width, 1), Color(0xA6BED6));

        let button_labels = ["NEW", "SAVE", "DELETE"];
        let button_colors = [0x4A8BC2, 0x3CA66B, 0xC45A57];
        for i in 0..3 {
            let rect = self.notepad_button_rect(i);
            self.fill_rect(rect, Color(button_colors[i]));
            self.draw_border(rect, Color(0x23374D));
            self.draw_text(
                (rect.x + 14) as u32,
                (rect.y + 7) as u32,
                button_labels[i].as_bytes(),
                Color(0xFFFFFF),
            );
        }

        let name_rect = self.notepad_filename_rect();
        self.fill_rect(name_rect, Color(0xFFFFFF));
        self.draw_border(
            name_rect,
            if self.notepad_edit_name {
                Color(0x2A6FAE)
            } else {
                Color(0x8FA8C4)
            },
        );

        let file_text = alloc::format!("File: {}", self.notepad_file_name);
        let file_text_trim = Self::trim_label(file_text.as_str(), ((name_rect.width as usize) / 6).saturating_sub(2));
        self.draw_text(
            (name_rect.x + 4) as u32,
            (name_rect.y + 7) as u32,
            file_text_trim.as_bytes(),
            Color(0x1E3C5A),
        );

        let editor = self.notepad_editor_rect();
        self.fill_rect(editor, Color(0xFFFFFF));
        self.draw_border(editor, Color(0xAFC0D3));

        let max_cols = ((editor.width as i32 - 8) / 6).max(1) as usize;
        let max_lines = ((editor.height as i32 - 8) / 9).max(1) as usize;
        let lines = Self::wrap_text_lines(self.notepad_text.as_str(), max_cols, max_lines);

        for (i, line) in lines.iter().enumerate() {
            self.draw_text(
                (editor.x + 4) as u32,
                (editor.y + 4 + (i as i32 * 9)) as u32,
                line.as_bytes(),
                Color(0x182736),
            );
        }

        if self.notepad_edit_name {
            let caret_x = (name_rect.x + 4 + file_text_trim.len() as i32 * 6)
                .min(name_rect.x + name_rect.width as i32 - 8)
                .max(name_rect.x + 4);
            self.draw_text(caret_x as u32, (name_rect.y + 7) as u32, b"_", Color(0x1E3C5A));
        } else {
            let line_idx = lines.len().saturating_sub(1);
            let col = lines.get(line_idx).map(|s| s.len()).unwrap_or(0);
            let caret_x = (editor.x + 4 + col as i32 * 6).min(editor.x + editor.width as i32 - 8);
            let caret_y = (editor.y + 4 + line_idx as i32 * 9).min(editor.y + editor.height as i32 - 10);
            self.draw_text(caret_x as u32, caret_y as u32, b"_", Color(0x182736));
        }

        let status_rect = self.notepad_status_rect();
        self.fill_rect(status_rect, Color(0xE6EEF8));
        self.fill_rect(
            Rect::new(status_rect.x, status_rect.y, status_rect.width, 1),
            Color(0xA6BED6),
        );

        let path_text = alloc::format!("Path: {}", self.notepad_dir_path);
        let path_trim = Self::trim_label(path_text.as_str(), 46);
        self.draw_text(
            8,
            (status_rect.y + 8) as u32,
            path_trim.as_bytes(),
            Color(0x2E4B66),
        );

        let status_trim = Self::trim_label(self.notepad_status.as_str(), 72);
        self.draw_text(
            8,
            (status_rect.y + 17) as u32,
            status_trim.as_bytes(),
            Color(0x2E4B66),
        );
    }

    pub fn render_search(&mut self) {
        if self.kind != WindowKind::Search {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0xF3F7FB));
        self.fill_rect(Rect::new(0, 0, self.rect.width, SEARCH_TOP_H as u32), Color(0xDCEBFA));
        self.fill_rect(Rect::new(0, SEARCH_TOP_H - 1, self.rect.width, 1), Color(0x9CB7D4));

        let query_rect = self.search_query_rect();
        self.fill_rect(query_rect, Color(0xFFFFFF));
        self.draw_border(
            query_rect,
            if self.search_input_active {
                Color(0x2A6FAE)
            } else {
                Color(0x8FA8C4)
            },
        );
        let query_trim = Self::trim_label(
            self.search_query.as_str(),
            ((query_rect.width as usize) / 6).saturating_sub(2),
        );
        self.draw_text(
            (query_rect.x + 6) as u32,
            (query_rect.y + 8) as u32,
            query_trim.as_bytes(),
            Color(0x1F3650),
        );
        if self.search_input_active {
            let caret_x = (query_rect.x + 6 + query_trim.len() as i32 * 6)
                .min(query_rect.x + query_rect.width as i32 - 8)
                .max(query_rect.x + 6);
            self.draw_text(caret_x as u32, (query_rect.y + 8) as u32, b"_", Color(0x1F3650));
        }

        let btn = self.search_button_rect();
        self.fill_rect(btn, Color(0x2D89D6));
        self.draw_border(btn, Color(0x1E5E95));
        self.draw_text((btn.x + 18) as u32, (btn.y + 8) as u32, b"Buscar", Color(0xFFFFFF));

        let area = self.search_results_rect();
        self.fill_rect(area, Color(0xFFFFFF));
        self.draw_border(area, Color(0xB2C4D8));

        let visible_rows = self.search_visible_rows();
        let to_draw = core::cmp::min(visible_rows, self.search_results.len());
        for idx in 0..to_draw {
            let Some(row) = self.search_result_row_rect(idx) else {
                break;
            };
            let bg = if idx % 2 == 0 { 0xF7FBFF } else { 0xEEF5FC };
            self.fill_rect(row, Color(bg));
            self.draw_border(row, Color(0xD6E2EF));

            let (entry_label, entry_subtitle) = {
                let entry = &self.search_results[idx];
                (entry.label.clone(), entry.subtitle.clone())
            };
            let label = Self::trim_label(
                entry_label.as_str(),
                ((row.width as usize) / 6).saturating_sub(3),
            );
            self.draw_text(
                (row.x + 6) as u32,
                (row.y + 5) as u32,
                label.as_bytes(),
                Color(0x18324C),
            );
            let subtitle = Self::trim_label(
                entry_subtitle.as_str(),
                ((row.width as usize) / 6).saturating_sub(3),
            );
            self.draw_text(
                (row.x + 6) as u32,
                (row.y + 14) as u32,
                subtitle.as_bytes(),
                Color(0x4A6076),
            );
        }

        let status = self.search_status_rect();
        self.fill_rect(status, Color(0xE6EEF8));
        self.fill_rect(Rect::new(status.x, status.y, status.width, 1), Color(0xA6BED6));
        let status_trim = Self::trim_label(self.search_status.as_str(), 88);
        self.draw_text(8, (status.y + 8) as u32, status_trim.as_bytes(), Color(0x2E4B66));
    }

    pub fn render_browser(&mut self) {
        if self.kind != WindowKind::Browser {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        // Background
        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0xF2F2F2));

        // Top Bar
        self.fill_rect(Rect::new(0, 0, self.rect.width, BROWSER_TOP_H as u32), Color(0xDDDDDD));
        self.fill_rect(Rect::new(0, BROWSER_TOP_H - 1, self.rect.width, 1), Color(0xAAAAAA));

        // Nav Buttons (Visual only)
        self.fill_rect(Rect::new(10, 10, 24, 24), Color(0xCCCCCC)); // Back
        self.draw_text(18, 18, b"<", Color(0x555555));
        self.fill_rect(Rect::new(40, 10, 24, 24), Color(0xCCCCCC)); // Fwd
        self.draw_text(48, 18, b">", Color(0x555555));

        // URL Bar
        let url_rect = self.browser_url_rect();
        self.fill_rect(url_rect, Color(0xFFFFFF));
        self.draw_border(url_rect, Color(0x999999));
        
        let url_trim = Self::trim_label(self.browser_url.as_str(), ((url_rect.width as usize) / 6).saturating_sub(2));
        self.draw_text((url_rect.x + 6) as u32, (url_rect.y + 8) as u32, url_trim.as_bytes(), Color(0x333333));

        // Go Button
        let go_rect = self.browser_go_rect();
        self.fill_rect(go_rect, Color(0x4A90E2));
        self.draw_border(go_rect, Color(0x357ABD));
        self.draw_text((go_rect.x + 17) as u32, (go_rect.y + 8) as u32, b"GO", Color(0xFFFFFF));

        // Scroll Controls
        let up_rect = self.browser_scroll_up_rect();
        let down_rect = self.browser_scroll_down_rect();
        self.fill_rect(up_rect, Color(0xC8D4E2));
        self.fill_rect(down_rect, Color(0xC8D4E2));
        self.draw_border(up_rect, Color(0x7C8FA6));
        self.draw_border(down_rect, Color(0x7C8FA6));
        self.draw_text((up_rect.x + 7) as u32, (up_rect.y + 2) as u32, b"^", Color(0x1E2E40));
        self.draw_text((down_rect.x + 7) as u32, (down_rect.y + 2) as u32, b"v", Color(0x1E2E40));

        // Viewport
        let view_rect = self.browser_viewport_rect();
        self.fill_rect(view_rect, Color(0xFFFFFF));
        let surf_w = self.browser_surface_width as usize;
        let surf_h = self.browser_surface_height as usize;
        let has_surface = surf_w > 0
            && surf_h > 0
            && self.browser_surface_pixels.len() >= surf_w.saturating_mul(surf_h);

        if has_surface {
            let avail_w = (view_rect.width as i32 - 8).max(1) as usize;
            let avail_h = (view_rect.height as i32 - 8).max(1) as usize;

            let mut draw_w = avail_w;
            let mut draw_h = (surf_h.saturating_mul(draw_w)).max(1) / surf_w.max(1);
            if draw_h > avail_h {
                draw_h = avail_h;
                draw_w = (surf_w.saturating_mul(draw_h)).max(1) / surf_h.max(1);
            }
            draw_w = draw_w.max(1).min(avail_w);
            draw_h = draw_h.max(1).min(avail_h);

            let start_x = view_rect.x + ((view_rect.width as i32 - draw_w as i32) / 2);
            let start_y = view_rect.y + ((view_rect.height as i32 - draw_h as i32) / 2);

            for dy in 0..draw_h {
                let sy = dy.saturating_mul(surf_h) / draw_h.max(1);
                for dx in 0..draw_w {
                    let sx = dx.saturating_mul(surf_w) / draw_w.max(1);
                    let src_idx = sy.saturating_mul(surf_w).saturating_add(sx);
                    if src_idx >= self.browser_surface_pixels.len() {
                        continue;
                    }
                    self.draw_pixel(
                        (start_x + dx as i32).max(0) as u32,
                        (start_y + dy as i32).max(0) as u32,
                        Color(self.browser_surface_pixels[src_idx]),
                    );
                }
            }

            self.draw_border(view_rect, Color(0xA7BACD));
            let src = if self.browser_surface_source.trim().is_empty() {
                String::from("servo")
            } else {
                self.browser_surface_source.clone()
            };
            let src_trim = Self::trim_label(src.as_str(), 48);
            self.draw_text(
                (view_rect.x + 8).max(0) as u32,
                (view_rect.y + 4).max(0) as u32,
                alloc::format!("Surface: {}", src_trim).as_bytes(),
                Color(0x2A3B4F),
            );
        } else {
            let flat = self.browser_flat_lines();
            let visible_rows = self.browser_visible_rows();
            let max_scroll = flat.len().saturating_sub(visible_rows);
            if self.browser_scroll > max_scroll {
                self.browser_scroll = max_scroll;
            }

            let mut y_offset = 4;
            for line in flat
                .iter()
                .skip(self.browser_scroll)
                .take(visible_rows)
            {
                if y_offset + 9 > view_rect.height as i32 {
                    break;
                }
                self.draw_text(
                    (view_rect.x + 8) as u32,
                    (view_rect.y + y_offset) as u32,
                    line.as_bytes(),
                    Color(0x000000),
                );
                y_offset += 10;
            }

            // Vertical scrollbar
            if flat.len() > visible_rows {
                let track = Rect::new(
                    view_rect.x + view_rect.width as i32 - 8,
                    view_rect.y + 1,
                    7,
                    view_rect.height.saturating_sub(2),
                );
                self.fill_rect(track, Color(0xE8EEF5));

                let track_h = track.height.max(1) as usize;
                let thumb_h = ((track_h * visible_rows) / flat.len()).max(12).min(track_h);
                let max_thumb_y = track_h.saturating_sub(thumb_h);
                let thumb_off = if max_scroll == 0 {
                    0
                } else {
                    (max_thumb_y * self.browser_scroll) / max_scroll
                } as i32;
                let thumb = Rect::new(track.x + 1, track.y + thumb_off, 5, thumb_h as u32);
                self.fill_rect(thumb, Color(0x95A8BE));
            }
        }

        // Status Bar
        let status_y = (content_h - BROWSER_STATUS_H).max(0);
        self.fill_rect(Rect::new(0, status_y, self.rect.width, BROWSER_STATUS_H as u32), Color(0xEEEEEE));
        self.fill_rect(Rect::new(0, status_y, self.rect.width, 1), Color(0xCCCCCC));

        let status_trim = Self::trim_label(self.browser_status.as_str(), 80);
        self.draw_text(6, (status_y + 8) as u32, status_trim.as_bytes(), Color(0x666666));
    }

    pub fn render_image_viewer(&mut self) {
        if self.kind != WindowKind::ImageViewer {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0xEAF1F8));
        self.fill_rect(
            Rect::new(0, 0, self.rect.width, IMAGE_VIEWER_TOP_H as u32),
            Color(0xD5E3F1),
        );
        self.fill_rect(Rect::new(0, IMAGE_VIEWER_TOP_H - 1, self.rect.width, 1), Color(0xA3B8CC));

        self.draw_text(10, 10, b"IMAGE VIEWER", Color(0x1F3952));
        let name_trim = Self::trim_label(self.image_viewer_file_name.as_str(), 62);
        self.draw_text(10, 22, name_trim.as_bytes(), Color(0x2F5B81));
        let info = if self.image_viewer_width == 0 || self.image_viewer_height == 0 {
            String::from("No image loaded")
        } else {
            alloc::format!(
                "{}x{} px",
                self.image_viewer_width, self.image_viewer_height
            )
        };
        self.draw_text(10, 34, info.as_bytes(), Color(0x2F5B81));

        let view = self.image_viewer_canvas_rect();
        self.fill_rect(view, Color(0xFFFFFF));
        self.draw_border(view, Color(0xA7BACD));

        let img_w = self.image_viewer_width as usize;
        let img_h = self.image_viewer_height as usize;
        if img_w > 0 && img_h > 0 && self.image_viewer_pixels.len() >= img_w.saturating_mul(img_h) {
            let avail_w = (view.width as i32 - 16).max(1) as usize;
            let avail_h = (view.height as i32 - 16).max(1) as usize;

            let mut draw_w = avail_w;
            let mut draw_h = (img_h.saturating_mul(draw_w)).max(1) / img_w.max(1);
            if draw_h > avail_h {
                draw_h = avail_h;
                draw_w = (img_w.saturating_mul(draw_h)).max(1) / img_h.max(1);
            }
            draw_w = draw_w.max(1).min(avail_w);
            draw_h = draw_h.max(1).min(avail_h);

            let start_x = view.x + ((view.width as i32 - draw_w as i32) / 2);
            let start_y = view.y + ((view.height as i32 - draw_h as i32) / 2);

            self.fill_rect(
                Rect::new(
                    start_x - 2,
                    start_y - 2,
                    (draw_w + 4) as u32,
                    (draw_h + 4) as u32,
                ),
                Color(0xD4DDE6),
            );

            for dy in 0..draw_h {
                let sy = dy.saturating_mul(img_h) / draw_h.max(1);
                for dx in 0..draw_w {
                    let sx = dx.saturating_mul(img_w) / draw_w.max(1);
                    let src_idx = sy.saturating_mul(img_w).saturating_add(sx);
                    if src_idx >= self.image_viewer_pixels.len() {
                        continue;
                    }
                    let color = self.image_viewer_pixels[src_idx];
                    self.draw_pixel(
                        (start_x + dx as i32).max(0) as u32,
                        (start_y + dy as i32).max(0) as u32,
                        Color(color),
                    );
                }
            }
        } else {
            self.draw_text(
                (view.x + 14).max(0) as u32,
                (view.y + 16).max(0) as u32,
                b"Open a PNG file from Explorer to preview it here.",
                Color(0x5D6F80),
            );
        }

        let status_y = (content_h - IMAGE_VIEWER_STATUS_H).max(0);
        self.fill_rect(
            Rect::new(0, status_y, self.rect.width, IMAGE_VIEWER_STATUS_H as u32),
            Color(0xDDE6EF),
        );
        self.fill_rect(Rect::new(0, status_y, self.rect.width, 1), Color(0xA3B8CC));
        let status_trim = Self::trim_label(self.image_viewer_status.as_str(), 80);
        self.draw_text(8, (status_y + 10) as u32, status_trim.as_bytes(), Color(0x2B4258));
    }

    pub fn render_settings(&mut self) {
        if self.kind != WindowKind::Settings {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        // Background
        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0xF2F2F2));

        // Header
        self.fill_rect(Rect::new(0, 0, self.rect.width, 40), Color(0x34495E));
        self.draw_text(15, 15, b"Configuracion del Sistema", Color(0xFFFFFF));

        let mut y = 60;

        // Section: System Info
        self.draw_text(15, y, b"Informacion de Software:", Color(0x2C3E50));
        y += 15;
        self.draw_text(25, y, b"- SO: Go OS v0.2.0 (Alpha)", Color(0x555555));
        y += 12;
        self.draw_text(25, y, b"- Kernel: x86_64 Microkernel", Color(0x555555));
        y += 25;

        // Section: Memory Info
        self.draw_text(15, y, b"Memoria RAM (Heap):", Color(0x2C3E50));
        y += 15;
        let heap_bytes = crate::allocator::heap_size_bytes();
        let heap_mib = heap_bytes / (1024 * 1024);
        let heap_task_reserved = crate::allocator::heap_reserved_bytes();
        let heap_task_reserved_mib = heap_task_reserved / (1024 * 1024);
        self.draw_text(
            25,
            y,
            alloc::format!("- Total Reservada: {} MB", heap_mib).as_bytes(),
            Color(0x555555),
        );
        y += 12;
        self.draw_text(
            25,
            y,
            alloc::format!("- Reservada por tareas: {} MB", heap_task_reserved_mib).as_bytes(),
            Color(0x555555),
        );
        y += 12;
        self.draw_text(25, y, b"- Estado: Activo", Color(0x555555));
        y += 25;

        // Section: Network Info
        self.draw_text(15, y, b"Estado de Red:", Color(0x2C3E50));
        y += 15;
        
        let has_net = unsafe { crate::net::IFACE.is_some() };
        let intel_model = crate::intel_net::get_model_name();
        let link_up = crate::intel_net::is_link_up();
        let active_transport = crate::net::get_active_transport();
        let failover_policy = crate::net::get_failover_policy();
        let ip_mode = crate::net::get_network_mode();
        let https_mode = crate::net::get_https_mode();
        let (s_ip, s_prefix, s_gw) = crate::net::get_static_ipv4_config();
        let wifi_model = crate::intel_wifi::get_model_name();
        let wifi_status = crate::intel_wifi::get_status();
        let wifi_datapath_ready = crate::intel_wifi::is_data_path_ready();
        let wifi_fw_hint = crate::intel_wifi::firmware_hint();

        self.draw_text(
            25,
            y,
            alloc::format!("- Transporte activo: {}", active_transport).as_bytes(),
            Color(0x34495E),
        );
        y += 12;
        self.draw_text(
            25,
            y,
            alloc::format!("- Failover: {}", failover_policy).as_bytes(),
            Color(0x34495E),
        );
        y += 12;
        self.draw_text(
            25,
            y,
            alloc::format!("- Modo IP: {}", ip_mode).as_bytes(),
            Color(0x34495E),
        );
        y += 12;
        self.draw_text(
            25,
            y,
            alloc::format!("- HTTPS: {}", https_mode).as_bytes(),
            Color(0x34495E),
        );
        y += 12;
        self.draw_text(
            25,
            y,
            alloc::format!(
                "- Perfil fija: {}.{}.{}.{}/{} gw {}.{}.{}.{}",
                s_ip[0], s_ip[1], s_ip[2], s_ip[3], s_prefix, s_gw[0], s_gw[1], s_gw[2], s_gw[3]
            )
            .as_bytes(),
            Color(0x34495E),
        );
        y += 12;

        if let Some(name) = intel_model {
            self.draw_text(25, y, alloc::format!("- Interfaz: {}", name).as_bytes(), Color(0x27AE60));
            y += 12;
            
            // Link Status with Color
            let link_color = if link_up { Color(0x27AE60) } else { Color(0xC0392B) };
            let link_text = if link_up { "Conectado (Enlace OK)" } else { "Sin Cable (Link Down)" };
            self.draw_text(25, y, alloc::format!("- Enlace: {}", link_text).as_bytes(), link_color);
            y += 12;

            if let Some(mac) = crate::intel_net::get_mac_address() {
                let mac_str = alloc::format!("- MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
                self.draw_text(25, y, mac_str.as_bytes(), Color(0x555555));
                y += 12;
            }

            // IP Address info
            let dhcp_status = unsafe { crate::net::DHCP_STATUS };
            self.draw_text(25, y, alloc::format!("- DHCP: {}", dhcp_status).as_bytes(), Color(0x34495E));
            y += 12;

            if let Some(ip) = crate::net::get_ip_address() {
                self.draw_text(25, y, alloc::format!("- IP: {}", ip).as_bytes(), Color(0x2980B9));
                y += 12;
            }
            if let Some(gw) = crate::net::get_gateway() {
                self.draw_text(25, y, alloc::format!("- Gateway: {}", gw).as_bytes(), Color(0x555555));
                y += 12;
            }

            // Packet stats
            let (rx, tx) = crate::net::get_packet_stats();
            self.draw_text(25, y, alloc::format!("- Paquetes: RX: {} | TX: {}", rx, tx).as_bytes(), Color(0x555555));
            y += 12;

            if let Some(wifi_name) = wifi_model {
                self.draw_text(
                    25,
                    y,
                    alloc::format!(
                        "- WiFi: {} ({}, datapath={})",
                        wifi_name,
                        wifi_status,
                        if wifi_datapath_ready { "ready" } else { "pending" }
                    )
                    .as_bytes(),
                    Color(0x8E44AD),
                );
                y += 12;
                if let Some(hint) = wifi_fw_hint {
                    self.draw_text(
                        25,
                        y,
                        alloc::format!("- WiFi FW: {}", hint).as_bytes(),
                        Color(0x7F8C8D),
                    );
                    y += 12;
                }
            }
        } else if has_net {
            self.draw_text(25, y, b"- Interfaz: VirtIO Ethernet", Color(0x27AE60));
            y += 12;
            let dhcp_status = unsafe { crate::net::DHCP_STATUS };
            self.draw_text(25, y, alloc::format!("- DHCP: {}", dhcp_status).as_bytes(), Color(0x555555));
            y += 12;
            if let Some(ip) = crate::net::get_ip_address() {
                self.draw_text(25, y, alloc::format!("- IP: {}", ip).as_bytes(), Color(0x2980B9));
                y += 12;
            }
            self.draw_text(25, y, b"- Estado: Conectado", Color(0x555555));
            y += 12;

            if let Some(wifi_name) = wifi_model {
                self.draw_text(
                    25,
                    y,
                    alloc::format!(
                        "- WiFi: {} ({}, datapath={})",
                        wifi_name,
                        wifi_status,
                        if wifi_datapath_ready { "ready" } else { "pending" }
                    )
                    .as_bytes(),
                    Color(0x8E44AD),
                );
                y += 12;
                if let Some(hint) = wifi_fw_hint {
                    self.draw_text(
                        25,
                        y,
                        alloc::format!("- WiFi FW: {}", hint).as_bytes(),
                        Color(0x7F8C8D),
                    );
                    y += 12;
                }
            }
        } else {
            if let Some(wifi_name) = wifi_model {
                self.draw_text(25, y, b"- Interfaz LAN: No encontrada", Color(0xC0392B));
                y += 12;
                self.draw_text(
                    25,
                    y,
                    alloc::format!(
                        "- WiFi: {} ({}, datapath={})",
                        wifi_name,
                        wifi_status,
                        if wifi_datapath_ready { "ready" } else { "pending" }
                    )
                    .as_bytes(),
                    Color(0x8E44AD),
                );
                y += 12;
                if let Some(hint) = wifi_fw_hint {
                    self.draw_text(
                        25,
                        y,
                        alloc::format!("- WiFi FW: {}", hint).as_bytes(),
                        Color(0x7F8C8D),
                    );
                    y += 12;
                }
            } else {
                self.draw_text(25, y, b"- Interfaz: No encontrada", Color(0xC0392B));
                y += 12;
            }
            self.draw_text(25, y, b"- Estado: Desconectado", Color(0x555555));
        }
        y += 25;

        // Hardware Notice
        let hy = y as i32;
        self.fill_rect(Rect::new(15, hy, self.rect.width.saturating_sub(30), 60), Color(0xECF0F1));
        self.draw_border(Rect::new(15, hy, self.rect.width.saturating_sub(30), 60), Color(0xBDC3C7));
        self.draw_text(20, (hy + 12) as u32, b"Informacion de Hardware:", Color(0x2C3E50));
        
        if intel_model.is_some() || wifi_model.is_some() {
             self.draw_text(20, (hy + 28) as u32, b"Estado: Hardware de red detectado (modo experimental).", Color(0x27AE60));
             if let Some(wifi_name) = wifi_model {
                 self.draw_text(20, (hy + 42) as u32, alloc::format!("WiFi: {}", wifi_name).as_bytes(), Color(0x8E44AD));
             }
        } else {
             self.draw_text(20, (hy + 28) as u32, b"Placa: ASUS ROG MAXIMUS Z890 HERO", Color(0x7F8C8D));
             self.draw_text(20, (hy + 42) as u32, b"Nota: Se requieren drivers Intel i226 para LAN.", Color(0x7F8C8D));
        }

        // ── "Administrar WiFi" button (only if WiFi adapter present) ──
        if crate::intel_wifi::is_present() {
            let btn_y = hy + 70;
            self.fill_rect(Rect::new(15, btn_y, 160, 28), Color(0x8E44AD));
            self.draw_border(Rect::new(15, btn_y, 160, 28), Color(0x6C3483));
            self.draw_text(26, (btn_y + 9) as u32, b"Administrar WiFi", Color(0xFFFFFF));
        }
    }

    pub fn render_media_player(&mut self) {
        if self.kind != WindowKind::MediaPlayer {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        let w = self.rect.width as i32;

        // ── Dark gradient background ──
        for row in 0..content_h {
            let blend = (row as u32 * 40 / content_h.max(1) as u32).min(40) as u32;
            let r = (0x0Au32 + blend / 2).min(0xFF);
            let g = (0x0Eu32 + blend / 3).min(0xFF);
            let b = (0x1Au32 + blend).min(0xFF);
            let color = Color((r << 16) | (g << 8) | b);
            self.fill_rect(Rect::new(0, row, self.rect.width, 1), color);
        }

        // ── Header bar ──
        self.fill_rect(Rect::new(0, 0, self.rect.width, 44), Color(0x111827));
        // Music note icon  ♫
        self.fill_rect(Rect::new(14, 10, 24, 24), Color(0x8B5CF6)); // purple bg
        self.draw_text(18, 16, b"M", Color(0xFFFFFF));
        self.draw_text(50, 10, b"MEDIA PLAYER", Color(0xE5E7EB));

        // Status text line
        let status_text = if crate::audio::is_playing() {
            "Reproduciendo"
        } else if crate::audio::is_ready() {
            "Listo"
        } else {
            crate::audio::status_text()
        };
        self.draw_text(50, 26, status_text.as_bytes(), Color(0x6B7280));

        // ── Track name (large) ──
        let track_name = self.notepad_file_name.clone();
        let track_display = if track_name.len() > 40 {
            alloc::format!("{}...", &track_name[..37])
        } else if track_name.is_empty() {
            alloc::string::String::from("Sin archivo")
        } else {
            track_name.clone()
        };
        self.draw_text(20, 60, track_display.as_bytes(), Color(0xF9FAFB));

        // ── Audio info text ──
        let info = self.notepad_status.clone();
        if !info.is_empty() {
            let info_display = if info.len() > 65 {
                alloc::format!("{}...", &info[..62])
            } else {
                info
            };
            self.draw_text(20, 78, info_display.as_bytes(), Color(0x9CA3AF));
        }

        // ── Progress bar ──
        let bar_x = 20i32;
        let bar_y = 108i32;
        let bar_w = (w - 40).max(60) as u32;
        let bar_h = 8u32;

        // Bar background (dark grey)
        self.fill_rect(Rect::new(bar_x, bar_y, bar_w, bar_h), Color(0x374151));

        // Progress fill
        let duration_ms = self.explorer_scroll.max(0) as u32;
        let position = crate::audio::playback_position() as u32;
        let total_bytes = unsafe { crate::audio::GLOBAL_HDA.pcm_total_bytes } as u32;

        let progress_frac = if total_bytes > 0 {
            (position as f64 / total_bytes as f64).min(1.0)
        } else {
            0.0
        };
        let fill_w = (bar_w as f64 * progress_frac) as u32;
        if fill_w > 0 {
            // Purple gradient fill
            self.fill_rect(Rect::new(bar_x, bar_y, fill_w, bar_h), Color(0x8B5CF6));
            // Bright head dot
            if fill_w > 2 {
                self.fill_rect(
                    Rect::new(bar_x + fill_w as i32 - 3, bar_y - 2, 6, 12),
                    Color(0xA78BFA),
                );
            }
        }

        // Time labels
        let elapsed_ms = if duration_ms > 0 {
            (progress_frac * duration_ms as f64) as u32
        } else {
            0
        };
        let elapsed_text = crate::wav::format_time(elapsed_ms);
        let total_text = crate::wav::format_time(duration_ms);
        self.draw_text(bar_x as u32, (bar_y + 14) as u32, elapsed_text.as_bytes(), Color(0x9CA3AF));
        let total_x = (bar_x + bar_w as i32 - 30).max(bar_x + 50);
        self.draw_text(total_x as u32, (bar_y + 14) as u32, total_text.as_bytes(), Color(0x9CA3AF));

        // ── Control buttons ──
        let btn_y = 145i32;
        let btn_size = 40i32;
        let center_x = w / 2;

        // Stop button (left)
        let stop_x = center_x - btn_size - 20;
        self.fill_rect(Rect::new(stop_x, btn_y, btn_size as u32, btn_size as u32), Color(0x1F2937));
        self.draw_border(Rect::new(stop_x, btn_y, btn_size as u32, btn_size as u32), Color(0x4B5563));
        // Stop icon: filled square
        self.fill_rect(Rect::new(stop_x + 12, btn_y + 12, 16, 16), Color(0xEF4444));

        // Play/Pause button (center, larger)
        let play_x = center_x - btn_size / 2;
        let play_bg = if crate::audio::is_playing() { 0x8B5CF6 } else { 0x6D28D9 };
        self.fill_rect(Rect::new(play_x, btn_y - 4, (btn_size + 8) as u32, (btn_size + 8) as u32), Color(play_bg));
        if crate::audio::is_playing() {
            // Pause icon: two vertical bars
            self.fill_rect(Rect::new(play_x + 14, btn_y + 8, 6, 20), Color(0xFFFFFF));
            self.fill_rect(Rect::new(play_x + 26, btn_y + 8, 6, 20), Color(0xFFFFFF));
        } else {
            // Play icon: triangle (approximated with rectangles)
            for i in 0..16 {
                let lx = play_x + 16;
                let ly = btn_y + 4 + i;
                let lw = (i.min(16 - i) * 2).max(1) as u32;
                self.fill_rect(Rect::new(lx, ly, lw, 1), Color(0xFFFFFF));
            }
        }

        // Forward/Skip button (right)  — visual only for now
        let fwd_x = center_x + 24;
        self.fill_rect(Rect::new(fwd_x, btn_y, btn_size as u32, btn_size as u32), Color(0x1F2937));
        self.draw_border(Rect::new(fwd_x, btn_y, btn_size as u32, btn_size as u32), Color(0x4B5563));
        self.fill_rect(Rect::new(fwd_x + 10, btn_y + 14, 8, 12), Color(0x9CA3AF));
        self.fill_rect(Rect::new(fwd_x + 22, btn_y + 14, 4, 12), Color(0x9CA3AF));

        // ── Volume bar ──
        let vol_y = btn_y + btn_size + 30;
        self.draw_text(20, vol_y as u32, b"Vol:", Color(0x9CA3AF));
        let vol_bar_x = 52i32;
        let vol_bar_w = 120u32;
        self.fill_rect(Rect::new(vol_bar_x, vol_y + 3, vol_bar_w, 6), Color(0x374151));
        let vol = unsafe { crate::audio::GLOBAL_HDA.volume } as u32;
        let vol_fill = (vol_bar_w * vol / 127).min(vol_bar_w);
        if vol_fill > 0 {
            self.fill_rect(Rect::new(vol_bar_x, vol_y + 3, vol_fill, 6), Color(0x10B981));
        }
        let vol_pct = (vol as u32 * 100 / 127).min(100);
        let vol_text = alloc::format!("{}%", vol_pct);
        self.draw_text((vol_bar_x + vol_bar_w as i32 + 8) as u32, vol_y as u32, vol_text.as_bytes(), Color(0x9CA3AF));

        // ── HDA Driver Status ──
        let status_y = vol_y + 28;
        let hda_status = crate::audio::status_text();
        let driver_info = alloc::format!("Driver HDA: {}", hda_status);
        let status_color = if crate::audio::is_ready() || crate::audio::is_playing() {
            0x10B981 // green
        } else {
            0xEF4444 // red
        };
        self.fill_rect(Rect::new(16, status_y, 8, 8), Color(status_color));
        self.draw_text(30, status_y as u32, driver_info.as_bytes(), Color(0x6B7280));
    }

    pub fn render_video_player(&mut self) {
        if self.kind != WindowKind::VideoPlayer {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        let w = self.rect.width as i32;
        let h = content_h as i32;

        self.buffer.fill(0x111827);

        // Ensure valid video properties
        let vw = self.video_player_width as usize;
        let vh = self.video_player_height as usize;
        if vw == 0 || vh == 0 || self.video_player_file_cluster < 2 {
            self.draw_text(20, 20, b"No video loaded", Color(0xEF4444));
            return;
        }

        let frame_size = vw.saturating_mul(vh).saturating_mul(4);
        if frame_size == 0 {
            self.draw_text(20, 20, b"Invalid video frame size", Color(0xEF4444));
            return;
        }

        let payload_bytes = (self.video_player_file_size as usize)
            .saturating_sub(self.video_player_data_offset);
        let max_frames = payload_bytes / frame_size;
        if max_frames == 0 {
            self.draw_text(20, 20, b"RPV has no complete frames", Color(0xEF4444));
            return;
        }
        if self.video_player_current_frame >= max_frames {
            self.video_player_current_frame = 0;
        }

        // Limit FPS
        let current_tick = crate::timer::ticks();
        let ms_per_frame = (1000 / self.video_player_fps.max(1) as u64).max(1);
        let mut advance_frame = false;

        if self.video_player_last_tick == 0 {
            self.video_player_last_tick = current_tick;
        } else if self.doom_native_running
            && current_tick >= self.video_player_last_tick.saturating_add(ms_per_frame)
        {
            self.video_player_last_tick = current_tick;
            advance_frame = true;
        }

        let frame_offset = self.video_player_current_frame.saturating_mul(frame_size);
        let offset = self.video_player_data_offset.saturating_add(frame_offset);

        // Reuse the frame buffer so playback does not allocate hundreds of KB
        // on every repaint. If the file was cached on open, draw from RAM so USB
        // latency cannot corrupt the live frame stream.
        if self.video_player_frame_buf.len() != frame_size {
            self.video_player_frame_buf.resize(frame_size, 0);
        }
        let bytes_read = match frame_offset.checked_add(frame_size) {
            Some(end) if end <= self.video_player_cached_payload.len() => {
                self.video_player_frame_buf.copy_from_slice(
                    &self.video_player_cached_payload[frame_offset..end],
                );
                frame_size
            }
            _ => {
                let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
                fat.read_file_range(
                    self.video_player_file_cluster,
                    self.video_player_file_size as usize,
                    offset,
                    &mut self.video_player_frame_buf,
                ).unwrap_or(0)
            }
        };

        if bytes_read == frame_size {
            let controls_h = 60i32.min(h.max(0));
            let video_area_h = (h - controls_h).max(1);
            let avail_w = w.max(1) as usize;
            let avail_h = video_area_h.max(1) as usize;

            let mut draw_w = avail_w;
            let mut draw_h = vh.saturating_mul(draw_w).max(1) / vw.max(1);
            if draw_h > avail_h {
                draw_h = avail_h;
                draw_w = vw.saturating_mul(draw_h).max(1) / vh.max(1);
            }
            draw_w = draw_w.max(1).min(avail_w);
            draw_h = draw_h.max(1).min(avail_h);

            let start_x = (w - draw_w as i32) / 2;
            let start_y = (video_area_h - draw_h as i32) / 2;

            // Draw scaled BGRA frame into the window buffer.
            for dy in 0..draw_h {
                let sy = dy.saturating_mul(vh) / draw_h.max(1);
                for dx in 0..draw_w {
                    let sx = dx.saturating_mul(vw) / draw_w.max(1);
                    let idx = (sy * vw + sx) * 4;
                    let b = self.video_player_frame_buf[idx];
                    let g = self.video_player_frame_buf[idx + 1];
                    let r = self.video_player_frame_buf[idx + 2];
                    let color = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                    let px = start_x + dx as i32;
                    let py = start_y + dy as i32;
                    if px >= 0 && py >= 0 && px < self.rect.width as i32 && py < self.rect.height as i32 {
                        let dst_idx = py as usize * self.rect.width as usize + px as usize;
                        if dst_idx < self.buffer.len() {
                            self.buffer[dst_idx] = color;
                        }
                    }
                }
            }

            if advance_frame {
                self.video_player_current_frame += 1;
                if self.video_player_current_frame >= max_frames {
                    self.video_player_current_frame = 0; // Loop video
                }
            }
        } else {
            // EOF or error
            if advance_frame {
                self.video_player_current_frame = 0; // Loop video on EOF
            }
        }

        // ── Controls Overlay ──
        let controls_h = 60;
        let controls_y = h - controls_h;
        // Dark bar at the bottom
        self.fill_rect(Rect::new(0, controls_y, w as u32, controls_h as u32), Color(0x111827));

        // Progress bar
        let bar_x = 20i32;
        let bar_y = controls_y + 10;
        let bar_w = (w - 40).max(60) as u32;
        self.fill_rect(Rect::new(bar_x, bar_y, bar_w, 4), Color(0x374151));
        let progress = if max_frames > 0 {
            self.video_player_current_frame as f64 / max_frames as f64
        } else {
            0.0
        };
        let fill_w = (bar_w as f64 * progress) as u32;
        self.fill_rect(Rect::new(bar_x, bar_y, fill_w, 4), Color(0x3B82F6)); // Blue progress

        // Play/Pause button
        let btn_size = 30i32;
        let btn_x = w / 2 - btn_size / 2;
        let btn_y = controls_y + 20;
        self.fill_rect(Rect::new(btn_x, btn_y, btn_size as u32, btn_size as u32), Color(0x1F2937));
        self.draw_border(Rect::new(btn_x, btn_y, btn_size as u32, btn_size as u32), Color(0x4B5563));

        if self.doom_native_running {
            self.fill_rect(Rect::new(btn_x + 8, btn_y + 8, 4, 14), Color(0xFFFFFF));
            self.fill_rect(Rect::new(btn_x + 18, btn_y + 8, 4, 14), Color(0xFFFFFF));
        } else {
            for i in 0..12 {
                let lx = btn_x + 10;
                let ly = btn_y + 9 + i;
                let lw = (i.min(12 - i) * 2).max(1) as u32;
                self.fill_rect(Rect::new(lx, ly, lw, 1), Color(0xFFFFFF));
            }
        }

        let video_name = if self.video_player_file_name.is_empty() {
            alloc::string::String::from("DEMO_VIDEO.RPV")
        } else {
            self.video_player_file_name.clone()
        };
        self.draw_text(20, (btn_y + 8) as u32, video_name.as_bytes(), Color(0x9CA3AF));
        let source = if self.video_player_cached_payload.is_empty() {
            "DISK"
        } else {
            "RAM"
        };
        let info = alloc::format!("{}x{} {}fps {}", vw, vh, self.video_player_fps, source);
        let info_x = (w - (info.len() as i32 * 6) - 20).max(20);
        self.draw_text(info_x as u32, (btn_y + 8) as u32, info.as_bytes(), Color(0x6B7280));
    }

    pub fn render_wifi_manager(&mut self) {
        if self.kind != WindowKind::WifiManager {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        let w = self.rect.width as i32;

        // ── Dark background ──
        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0x111827));

        // ── Header bar ──
        self.fill_rect(Rect::new(0, 0, self.rect.width, 42), Color(0x1E293B));
        self.fill_rect(Rect::new(14, 10, 24, 24), Color(0x3B82F6));
        self.draw_text(19, 16, b"W", Color(0xFFFFFF));
        self.draw_text(50, 10, b"WIFI MANAGER", Color(0xE5E7EB));

        // WiFi driver status
        let driver_status = crate::intel_wifi::get_status();
        let driver_trim = Self::trim_label(driver_status, 50);
        self.draw_text(50, 26, driver_trim.as_bytes(), Color(0x6B7280));

        // ── Mode toggle (Ethernet / WiFi) ──
        let toggle_y = 52i32;
        self.fill_rect(Rect::new(10, toggle_y, self.rect.width.saturating_sub(20), 28), Color(0x1F2937));
        self.draw_border(Rect::new(10, toggle_y, self.rect.width.saturating_sub(20), 28), Color(0x374151));

        // Ethernet side
        let eth_bg = if !self.wifi_mode_active { 0x2563EB } else { 0x1F2937 };
        self.fill_rect(Rect::new(12, toggle_y + 2, (w / 2 - 14) as u32, 24), Color(eth_bg));
        let eth_label_x = 12 + (w / 2 - 14) / 2 - 24;
        self.draw_text(eth_label_x as u32, (toggle_y + 8) as u32, b"Ethernet", Color(0xF9FAFB));

        // WiFi side
        let wifi_bg = if self.wifi_mode_active { 0x2563EB } else { 0x1F2937 };
        self.fill_rect(Rect::new(w / 2, toggle_y + 2, (w / 2 - 14) as u32, 24), Color(wifi_bg));
        let wifi_label_x = w / 2 + (w / 2 - 14) / 2 - 12;
        self.draw_text(wifi_label_x as u32, (toggle_y + 8) as u32, b"WiFi", Color(0xF9FAFB));

        // ── Scan button ──
        let scan_y = 90i32;
        let scan_rect = Rect::new(10, scan_y, 100, 24);
        self.fill_rect(scan_rect, Color(0x059669));
        self.draw_border(scan_rect, Color(0x047857));
        self.draw_text(24, (scan_y + 7) as u32, b"Escanear", Color(0xFFFFFF));

        // Network count
        let count_text = alloc::format!("{} redes", self.wifi_scan_entries.len());
        self.draw_text(120, (scan_y + 7) as u32, count_text.as_bytes(), Color(0x9CA3AF));

        // ── Network list ──
        let list_y = 120i32;
        let list_h = (content_h - list_y - 120).max(80);
        self.fill_rect(Rect::new(10, list_y, self.rect.width.saturating_sub(20), list_h as u32), Color(0x1F2937));
        self.draw_border(Rect::new(10, list_y, self.rect.width.saturating_sub(20), list_h as u32), Color(0x374151));

        let item_h = 24i32;
        let visible_count = (list_h / item_h).max(1) as usize;
        let max_scroll = self.wifi_scan_entries.len().saturating_sub(visible_count);
        if self.wifi_scroll > max_scroll {
            self.wifi_scroll = max_scroll;
        }

        if self.wifi_scan_entries.is_empty() {
            self.draw_text(20, (list_y + 12) as u32, b"Sin redes. Presiona Escanear.", Color(0x6B7280));
        } else {
            let entries_clone = self.wifi_scan_entries.clone();
            for (i, entry) in entries_clone
                .iter()
                .skip(self.wifi_scroll)
                .take(visible_count)
                .enumerate()
            {
                let actual_idx = i + self.wifi_scroll;
                let iy = list_y + 2 + (i as i32) * item_h;

                // Highlight selected
                if actual_idx == self.wifi_selected_index {
                    self.fill_rect(
                        Rect::new(12, iy, self.rect.width.saturating_sub(24), item_h as u32),
                        Color(0x2563EB),
                    );
                }

                // Lock icon for secure
                let lock = if entry.3 { "@ " } else { "  " };
                let ssid_label = alloc::format!("{}{}", lock, entry.0);
                let ssid_trim = Self::trim_label(ssid_label.as_str(), 30);
                let text_color = if actual_idx == self.wifi_selected_index { 0xFFFFFF } else { 0xD1D5DB };
                self.draw_text(18, (iy + 7) as u32, ssid_trim.as_bytes(), Color(text_color));

                // Signal strength bars
                let rssi = entry.1;
                let bars = if rssi > -50 { 4 } else if rssi > -65 { 3 } else if rssi > -75 { 2 } else { 1 };
                let bar_x = w - 60;
                for b in 0..4 {
                    let bh = 4 + b * 3;
                    let by = iy + 18 - bh;
                    let bc = if b < bars { 0x10B981 } else { 0x4B5563 };
                    self.fill_rect(Rect::new(bar_x + b * 8, by, 5, bh as u32), Color(bc));
                }

                // Channel
                let ch_text = alloc::format!("ch{}", entry.2);
                self.draw_text((w - 100) as u32, (iy + 7) as u32, ch_text.as_bytes(), Color(0x6B7280));
            }
        }

        // ── Password field ──
        let pw_y = list_y + list_h + 8;
        self.draw_text(14, pw_y as u32, b"Clave:", Color(0x9CA3AF));
        let pw_field_x = 60i32;
        let pw_field_w = (w - pw_field_x - 14).max(60) as u32;
        let pw_border = if self.wifi_password_editing { 0x3B82F6 } else { 0x4B5563 };
        self.fill_rect(Rect::new(pw_field_x, pw_y, pw_field_w, 22), Color(0x111827));
        self.draw_border(Rect::new(pw_field_x, pw_y, pw_field_w, 22), Color(pw_border));

        // Show password as dots or text
        let pw_display = if self.wifi_password_input.is_empty() {
            String::from("...")
        } else {
            let dots: String = core::iter::repeat('*').take(self.wifi_password_input.len()).collect();
            dots
        };
        let pw_trim = Self::trim_label(pw_display.as_str(), ((pw_field_w / 6) as usize).saturating_sub(2));
        self.draw_text((pw_field_x + 6) as u32, (pw_y + 6) as u32, pw_trim.as_bytes(), Color(0xD1D5DB));

        // ── Connect / Disconnect buttons ──
        let btn_y = pw_y + 32;
        let connect_rect = Rect::new(10, btn_y, 110, 28);
        self.fill_rect(connect_rect, Color(0x2563EB));
        self.draw_border(connect_rect, Color(0x1D4ED8));
        self.draw_text(26, (btn_y + 9) as u32, b"Conectar", Color(0xFFFFFF));

        let disconnect_rect = Rect::new(130, btn_y, 120, 28);
        self.fill_rect(disconnect_rect, Color(0xDC2626));
        self.draw_border(disconnect_rect, Color(0xB91C1C));
        self.draw_text(140, (btn_y + 9) as u32, b"Desconectar", Color(0xFFFFFF));

        // ── Status bar ──
        let status_y = (content_h - 30).max(btn_y + 36);
        self.fill_rect(Rect::new(0, status_y, self.rect.width, 30), Color(0x0F172A));
        self.fill_rect(Rect::new(0, status_y, self.rect.width, 1), Color(0x1E293B));

        // Connection indicator
        let connected = crate::intel_wifi::is_connected();
        let dot_color = if connected { 0x10B981 } else { 0xEF4444 };
        self.fill_rect(Rect::new(10, status_y + 11, 8, 8), Color(dot_color));

        let conn_text = if connected {
            if let Some((ssid_buf, len)) = crate::intel_wifi::connected_ssid() {
                let ssid = core::str::from_utf8(&ssid_buf[..len]).unwrap_or("?");
                alloc::format!("Conectado: {}", ssid)
            } else {
                String::from("Conectado")
            }
        } else if !self.wifi_status_msg.is_empty() {
            self.wifi_status_msg.clone()
        } else {
            String::from("Desconectado")
        };
        let status_trim = Self::trim_label(conn_text.as_str(), 60);
        self.draw_text(24, (status_y + 10) as u32, status_trim.as_bytes(), Color(0x9CA3AF));
    }

    pub fn render_task_manager(&mut self) {
        if self.kind != WindowKind::TaskManager {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        let w = self.rect.width as i32;
        let content_h_i32 = content_h as i32;

        // Background
        self.fill_rect(
            Rect::new(0, 0, self.rect.width, content_h as u32),
            Color(0x111827),
        );

        // Header
        self.fill_rect(Rect::new(0, 0, self.rect.width, TASK_MGR_HEADER_H as u32), Color(0x1F2937));
        self.draw_text(14, 14, b"TASK MANAGER", Color(0xF9FAFB));
        if !self.task_manager_status.is_empty() {
            let status_trim = Self::trim_label(self.task_manager_status.as_str(), 48);
            self.draw_text(14, 28, status_trim.as_bytes(), Color(0x9CA3AF));
        }

        // List area
        let list_y = TASK_MGR_HEADER_H + 6;
        let list_h = (content_h_i32 - TASK_MGR_HEADER_H - TASK_MGR_FOOTER_H).max(60);
        self.fill_rect(
            Rect::new(10, list_y, self.rect.width.saturating_sub(20), list_h as u32),
            Color(0x0F172A),
        );
        self.draw_border(
            Rect::new(10, list_y, self.rect.width.saturating_sub(20), list_h as u32),
            Color(0x334155),
        );

        let visible_rows = (list_h / TASK_MGR_ROW_H).max(1) as usize;
        let max_scroll = self.task_manager_lines.len().saturating_sub(visible_rows);
        if self.task_manager_scroll > max_scroll {
            self.task_manager_scroll = max_scroll;
        }

        let start = self.task_manager_scroll;
        let end = (start + visible_rows).min(self.task_manager_lines.len());
        for idx in start..end {
            let line = self.task_manager_lines[idx].clone();
            let row = idx - start;
            let row_y = list_y + 4 + (row as i32 * TASK_MGR_ROW_H);
            if row_y + TASK_MGR_ROW_H > list_y + list_h {
                break;
            }
            if self.task_manager_selected == Some(idx) {
                self.fill_rect(
                    Rect::new(12, row_y - 2, self.rect.width.saturating_sub(24), TASK_MGR_ROW_H as u32),
                    Color(0x1E40AF),
                );
            }
            let text = Self::trim_label(line.as_str(), 72);
            let color = if self.task_manager_selected == Some(idx) {
                0xF8FAFC
            } else {
                0xE2E8F0
            };
            self.draw_text(16, row_y as u32, text.as_bytes(), Color(color));
        }

        // Footer buttons
        let footer_y = content_h_i32 - TASK_MGR_FOOTER_H;
        let btn_w = ((w - 30).max(140) / 2).max(120);
        let btn_h = 22;
        let left_x = 12;
        let right_x = left_x + btn_w + 10;
        let row1_y = footer_y + 8;
        let row2_y = row1_y + btn_h + 8;

        let buttons = [
            (left_x, row1_y, btn_w, btn_h, 0xB91C1C, "Cancelar Install"),
            (right_x, row1_y, btn_w, btn_h, 0x9A3412, "Cancelar FS"),
            (left_x, row2_y, btn_w, btn_h, 0x7C2D12, "Cancelar Paste"),
            (right_x, row2_y, btn_w, btn_h, 0x991B1B, "Cancelar Todo"),
        ];

        for (bx, by, bw, bh, color, label) in buttons.iter() {
            if *by + *bh <= content_h_i32 {
                self.fill_rect(Rect::new(*bx, *by, *bw as u32, *bh as u32), Color(*color));
                self.draw_border(Rect::new(*bx, *by, *bw as u32, *bh as u32), Color(0x1F2937));
                let text = Self::trim_label(label, 20);
                self.draw_text((*bx + 8) as u32, (*by + 6) as u32, text.as_bytes(), Color(0xF8FAFC));
            }
        }
    }

    pub fn render_app_runner(&mut self) {
        if self.kind != WindowKind::AppRunner {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        self.fill_rect(
            Rect::new(0, 0, self.rect.width, content_h as u32),
            Color(self.app_runner_background_color),
        );
        self.fill_rect(
            Rect::new(0, 0, self.rect.width, APP_RUNNER_TOP_H as u32),
            Color(0x1E2D3C),
        );
        self.fill_rect(
            Rect::new(0, APP_RUNNER_TOP_H - 1, self.rect.width, 1),
            Color(0x5A7288),
        );

        self.draw_text(10, 9, b"APP RUNNER", Color(0xE9F6FF));
        let src_trim = Self::trim_label(self.app_runner_source_file.as_str(), 64);
        self.draw_text(10, 21, src_trim.as_bytes(), Color(0xBFD8EE));
        let theme_line = alloc::format!("Theme: {}", self.app_runner_theme);
        let theme_trim = Self::trim_label(theme_line.as_str(), 60);
        self.draw_text(10, 33, theme_trim.as_bytes(), Color(0xA9C6DE));

        let canvas = self.app_runner_canvas_rect();
        self.fill_rect(canvas, Color(0xFFFFFF));
        self.draw_border(canvas, Color(0x9CB2C8));

        let inner = Rect::new(
            canvas.x + 10,
            canvas.y + 10,
            canvas.width.saturating_sub(20),
            canvas.height.saturating_sub(20),
        );
        self.fill_rect(inner, Color(self.app_runner_background_color));
        self.draw_border(inner, Color(0xC5D4E2));
        let mut elements = if self.app_runner_elements.is_empty() {
            Self::ide_default_preview_elements(
                self.app_runner_header_text.as_str(),
                self.app_runner_body_text.as_str(),
                self.app_runner_button_label.as_str(),
                self.app_runner_button_id.as_str(),
                self.app_runner_header_color,
                self.app_runner_body_color,
                self.app_runner_button_color,
            )
        } else {
            self.app_runner_elements.clone()
        };
        if elements.is_empty() {
            elements.push(PreviewElement {
                kind: PreviewElementKind::Button,
                text: String::from("Run"),
                id: String::from("action"),
                color: self.app_runner_button_color,
                size: 14,
                margin_top: 0,
                margin_bottom: 0,
                margin_left: 0,
                margin_right: 0,
            });
        }

        self.app_runner_button_targets.clear();
        self.app_runner_button_rect_cached = Rect::new(0, 0, 0, 0);
        self.app_runner_button_rect_valid = false;

        let base_pad = self.app_runner_padding.clamp(0, 64);
        let mut y = inner.y + base_pad;
        let bottom_limit = inner.y + inner.height as i32 - base_pad;
        let mut first_button: Option<Rect> = None;

        for element in elements.iter() {
            if y >= bottom_limit {
                break;
            }
            let mt = element.margin_top.clamp(-128, 256);
            let mb = element.margin_bottom.clamp(-128, 256);
            let ml = element.margin_left.clamp(-128, 256);
            let mr = element.margin_right.clamp(-128, 256);
            y = (y + mt).max(inner.y + base_pad);

            let x = inner.x + base_pad + ml;
            let right = inner.x + inner.width as i32 - base_pad - mr;
            let width = (right - x).max(24);
            let scale = Self::preview_scale_from_size(element.size);
            let char_w = 6 * scale as i32;
            let line_h = 8 * scale as i32;
            let row_step = line_h + 2;
            let max_cols = ((width - 2).max(char_w) / char_w.max(1)) as usize;

            match element.kind {
                PreviewElementKind::Header => {
                    let lines = Self::wrap_text_lines(element.text.as_str(), max_cols.max(1), 3);
                    let mut ly = y;
                    for line in lines.iter() {
                        if ly + line_h >= bottom_limit {
                            break;
                        }
                        self.draw_text_scaled(
                            x.max(0) as u32,
                            ly.max(0) as u32,
                            line.as_bytes(),
                            Color(element.color),
                            scale,
                        );
                        ly += row_step;
                    }
                    y = ly + mb;
                }
                PreviewElementKind::Text => {
                    let lines = Self::wrap_text_lines(element.text.as_str(), max_cols.max(1), 12);
                    let mut ly = y;
                    for line in lines.iter() {
                        if ly + line_h >= bottom_limit {
                            break;
                        }
                        self.draw_text_scaled(
                            x.max(0) as u32,
                            ly.max(0) as u32,
                            line.as_bytes(),
                            Color(element.color),
                            scale,
                        );
                        ly += row_step;
                    }
                    y = ly + mb;
                }
                PreviewElementKind::Button => {
                    let label_max = (max_cols.saturating_sub(2)).max(1);
                    let label = Self::trim_label(element.text.as_str(), label_max);
                    let ideal_w = ((label.len() as i32 + 4) * char_w).clamp(72, width);
                    let btn_w = ideal_w.max(48) as u32;
                    let btn_h = (line_h + 12).clamp(22, 42) as u32;
                    let btn_rect = Rect::new(x, y, btn_w, btn_h);
                    if btn_rect.y + (btn_rect.height as i32) < bottom_limit {
                        self.fill_rect(btn_rect, Color(element.color));
                        self.draw_border(btn_rect, Color(0x1D3247));
                        let text_y = btn_rect.y + ((btn_h as i32 - line_h) / 2).max(1);
                        self.draw_text_scaled(
                            (btn_rect.x + 8).max(0) as u32,
                            text_y.max(0) as u32,
                            label.as_bytes(),
                            Color(0xFFFFFF),
                            scale,
                        );
                        let target_id = if element.id.trim().is_empty() {
                            if self.app_runner_button_id.trim().is_empty() {
                                String::from("action")
                            } else {
                                self.app_runner_button_id.clone()
                            }
                        } else {
                            String::from(element.id.trim())
                        };
                        self.app_runner_button_targets.push((btn_rect, target_id));
                        if first_button.is_none() {
                            first_button = Some(btn_rect);
                        }
                    }
                    y += btn_h as i32 + mb;
                }
            }
        }

        if let Some(btn_rect) = first_button {
            self.app_runner_button_rect_cached = btn_rect;
            self.app_runner_button_rect_valid = true;
        }

        let status_y = (content_h - APP_RUNNER_STATUS_H).max(0);
        self.fill_rect(
            Rect::new(0, status_y, self.rect.width, APP_RUNNER_STATUS_H as u32),
            Color(0xDEE9F3),
        );
        self.fill_rect(Rect::new(0, status_y, self.rect.width, 1), Color(0xA9BFD3));
        let status_trim = Self::trim_label(self.app_runner_status.as_str(), 80);
        self.draw_text(8, (status_y + 10) as u32, status_trim.as_bytes(), Color(0x294359));
    }

    pub fn render_ide_studio(&mut self) {
        if self.kind != WindowKind::IdeStudio {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0x0F172A));
        self.fill_rect(
            Rect::new(0, 0, self.rect.width, IDE_STUDIO_TOP_H as u32),
            Color(0x1A2940),
        );
        self.fill_rect(Rect::new(0, IDE_STUDIO_TOP_H - 1, self.rect.width, 1), Color(0x446080));

        self.draw_text(10, 8, b"REDUX STUDIO (INTERNAL)", Color(0xDDEEFF));
        let project_trim = Self::trim_label(self.ide_project_name.as_str(), 24);
        self.draw_text(
            10,
            20,
            alloc::format!("PROJECT: {}", project_trim).as_bytes(),
            Color(0xAFCCEA),
        );

        let tab_labels = ["RUST", "RUBY", "RML", "RDX", "DOCS"];
        for (idx, label) in tab_labels.iter().enumerate() {
            let rect = self.ide_tab_rect(idx);
            let active = self.ide_active_tab == idx as u8;
            self.fill_rect(rect, Color(if active { 0x2D4A6A } else { 0x26364C }));
            self.draw_border(rect, Color(if active { 0x81B4E1 } else { 0x4F6781 }));
            self.draw_text(
                (rect.x + 16).max(0) as u32,
                (rect.y + 7).max(0) as u32,
                label.as_bytes(),
                Color(if active { 0xFFFFFF } else { 0xC7DBEE }),
            );
        }

        let action_labels = [
            "RESTART",
            "PREVIEW",
            "LINK",
            "RUBY",
            "RUST",
            "LOAD",
            "INSTALL",
            "EXPORT",
        ];
        let action_colors = [
            0x6A2D2D, 0x2E5C86, 0x3E6D48, 0x5C3C83, 0x7A5B26, 0x385B7A, 0x7A2F2F, 0x2B6171,
        ];
        for idx in 0..action_labels.len() {
            let rect = self.ide_action_rect(idx);
            self.fill_rect(rect, Color(action_colors[idx]));
            self.draw_border(rect, Color(0x173047));
            let txt = action_labels[idx];
            let tx = rect.x + ((rect.width as i32 - (txt.len() as i32 * 6)) / 2);
            self.draw_text(
                tx.max(0) as u32,
                (rect.y + 7).max(0) as u32,
                txt.as_bytes(),
                Color(0xFFFFFF),
            );
        }

        let editor = self.ide_editor_rect();
        self.fill_rect(editor, Color(0x0B1220));
        self.draw_border(editor, Color(0x6A89AC));
        let tab_name = match self.ide_active_tab {
            0 => "main.rs",
            1 => "main.rb",
            2 => "main.rml",
            3 => "main.rdx",
            _ => "rdx_syntax.txt",
        };
        self.draw_text(
            (editor.x + 6).max(0) as u32,
            (editor.y + 6).max(0) as u32,
            alloc::format!("EDIT: {}", tab_name).as_bytes(),
            Color(0x92B7DA),
        );
        let (content_src, cursor_raw) = self.ide_active_text_and_cursor();
        let content = String::from(content_src);
        let cursor = Self::ide_clamp_cursor_index(content.as_str(), cursor_raw);
        let max_cols = Self::ide_editor_max_cols(editor).max(1);
        let max_lines = Self::ide_editor_max_lines(editor).max(1);
        let (line_start, col_start, cursor_line, cursor_col) =
            Self::ide_viewport_origin(content.as_str(), cursor, max_cols, max_lines);
        let starts = Self::ide_line_starts(content.as_str());
        let (sel_raw_start, sel_raw_end) = self.ide_active_selection();
        let (sel_start, sel_end) =
            Self::ide_clamp_selection_for_text(content.as_str(), sel_raw_start, sel_raw_end);
        let has_selection = sel_start < sel_end;
        let text_x = editor.x + IDE_STUDIO_EDITOR_TEXT_X;
        let text_y = editor.y + IDE_STUDIO_EDITOR_TEXT_Y;
        let tab_for_highlight = self.ide_active_tab;
        let mut in_block_comment =
            Self::ide_comment_block_state_before(content.as_str(), starts.as_slice(), line_start, tab_for_highlight);

        for row in 0..max_lines {
            let line_idx = line_start + row;
            if line_idx >= starts.len() {
                break;
            }
            let (line_beg, line_end) = Self::ide_line_bounds(content.as_str(), starts.as_slice(), line_idx);
            let full_line = &content[line_beg..line_end];
            let seg_start = Self::ide_byte_index_for_col(full_line, col_start);
            let seg_end = Self::ide_byte_index_for_col(full_line, col_start + max_cols);
            let display = &full_line[seg_start..seg_end];
            if has_selection {
                let row_sel_start = sel_start.max(line_beg);
                let row_sel_end = sel_end.min(line_end);
                if row_sel_start < row_sel_end {
                    let line_sel_start = row_sel_start.saturating_sub(line_beg);
                    let line_sel_end = row_sel_end.saturating_sub(line_beg);
                    let sel_col_start = Self::ide_col_for_byte_index(full_line, line_sel_start);
                    let sel_col_end = Self::ide_col_for_byte_index(full_line, line_sel_end);
                    let vis_col_start = sel_col_start.max(col_start);
                    let vis_col_end = sel_col_end.min(col_start + max_cols);
                    if vis_col_start < vis_col_end {
                        let highlight_x = text_x + (vis_col_start - col_start) as i32 * IDE_STUDIO_EDITOR_CHAR_W;
                        let highlight_w =
                            ((vis_col_end - vis_col_start) as i32 * IDE_STUDIO_EDITOR_CHAR_W).max(1);
                        let highlight_y = text_y + row as i32 * IDE_STUDIO_EDITOR_LINE_H;
                        self.fill_rect(
                            Rect::new(
                                highlight_x,
                                highlight_y,
                                highlight_w as u32,
                                (IDE_STUDIO_EDITOR_LINE_H - 1).max(1) as u32,
                            ),
                            Color(0x30588C),
                        );
                    }
                }
            }
            let mut painted = false;
            let comment_segments =
                Self::ide_comment_line_segments(full_line, tab_for_highlight, &mut in_block_comment);
            if !comment_segments.is_empty() {
                let row_y = (text_y + row as i32 * IDE_STUDIO_EDITOR_LINE_H).max(0) as u32;
                for (seg_line_start, seg_line_end, is_comment) in comment_segments.iter() {
                    let clip_start = (*seg_line_start).max(seg_start);
                    let clip_end = (*seg_line_end).min(seg_end);
                    if clip_start >= clip_end {
                        continue;
                    }
                    let draw_col = Self::ide_col_for_byte_index(full_line, clip_start);
                    let draw_x = text_x + (draw_col.saturating_sub(col_start) as i32 * IDE_STUDIO_EDITOR_CHAR_W);
                    let slice = &full_line[clip_start..clip_end];
                    self.draw_text(
                        draw_x.max(0) as u32,
                        row_y,
                        slice.as_bytes(),
                        Color(if *is_comment { 0x7D8FA7 } else { 0xE3EDF8 }),
                    );
                    painted = true;
                }
            }
            if !painted {
                self.draw_text(
                    text_x.max(0) as u32,
                    (text_y + row as i32 * IDE_STUDIO_EDITOR_LINE_H).max(0) as u32,
                    display.as_bytes(),
                    Color(0xE3EDF8),
                );
            }
        }

        let caret_row = cursor_line.saturating_sub(line_start).min(max_lines.saturating_sub(1));
        let caret_col = cursor_col.saturating_sub(col_start).min(max_cols.saturating_sub(1));
        let caret_x = text_x + caret_col as i32 * IDE_STUDIO_EDITOR_CHAR_W;
        let caret_y = text_y + caret_row as i32 * IDE_STUDIO_EDITOR_LINE_H;
        self.fill_rect(Rect::new(caret_x, caret_y, 2, 8), Color(0xE3EDF8));

        let preview = self.ide_preview_rect();
        self.fill_rect(preview, Color(0x0E1726));
        self.draw_border(preview, Color(0x5F7A97));
        self.draw_text(
            (preview.x + 6).max(0) as u32,
            (preview.y + 6).max(0) as u32,
            b"RML PREVIEW",
            Color(0xCFE5FF),
        );
        self.draw_text(
            (preview.x + 62).max(0) as u32,
            (preview.y + 6).max(0) as u32,
            b"VIEW ID:",
            Color(0xAFC9E4),
        );

        let view_input_rect = self.ide_view_input_rect();
        let view_go_rect = self.ide_view_go_rect();
        self.fill_rect(
            view_input_rect,
            Color(if self.ide_preview_view_input_active {
                0x0D2238
            } else {
                0x16273A
            }),
        );
        self.draw_border(
            view_input_rect,
            Color(if self.ide_preview_view_input_active {
                0x8EC5F4
            } else {
                0x5E7E9F
            }),
        );
        let view_input_trim = if self.ide_preview_view_input.trim().is_empty() {
            String::from("<id>")
        } else {
            Self::trim_label(
                self.ide_preview_view_input.as_str(),
                ((view_input_rect.width as i32 - 6).max(6) / 6) as usize,
            )
        };
        self.draw_text(
            (view_input_rect.x + 3).max(0) as u32,
            (view_input_rect.y + 4).max(0) as u32,
            view_input_trim.as_bytes(),
            Color(if self.ide_preview_view_input.trim().is_empty() {
                0x8FA8C2
            } else {
                0xE7F3FF
            }),
        );
        self.fill_rect(view_go_rect, Color(0x1D7FAE));
        self.draw_border(view_go_rect, Color(0x114A66));
        self.draw_text(
            (view_go_rect.x + 10).max(0) as u32,
            (view_go_rect.y + 5).max(0) as u32,
            b"GO",
            Color(0xFFFFFF),
        );

        let panel = Rect::new(
            preview.x + 8,
            preview.y + 36,
            preview.width.saturating_sub(16),
            preview.height.saturating_sub(44),
        );
        self.fill_rect(panel, Color(self.ide_preview_background_color));
        self.draw_border(panel, Color(0x9CB2C8));

        self.ide_preview_button_rect_valid = false;
        self.ide_preview_button_rect_cached = Rect::new(0, 0, 0, 0);
        if let Some(btn_rect) = self.ide_render_preview_elements(panel) {
            self.ide_preview_button_rect_cached = btn_rect;
            self.ide_preview_button_rect_valid = true;
        }

        let max_prev_cols = ((panel.width as i32 - 12).max(6) / 6) as usize;
        let event_trim = Self::trim_label(self.ide_preview_event.as_str(), max_prev_cols);
        let event_h = 12i32;
        let event_y = (panel.y + panel.height as i32 - event_h - 1).max(panel.y + 2);
        self.fill_rect(
            Rect::new(panel.x + 1, event_y - 1, panel.width.saturating_sub(2), event_h as u32),
            Color(0x1A2B3D),
        );
        self.draw_text(
            (panel.x + 6).max(0) as u32,
            event_y.max(0) as u32,
            event_trim.as_bytes(),
            Color(0xBFD5EA),
        );

        let status = self.ide_status_rect();
        self.fill_rect(status, Color(0x152235));
        self.fill_rect(Rect::new(status.x, status.y, status.width, 1), Color(0x4B6785));
        let status_trim = Self::trim_label(self.ide_status.as_str(), 112);
        self.draw_text(
            (status.x + 8).max(0) as u32,
            (status.y + 8).max(0) as u32,
            status_trim.as_bytes(),
            Color(0xD8EAFD),
        );
        let hint = alloc::format!(
            "TAB={} cursor=L{}:C{} theme={} view={} btn_id={}",
            tab_name,
            cursor_line + 1,
            cursor_col + 1,
            self.ide_preview_theme,
            if self.ide_preview_active_view_id.is_empty() {
                "<default>"
            } else {
                self.ide_preview_active_view_id.as_str()
            },
            if self.ide_preview_button_id.is_empty() {
                "<none>"
            } else {
                self.ide_preview_button_id.as_str()
            }
        );
        self.draw_text(
            (status.x + 8).max(0) as u32,
            (status.y + 19).max(0) as u32,
            Self::trim_label(hint.as_str(), 112).as_bytes(),
            Color(0xA9C4DF),
        );
    }

    pub fn render_doom_launcher(&mut self) {
        if self.kind != WindowKind::DoomLauncher {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0x111619));
        self.fill_rect(
            Rect::new(0, 0, self.rect.width, DOOM_LAUNCHER_TOP_H as u32),
            Color(0x2A1717),
        );
        self.fill_rect(
            Rect::new(0, DOOM_LAUNCHER_TOP_H - 1, self.rect.width, 1),
            Color(0x6E3333),
        );

        self.draw_text(10, 10, b"CPP-DOOM NATIVE", Color(0xFFE3D0));
        self.draw_text(10, 23, b"Renderer integrado en GUI (sin runloop Linux)", Color(0xD7B9A5));
        self.draw_text(10, 35, b"WASD/Flechas mover, Q/E girar, SPACE disparar", Color(0xC6A695));

        let canvas = self.doom_launcher_canvas_rect();
        self.fill_rect(canvas, Color(0x111923));
        self.draw_border(canvas, Color(0x445765));

        if self.doom_native_running {
            self.doom_native_draw_scene(canvas);
            self.doom_native_draw_minimap(canvas);
            self.doom_native_draw_hud(canvas);
        } else {
            self.draw_text(
                (canvas.x + 12).max(0) as u32,
                (canvas.y + 12).max(0) as u32,
                b"Click para iniciar modo CPP-DOOM nativo.",
                Color(0xD8E6EF),
            );
            self.draw_text(
                (canvas.x + 12).max(0) as u32,
                (canvas.y + 24).max(0) as u32,
                b"Sin rutas EFI/APPS: corre directo en esta ventana.",
                Color(0x9FB6C7),
            );
        }

        let btn = self.doom_launch_button_rect();
        self.fill_rect(btn, if self.doom_native_running { Color(0x6C1F1F) } else { Color(0xC23B22) });
        self.draw_border(btn, Color(0x5C140B));
        self.draw_text(
            (btn.x + 14).max(0) as u32,
            (btn.y + if self.doom_native_running { 9 } else { 12 }).max(0) as u32,
            if self.doom_native_running {
                b"DETENER SESION"
            } else {
                b"INICIAR CPP-DOOM"
            },
            Color(0xFFFFFF),
        );

        let status_rect = self.doom_status_rect();
        self.fill_rect(status_rect, Color(0x172128));
        self.fill_rect(
            Rect::new(status_rect.x, status_rect.y, status_rect.width, 1),
            Color(0x415261),
        );
        let status_trim = Self::trim_label(self.doom_status.as_str(), 82);
        self.draw_text(
            (status_rect.x + 8).max(0) as u32,
            (status_rect.y + 10).max(0) as u32,
            status_trim.as_bytes(),
            Color(0xCFE4F2),
        );
    }

    pub fn render_linux_bridge(&mut self) {
        if self.kind != WindowKind::LinuxBridge {
            return;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return;
        }

        self.fill_rect(Rect::new(0, 0, self.rect.width, content_h as u32), Color(0x0D1620));
        self.fill_rect(Rect::new(0, 0, self.rect.width, LINUX_BRIDGE_TOP_H as u32), Color(0x162838));
        self.fill_rect(
            Rect::new(0, LINUX_BRIDGE_TOP_H - 1, self.rect.width, 1),
            Color(0x3F5F79),
        );

        self.draw_text(10, 10, b"LINUX BRIDGE", Color(0xE2F2FF));
        let src_trim = Self::trim_label(self.linux_bridge_source.as_str(), 62);
        self.draw_text(10, 23, src_trim.as_bytes(), Color(0xB8D4EA));
        let info = if self.linux_bridge_width == 0 || self.linux_bridge_height == 0 {
            String::from("Sin frame")
        } else {
            alloc::format!("{}x{} px", self.linux_bridge_width, self.linux_bridge_height)
        };
        self.draw_text(10, 35, info.as_bytes(), Color(0x9AC0DE));

        let canvas = self.linux_bridge_canvas_rect();
        self.fill_rect(canvas, Color(0x09131C));
        self.draw_border(canvas, Color(0x365067));

        let img_w = self.linux_bridge_width as usize;
        let img_h = self.linux_bridge_height as usize;
        if img_w > 0
            && img_h > 0
            && self.linux_bridge_pixels.len() >= img_w.saturating_mul(img_h)
        {
            let avail_w = (canvas.width as i32 - 10).max(1) as usize;
            let avail_h = (canvas.height as i32 - 10).max(1) as usize;

            let mut draw_w = avail_w;
            let mut draw_h = (img_h.saturating_mul(draw_w)).max(1) / img_w.max(1);
            if draw_h > avail_h {
                draw_h = avail_h;
                draw_w = (img_w.saturating_mul(draw_h)).max(1) / img_h.max(1);
            }
            draw_w = draw_w.max(1).min(avail_w);
            draw_h = draw_h.max(1).min(avail_h);

            let start_x = canvas.x + ((canvas.width as i32 - draw_w as i32) / 2);
            let start_y = canvas.y + ((canvas.height as i32 - draw_h as i32) / 2);

            for dy in 0..draw_h {
                let sy = dy.saturating_mul(img_h) / draw_h.max(1);
                for dx in 0..draw_w {
                    let sx = dx.saturating_mul(img_w) / draw_w.max(1);
                    let src_idx = sy.saturating_mul(img_w).saturating_add(sx);
                    if src_idx >= self.linux_bridge_pixels.len() {
                        continue;
                    }
                    self.draw_pixel(
                        (start_x + dx as i32).max(0) as u32,
                        (start_y + dy as i32).max(0) as u32,
                        Color(self.linux_bridge_pixels[src_idx]),
                    );
                }
            }
        } else {
            self.draw_text(
                (canvas.x + 10).max(0) as u32,
                (canvas.y + 14).max(0) as u32,
                b"Esperando salida grafica Linux (SDL/X11 subset).",
                Color(0x7FA1BE),
            );
        }

        let status_rect = self.linux_bridge_status_rect();
        self.fill_rect(status_rect, Color(0x12202D));
        self.fill_rect(
            Rect::new(status_rect.x, status_rect.y, status_rect.width, 1),
            Color(0x365067),
        );
        let status_trim = Self::trim_label(self.linux_bridge_status.as_str(), 82);
        self.draw_text(
            (status_rect.x + 8).max(0) as u32,
            (status_rect.y + 10).max(0) as u32,
            status_trim.as_bytes(),
            Color(0xCCE6FA),
        );
    }

    pub fn set_explorer_home(&mut self) {
        if self.kind != WindowKind::Explorer {
            return;
        }

        self.explorer_current_cluster = 0;
        self.explorer_device_index = None;
        self.explorer_path = String::from("Quick Access");
        self.explorer_status = String::from("Select a storage volume to browse FAT32/exFAT.");
        self.explorer_preview_lines.clear();
        self.explorer_scroll = 0;
        self.explorer_items = alloc::vec![
            ExplorerItem::new("Desktop", ExplorerItemKind::ShortcutDesktop, 0, 0),
            ExplorerItem::new("Downloads", ExplorerItemKind::ShortcutDownloads, 0, 0),
            ExplorerItem::new("Documents", ExplorerItemKind::ShortcutDocuments, 0, 0),
            ExplorerItem::new("Images", ExplorerItemKind::ShortcutImages, 0, 0),
            ExplorerItem::new("Videos", ExplorerItemKind::ShortcutVideos, 0, 0),
            ExplorerItem::new("Storage", ExplorerItemKind::ShortcutUsb, 0, 0),
        ];
        self.explorer_search_source_items = self.explorer_items.clone();
        self.explorer_search_query.clear();
        self.explorer_search_input_active = false;
        self.explorer_search_active = false;

        self.render();
    }

    pub fn set_explorer_listing(
        &mut self,
        path: &str,
        cluster: u32,
        device_index: Option<usize>,
        items: Vec<ExplorerItem>,
    ) {
        if self.kind != WindowKind::Explorer {
            return;
        }

        self.explorer_path = String::from(path);
        self.explorer_current_cluster = cluster;
        self.explorer_device_index = device_index;
        self.explorer_search_source_items = items.clone();
        self.explorer_items = items;
        self.explorer_preview_lines.clear();
        self.explorer_scroll = 0;
        self.explorer_search_query.clear();
        self.explorer_search_input_active = false;
        self.explorer_search_active = false;
        self.render();
    }

    pub fn set_explorer_status(&mut self, status: &str) {
        if self.kind != WindowKind::Explorer {
            return;
        }
        self.explorer_status = String::from(status);
        self.render();
    }

    pub fn set_explorer_preview(&mut self, status: &str, preview_lines: Vec<String>) {
        if self.kind != WindowKind::Explorer {
            return;
        }
        self.explorer_status = String::from(status);
        self.explorer_preview_lines = preview_lines;
        self.render();
    }

    pub fn load_image_viewer(
        &mut self,
        file_name: &str,
        width: u32,
        height: u32,
        pixels: Vec<u32>,
        status: &str,
    ) {
        if self.kind != WindowKind::ImageViewer {
            return;
        }
        self.image_viewer_file_name = String::from(file_name);
        self.image_viewer_width = width;
        self.image_viewer_height = height;
        self.image_viewer_pixels = pixels;
        self.image_viewer_status = String::from(status);
        self.render();
    }

    pub fn load_app_runner_layout(
        &mut self,
        source_file: &str,
        theme: &str,
        header_text: &str,
        body_text: &str,
        button_label: &str,
        background_color: u32,
        header_color: u32,
        body_color: u32,
        button_color: u32,
        padding: i32,
        elements: Vec<PreviewElement>,
        status: &str,
    ) {
        if self.kind != WindowKind::AppRunner {
            return;
        }
        self.app_runner_source_file = String::from(source_file);
        self.app_runner_rml_source.clear();
        self.app_runner_active_view_id.clear();
        self.app_runner_theme = String::from(theme);
        self.app_runner_header_text = String::from(header_text);
        self.app_runner_body_text = String::from(body_text);
        self.app_runner_button_label = String::from(button_label);
        self.app_runner_rdx_source.clear();
        self.app_runner_rust_source.clear();
        self.app_runner_background_color = background_color;
        self.app_runner_header_color = header_color;
        self.app_runner_body_color = body_color;
        self.app_runner_button_color = button_color;
        self.app_runner_padding = padding.clamp(0, 64);

        let mut normalized = if elements.is_empty() {
            Self::ide_default_preview_elements(
                header_text,
                body_text,
                button_label,
                self.app_runner_button_id.as_str(),
                header_color,
                body_color,
                button_color,
            )
        } else {
            elements
        };

        if let Some(btn) = normalized
            .iter()
            .find(|el| el.kind == PreviewElementKind::Button)
        {
            self.app_runner_button_label = String::from(btn.text.as_str());
            self.app_runner_button_color = btn.color;
            if !btn.id.trim().is_empty() {
                self.app_runner_button_id = String::from(btn.id.as_str());
            }
        } else if self.app_runner_button_id.trim().is_empty() {
            self.app_runner_button_id = String::from("action");
        }

        if normalized.is_empty() {
            normalized = Self::ide_default_preview_elements(
                header_text,
                body_text,
                button_label,
                self.app_runner_button_id.as_str(),
                header_color,
                body_color,
                button_color,
            );
        }
        self.app_runner_elements = normalized;
        self.app_runner_button_targets.clear();
        self.app_runner_button_rect_cached = Rect::new(0, 0, 0, 0);
        self.app_runner_button_rect_valid = false;
        self.app_runner_status = String::from(status);
        self.render();
    }

    pub fn ide_set_status(&mut self, status: &str) {
        if self.kind != WindowKind::IdeStudio {
            return;
        }
        self.ide_status = String::from(status);
        self.render();
    }

    pub fn ide_set_preview_event(&mut self, event: &str) {
        if self.kind != WindowKind::IdeStudio {
            return;
        }
        self.ide_preview_event = String::from(event);
        self.render();
    }

    pub fn ide_set_view_input_focus(&mut self, active: bool) {
        if self.kind != WindowKind::IdeStudio {
            return;
        }
        if self.ide_preview_view_input_active == active {
            return;
        }
        self.ide_preview_view_input_active = active;
        self.render();
    }

    pub fn ide_view_input_text(&self) -> String {
        if self.kind != WindowKind::IdeStudio {
            return String::new();
        }
        String::from(self.ide_preview_view_input.as_str())
    }

    pub fn ide_set_active_tab(&mut self, tab: u8) {
        if self.kind != WindowKind::IdeStudio {
            return;
        }
        self.ide_active_tab = tab.min(4);
        self.ide_preview_view_input_active = false;
        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        let clamped_cursor = Self::ide_clamp_cursor_index(target.as_str(), *cursor);
        let (clamped_start, clamped_end) =
            Self::ide_clamp_selection_for_text(target.as_str(), *sel_start, *sel_end);
        *cursor = clamped_cursor;
        *sel_start = clamped_start;
        *sel_end = clamped_end;
        self.render();
    }

    pub fn ide_load_project_sources(
        &mut self,
        project_name: &str,
        rust_src: &str,
        ruby_src: &str,
        rml_src: &str,
        rdx_src: &str,
    ) {
        if self.kind != WindowKind::IdeStudio {
            return;
        }

        let clean_name = project_name.trim();
        self.ide_project_name = if clean_name.is_empty() {
            String::from("IDEAPP")
        } else {
            String::from(clean_name)
        };

        self.ide_rust_text = String::from(rust_src);
        self.ide_ruby_text = String::from(ruby_src);
        self.ide_rml_text = String::from(rml_src);
        self.ide_rdx_text = String::from(rdx_src);

        self.ide_active_tab = 2;
        self.ide_cursor_rust = 0;
        self.ide_cursor_ruby = 0;
        self.ide_cursor_rml = 0;
        self.ide_cursor_rdx = 0;
        self.ide_cursor_docs = 0;
        self.ide_sel_start_rust = 0;
        self.ide_sel_end_rust = 0;
        self.ide_sel_start_ruby = 0;
        self.ide_sel_end_ruby = 0;
        self.ide_sel_start_rml = 0;
        self.ide_sel_end_rml = 0;
        self.ide_sel_start_rdx = 0;
        self.ide_sel_end_rdx = 0;
        self.ide_sel_start_docs = 0;
        self.ide_sel_end_docs = 0;
        self.ide_undo_stack.clear();
        self.ide_redo_stack.clear();
        self.ide_preview_view_input.clear();
        self.ide_preview_view_input_active = false;
        self.ide_preview_active_view_id.clear();
        self.ide_last_export_rust = self.ide_rust_text.clone();
        self.ide_last_export_ruby = self.ide_ruby_text.clone();
        self.ide_last_export_rml = self.ide_rml_text.clone();
        self.ide_last_export_rdx = self.ide_rdx_text.clone();

        self.render();
    }

    pub fn ide_mark_export_checkpoint(&mut self) {
        if self.kind != WindowKind::IdeStudio {
            return;
        }
        self.ide_last_export_rust = self.ide_rust_text.clone();
        self.ide_last_export_ruby = self.ide_ruby_text.clone();
        self.ide_last_export_rml = self.ide_rml_text.clone();
        self.ide_last_export_rdx = self.ide_rdx_text.clone();
    }

    pub fn ide_has_unexported_changes(&self) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        self.ide_rust_text != self.ide_last_export_rust
            || self.ide_ruby_text != self.ide_last_export_ruby
            || self.ide_rml_text != self.ide_last_export_rml
            || self.ide_rdx_text != self.ide_last_export_rdx
    }

    pub fn ide_cursor_index(&self) -> usize {
        if self.kind != WindowKind::IdeStudio {
            return 0;
        }
        let (text, cursor) = self.ide_active_text_and_cursor();
        Self::ide_clamp_cursor_index(text, cursor)
    }

    pub fn ide_has_selection(&self) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let (text, _) = self.ide_active_text_and_cursor();
        let (sel_start, sel_end) = self.ide_active_selection();
        let (start, end) = Self::ide_clamp_selection_for_text(text, sel_start, sel_end);
        start < end
    }

    pub fn ide_selected_text(&self) -> Option<String> {
        if self.kind != WindowKind::IdeStudio {
            return None;
        }
        let (text, _) = self.ide_active_text_and_cursor();
        let (sel_start, sel_end) = self.ide_active_selection();
        let (start, end) = Self::ide_clamp_selection_for_text(text, sel_start, sel_end);
        if start >= end {
            return None;
        }
        Some(String::from(&text[start..end]))
    }

    pub fn ide_clear_selection(&mut self) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let cursor = self.ide_cursor_index();
        let (sel_start, sel_end) = self.ide_active_selection_mut();
        if *sel_start == cursor && *sel_end == cursor {
            return false;
        }
        *sel_start = cursor;
        *sel_end = cursor;
        self.render();
        true
    }

    pub fn ide_can_undo(&self) -> bool {
        if self.kind != WindowKind::IdeStudio || self.ide_active_tab == 4 {
            return false;
        }
        let tab = self.ide_active_tab;
        self.ide_undo_stack.iter().any(|s| s.tab == tab)
    }

    pub fn ide_can_redo(&self) -> bool {
        if self.kind != WindowKind::IdeStudio || self.ide_active_tab == 4 {
            return false;
        }
        let tab = self.ide_active_tab;
        self.ide_redo_stack.iter().any(|s| s.tab == tab)
    }

    pub fn ide_undo(&mut self) -> bool {
        if self.kind != WindowKind::IdeStudio || self.ide_active_tab == 4 {
            return false;
        }
        let tab = self.ide_active_tab;
        let Some(idx) = self.ide_undo_stack.iter().rposition(|s| s.tab == tab) else {
            return false;
        };
        let snapshot = self.ide_undo_stack.remove(idx);
        let current = self.ide_snapshot_for_tab(tab);
        if !Self::ide_snapshot_same(&current, &snapshot) {
            self.ide_redo_stack.push(current);
            if self.ide_redo_stack.len() > IDE_STUDIO_UNDO_STACK_LIMIT {
                self.ide_redo_stack.remove(0);
            }
        }
        self.ide_apply_snapshot(&snapshot);
        self.render();
        true
    }

    pub fn ide_redo(&mut self) -> bool {
        if self.kind != WindowKind::IdeStudio || self.ide_active_tab == 4 {
            return false;
        }
        let tab = self.ide_active_tab;
        let Some(idx) = self.ide_redo_stack.iter().rposition(|s| s.tab == tab) else {
            return false;
        };
        let snapshot = self.ide_redo_stack.remove(idx);
        let current = self.ide_snapshot_for_tab(tab);
        if !Self::ide_snapshot_same(&current, &snapshot) {
            self.ide_undo_stack.push(current);
            if self.ide_undo_stack.len() > IDE_STUDIO_UNDO_STACK_LIMIT {
                self.ide_undo_stack.remove(0);
            }
        }
        self.ide_apply_snapshot(&snapshot);
        self.render();
        true
    }

    pub fn ide_cut_selected_text(&mut self) -> Option<String> {
        if self.kind != WindowKind::IdeStudio || self.ide_active_tab == 4 {
            return None;
        }
        let selected = self.ide_selected_text()?;
        if !self.ide_delete_selection_only() {
            return None;
        }
        self.render();
        Some(selected)
    }

    pub fn ide_paste_text(&mut self, text: &str) -> bool {
        if self.kind != WindowKind::IdeStudio || self.ide_active_tab == 4 {
            return false;
        }

        let mut normalized = String::new();
        for ch in text.chars() {
            if ch == '\n' || (ch.is_ascii() && !ch.is_control()) {
                normalized.push(ch);
            }
        }
        if normalized.is_empty() {
            return false;
        }
        if !self.ide_insert_text_at_cursor_or_selection(normalized.as_str()) {
            return false;
        }
        self.render();
        true
    }

    pub fn ide_set_preview(
        &mut self,
        theme: &str,
        header_text: &str,
        body_text: &str,
        button_label: &str,
        button_id: &str,
        background_color: u32,
        header_color: u32,
        body_color: u32,
        button_color: u32,
        status: &str,
    ) {
        let default_elements = Self::ide_default_preview_elements(
            header_text,
            body_text,
            button_label,
            button_id,
            header_color,
            body_color,
            button_color,
        );
        self.ide_set_preview_layout(
            theme,
            header_text,
            body_text,
            button_label,
            button_id,
            background_color,
            header_color,
            body_color,
            button_color,
            10,
            default_elements,
            status,
        );
    }

    pub fn ide_set_preview_layout(
        &mut self,
        theme: &str,
        header_text: &str,
        body_text: &str,
        button_label: &str,
        button_id: &str,
        background_color: u32,
        header_color: u32,
        body_color: u32,
        button_color: u32,
        padding: i32,
        elements: Vec<PreviewElement>,
        status: &str,
    ) {
        if self.kind != WindowKind::IdeStudio {
            return;
        }
        self.ide_preview_theme = String::from(theme);
        self.ide_preview_header_text = String::from(header_text);
        self.ide_preview_body_text = String::from(body_text);
        self.ide_preview_button_label = String::from(button_label);
        self.ide_preview_button_id = String::from(button_id);
        self.ide_preview_background_color = background_color;
        self.ide_preview_header_color = header_color;
        self.ide_preview_body_color = body_color;
        self.ide_preview_button_color = button_color;
        self.ide_preview_padding = padding.clamp(0, 64);

        let mut normalized = if elements.is_empty() {
            Self::ide_default_preview_elements(
                header_text,
                body_text,
                button_label,
                button_id,
                header_color,
                body_color,
                button_color,
            )
        } else {
            elements
        };

        if let Some(btn) = normalized.iter().find(|el| el.kind == PreviewElementKind::Button) {
            self.ide_preview_button_label = String::from(btn.text.as_str());
            self.ide_preview_button_color = btn.color;
            if !btn.id.trim().is_empty() {
                self.ide_preview_button_id = String::from(btn.id.as_str());
            }
        } else {
            self.ide_preview_button_id.clear();
        }

        if normalized.is_empty() {
            normalized = Self::ide_default_preview_elements(
                header_text,
                body_text,
                button_label,
                button_id,
                header_color,
                body_color,
                button_color,
            );
        }
        self.ide_preview_elements = normalized;

        self.ide_preview_event = if button_id.trim().is_empty() {
            String::from("Preview listo: boton sin id.")
        } else {
            alloc::format!("Preview listo: boton id='{}'.", self.ide_preview_button_id.as_str())
        };
        self.ide_status = String::from(status);
        self.ide_preview_button_rect_valid = false;
        self.ide_preview_button_rect_cached = Rect::new(0, 0, 0, 0);
        self.ide_preview_button_targets.clear();
        self.render();
    }

    pub fn ide_set_preview_text_by_id(&mut self, element_id: &str, text: &str) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let needle = element_id.trim();
        if needle.is_empty() {
            return false;
        }
        let mut changed = false;
        for el in self.ide_preview_elements.iter_mut() {
            if el.id.trim().is_empty() {
                continue;
            }
            if !el.id.eq_ignore_ascii_case(needle) {
                continue;
            }
            if el.text != text {
                el.text = String::from(text);
                changed = true;
            }
            match el.kind {
                PreviewElementKind::Header => {
                    self.ide_preview_header_text = String::from(text);
                }
                PreviewElementKind::Text => {
                    self.ide_preview_body_text = String::from(text);
                }
                PreviewElementKind::Button => {
                    self.ide_preview_button_label = String::from(text);
                }
            }
            break;
        }
        if changed {
            self.ide_preview_button_rect_valid = false;
            self.ide_preview_button_rect_cached = Rect::new(0, 0, 0, 0);
            self.ide_preview_button_targets.clear();
            self.render();
        }
        changed
    }

    pub fn app_runner_set_text_by_id(&mut self, element_id: &str, text: &str) -> bool {
        if self.kind != WindowKind::AppRunner {
            return false;
        }
        let needle = element_id.trim();
        if needle.is_empty() {
            return false;
        }
        let mut changed = false;
        for el in self.app_runner_elements.iter_mut() {
            if el.id.trim().is_empty() {
                continue;
            }
            if !el.id.eq_ignore_ascii_case(needle) {
                continue;
            }
            if el.text != text {
                el.text = String::from(text);
                changed = true;
            }
            match el.kind {
                PreviewElementKind::Header => {
                    self.app_runner_header_text = String::from(text);
                }
                PreviewElementKind::Text => {
                    self.app_runner_body_text = String::from(text);
                }
                PreviewElementKind::Button => {
                    self.app_runner_button_label = String::from(text);
                }
            }
            break;
        }
        if changed {
            self.app_runner_button_rect_valid = false;
            self.app_runner_button_rect_cached = Rect::new(0, 0, 0, 0);
            self.app_runner_button_targets.clear();
        }
        changed
    }

    pub fn ide_preview_button_id_at(&self, global_x: i32, global_y: i32) -> Option<String> {
        if self.kind != WindowKind::IdeStudio {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }
        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };
        for (rect, id) in self.ide_preview_button_targets.iter() {
            if rect.contains(p) {
                if id.trim().is_empty() {
                    if self.ide_preview_button_id.trim().is_empty() {
                        return Some(String::from("action"));
                    }
                    return Some(self.ide_preview_button_id.clone());
                }
                return Some(id.clone());
            }
        }
        None
    }

    pub fn ide_move_cursor_left(&mut self) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        let cur = Self::ide_clamp_cursor_index(target.as_str(), *cursor);
        let new_cur = Self::ide_prev_cursor_index(target.as_str(), cur);
        if new_cur == cur {
            return false;
        }
        *cursor = new_cur;
        *sel_start = new_cur;
        *sel_end = new_cur;
        self.render();
        true
    }

    pub fn ide_move_cursor_right(&mut self) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        let cur = Self::ide_clamp_cursor_index(target.as_str(), *cursor);
        let new_cur = Self::ide_next_cursor_index(target.as_str(), cur);
        if new_cur == cur {
            return false;
        }
        *cursor = new_cur;
        *sel_start = new_cur;
        *sel_end = new_cur;
        self.render();
        true
    }

    pub fn ide_move_cursor_up(&mut self) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        let cur = Self::ide_clamp_cursor_index(target.as_str(), *cursor);
        let (line, col) = Self::ide_cursor_line_col(target.as_str(), cur);
        if line == 0 {
            return false;
        }
        let new_cur = Self::ide_cursor_from_line_col(target.as_str(), line - 1, col);
        if new_cur == cur {
            return false;
        }
        *cursor = new_cur;
        *sel_start = new_cur;
        *sel_end = new_cur;
        self.render();
        true
    }

    pub fn ide_move_cursor_down(&mut self) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        let cur = Self::ide_clamp_cursor_index(target.as_str(), *cursor);
        let (line, col) = Self::ide_cursor_line_col(target.as_str(), cur);
        let line_count = Self::ide_line_starts(target.as_str()).len();
        if line + 1 >= line_count {
            return false;
        }
        let new_cur = Self::ide_cursor_from_line_col(target.as_str(), line + 1, col);
        if new_cur == cur {
            return false;
        }
        *cursor = new_cur;
        *sel_start = new_cur;
        *sel_end = new_cur;
        self.render();
        true
    }

    pub fn ide_set_cursor_from_point(&mut self, global_x: i32, global_y: i32) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let Some(new_cursor) = self.ide_cursor_from_point(global_x, global_y) else {
            return false;
        };

        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        let clamped_new = Self::ide_clamp_cursor_index(target.as_str(), new_cursor);
        let cursor_changed = *cursor != clamped_new;
        let selection_changed = *sel_start != clamped_new || *sel_end != clamped_new;
        if !cursor_changed && !selection_changed {
            return false;
        }
        *cursor = clamped_new;
        *sel_start = clamped_new;
        *sel_end = clamped_new;
        self.render();
        true
    }

    pub fn ide_select_to_point(&mut self, global_x: i32, global_y: i32, anchor: usize) -> bool {
        if self.kind != WindowKind::IdeStudio {
            return false;
        }
        let Some(new_cursor_raw) = self.ide_cursor_from_point(global_x, global_y) else {
            return false;
        };

        let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
        let clamped_cursor = Self::ide_clamp_cursor_index(target.as_str(), new_cursor_raw);
        let clamped_anchor = Self::ide_clamp_cursor_index(target.as_str(), anchor);
        let (next_start, next_end) =
            Self::ide_clamp_selection_for_text(target.as_str(), clamped_anchor, clamped_cursor);

        let changed =
            *cursor != clamped_cursor || *sel_start != next_start || *sel_end != next_end;
        if !changed {
            return false;
        }
        *cursor = clamped_cursor;
        *sel_start = next_start;
        *sel_end = next_end;
        self.render();
        true
    }

    pub fn ide_action_at(&self, global_x: i32, global_y: i32) -> Option<IdeStudioClickAction> {
        if self.kind != WindowKind::IdeStudio {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }
        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };

        if self.ide_tab_rect(0).contains(p) {
            return Some(IdeStudioClickAction::TabRust);
        }
        if self.ide_tab_rect(1).contains(p) {
            return Some(IdeStudioClickAction::TabRuby);
        }
        if self.ide_tab_rect(2).contains(p) {
            return Some(IdeStudioClickAction::TabRml);
        }
        if self.ide_tab_rect(3).contains(p) {
            return Some(IdeStudioClickAction::TabRdx);
        }
        if self.ide_tab_rect(4).contains(p) {
            return Some(IdeStudioClickAction::TabDocs);
        }
        if self.ide_view_input_rect().contains(p) {
            return Some(IdeStudioClickAction::ViewInput);
        }
        if self.ide_view_go_rect().contains(p) {
            return Some(IdeStudioClickAction::ViewGo);
        }
        if self.ide_action_rect(0).contains(p) {
            return Some(IdeStudioClickAction::Restart);
        }
        if self.ide_action_rect(1).contains(p) {
            return Some(IdeStudioClickAction::Preview);
        }
        if self.ide_action_rect(2).contains(p) {
            return Some(IdeStudioClickAction::Link);
        }
        if self.ide_action_rect(3).contains(p) {
            return Some(IdeStudioClickAction::RubyRun);
        }
        if self.ide_action_rect(4).contains(p) {
            return Some(IdeStudioClickAction::RustCheck);
        }
        if self.ide_action_rect(5).contains(p) {
            return Some(IdeStudioClickAction::Load);
        }
        if self.ide_action_rect(6).contains(p) {
            return Some(IdeStudioClickAction::Build);
        }
        if self.ide_action_rect(7).contains(p) {
            return Some(IdeStudioClickAction::Export);
        }
        if self
            .ide_preview_button_targets
            .iter()
            .any(|(rect, _)| rect.contains(p))
            || (self.ide_preview_button_rect_valid && self.ide_preview_button_rect().contains(p))
        {
            return Some(IdeStudioClickAction::PreviewButton);
        }
        if self.ide_editor_rect().contains(p) {
            return Some(IdeStudioClickAction::EditorArea);
        }

        None
    }

    pub fn set_doom_status(&mut self, status: &str) {
        if self.kind != WindowKind::DoomLauncher {
            return;
        }
        self.doom_status = String::from(status);
        self.render();
    }

    pub fn set_linux_bridge_status(&mut self, status: &str) {
        if self.kind != WindowKind::LinuxBridge {
            return;
        }
        self.linux_bridge_status = String::from(status);
        self.render();
    }

    pub fn set_linux_bridge_frame(
        &mut self,
        source: &str,
        width: u32,
        height: u32,
        pixels: Vec<u32>,
        status: &str,
    ) {
        if self.kind != WindowKind::LinuxBridge {
            return;
        }
        self.linux_bridge_source = String::from(source);
        self.linux_bridge_width = width;
        self.linux_bridge_height = height;
        self.linux_bridge_pixels = pixels;
        self.linux_bridge_status = String::from(status);
        self.render();
    }

    pub fn doom_launch_clicked(&self, global_x: i32, global_y: i32) -> bool {
        if self.kind != WindowKind::DoomLauncher {
            return false;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return false;
        }

        self.doom_launch_button_rect()
            .contains(crate::gui::Point { x: local_x, y: local_y })
    }

    pub fn app_runner_button_rect(&self) -> Rect {
        if self.app_runner_button_rect_valid {
            return self.app_runner_button_rect_cached;
        }
        let canvas = self.app_runner_canvas_rect();
        let inner = Rect::new(
            canvas.x + 10,
            canvas.y + 10,
            canvas.width.saturating_sub(20),
            canvas.height.saturating_sub(20),
        );
        let btn_w = ((inner.width as i32 / 3).clamp(90, 220)) as u32;
        let btn_h = 26u32;
        let btn_x = inner.x + ((inner.width as i32 - btn_w as i32) / 2);
        let mut btn_y = inner.y + inner.height as i32 - btn_h as i32 - 14;
        let min_btn_y = inner.y + 34;
        if btn_y < min_btn_y {
            btn_y = min_btn_y;
        }
        Rect::new(btn_x, btn_y, btn_w, btn_h)
    }

    pub fn app_runner_button_clicked(&self, global_x: i32, global_y: i32) -> bool {
        if self.kind != WindowKind::AppRunner {
            return false;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return false;
        }

        self.app_runner_button_rect()
            .contains(crate::gui::Point { x: local_x, y: local_y })
    }

    pub fn app_runner_button_id_at(&self, global_x: i32, global_y: i32) -> Option<String> {
        if self.kind != WindowKind::AppRunner {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }
        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };

        for (rect, id) in self.app_runner_button_targets.iter() {
            if rect.contains(p) {
                if id.trim().is_empty() {
                    if self.app_runner_button_id.trim().is_empty() {
                        return Some(String::from("action"));
                    }
                    return Some(self.app_runner_button_id.clone());
                }
                return Some(id.clone());
            }
        }

        if self
            .app_runner_button_rect()
            .contains(crate::gui::Point { x: local_x, y: local_y })
        {
            if self.app_runner_button_id.trim().is_empty() {
                return Some(String::from("action"));
            }
            return Some(self.app_runner_button_id.clone());
        }

        None
    }

    pub fn explorer_item_at(&self, global_x: i32, global_y: i32) -> Option<ExplorerItem> {
        if self.kind != WindowKind::Explorer {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }

        let content_h = self.content_height();
        if local_x >= self.rect.width as i32 || local_y >= content_h {
            return None;
        }

        for (idx, item) in self.explorer_items.iter().enumerate() {
            let Some(icon_rect) = self.explorer_icon_rect(idx) else {
                continue;
            };
            if icon_rect.contains(crate::gui::Point { x: local_x, y: local_y }) {
                return Some(item.clone());
            }
        }

        None
    }

    pub fn explorer_item_global_rect(&self, index: usize) -> Option<Rect> {
        if self.kind != WindowKind::Explorer {
            return None;
        }

        let local = self.explorer_icon_rect(index)?;
        Some(Rect::new(
            self.rect.x + local.x,
            self.rect.y + TITLE_BAR_H + local.y,
            local.width,
            local.height,
        ))
    }

    pub fn explorer_canvas_contains(&self, global_x: i32, global_y: i32) -> bool {
        if self.kind != WindowKind::Explorer {
            return false;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return false;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return false;
        }

        let status_top = self.content_height() - EXPLORER_STATUS_H;
        if status_top <= EXPLORER_TOP_H {
            return false;
        }

        local_y >= EXPLORER_TOP_H && local_y < status_top
    }

    pub fn set_explorer_search_focus(&mut self, focus: bool) {
        if self.kind != WindowKind::Explorer {
            return;
        }
        if self.explorer_search_input_active == focus {
            return;
        }
        self.explorer_search_input_active = focus;
        self.render();
    }

    pub fn explorer_search_action_at(
        &self,
        global_x: i32,
        global_y: i32,
    ) -> Option<ExplorerSearchClickAction> {
        if self.kind != WindowKind::Explorer {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }

        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };
        if self.explorer_search_query_rect().contains(p) {
            return Some(ExplorerSearchClickAction::QueryField);
        }
        if self.explorer_search_button_rect().contains(p) {
            return Some(ExplorerSearchClickAction::SearchButton);
        }

        None
    }

    pub fn execute_explorer_search(&mut self) {
        if self.kind != WindowKind::Explorer {
            return;
        }

        let trimmed = self.explorer_search_query.trim().to_ascii_lowercase();
        self.explorer_scroll = 0;
        self.explorer_preview_lines.clear();

        if trimmed.is_empty() {
            self.explorer_items = self.explorer_search_source_items.clone();
            self.explorer_search_active = false;
            let total = self
                .explorer_items
                .iter()
                .filter(|item| {
                    item.kind != ExplorerItemKind::Home && item.kind != ExplorerItemKind::Up
                })
                .count();
            self.explorer_status = alloc::format!(
                "Busqueda limpia. {} elemento(s) visibles en esta ruta.",
                total
            );
            self.render();
            return;
        }

        let mut filtered: Vec<ExplorerItem> = Vec::new();
        for item in self.explorer_search_source_items.iter() {
            if item.kind == ExplorerItemKind::Home || item.kind == ExplorerItemKind::Up {
                filtered.push(item.clone());
            }
        }

        let mut matches = 0usize;
        for item in self.explorer_search_source_items.iter() {
            if item.kind == ExplorerItemKind::Home || item.kind == ExplorerItemKind::Up {
                continue;
            }
            if item.label.to_ascii_lowercase().contains(trimmed.as_str()) {
                filtered.push(item.clone());
                matches += 1;
            }
        }

        self.explorer_items = filtered;
        self.explorer_search_active = true;
        self.explorer_status = if matches == 0 {
            alloc::format!(
                "Sin resultados en esta ruta para '{}'.",
                self.explorer_search_query.trim()
            )
        } else {
            alloc::format!(
                "Busqueda local: {} resultado(s) en '{}'.",
                matches,
                self.explorer_path
            )
        };
        self.render();
    }

    pub fn load_notepad_document(
        &mut self,
        dir_cluster: u32,
        dir_path: &str,
        file_name: &str,
        text: &str,
        status: &str,
    ) {
        if self.kind != WindowKind::Notepad {
            return;
        }

        self.notepad_dir_cluster = dir_cluster;
        self.notepad_dir_path = String::from(dir_path);
        self.notepad_file_name = String::from(file_name);
        self.notepad_text = String::from(text);
        self.notepad_status = String::from(status);
        self.notepad_edit_name = false;
        self.render();
    }

    pub fn set_notepad_status(&mut self, status: &str) {
        if self.kind != WindowKind::Notepad {
            return;
        }
        self.notepad_status = String::from(status);
        self.render();
    }

    pub fn prepare_notepad_new(&mut self, default_name: &str) {
        if self.kind != WindowKind::Notepad {
            return;
        }

        self.notepad_file_name = String::from(default_name);
        self.notepad_text.clear();
        self.notepad_edit_name = true;
        self.notepad_status = String::from("New file. Type a name and edit text.");
        self.render();
    }

    pub fn set_notepad_filename_focus(&mut self, focus: bool) {
        if self.kind != WindowKind::Notepad {
            return;
        }
        self.notepad_edit_name = focus;
        self.render();
    }

    pub fn notepad_action_at(&self, global_x: i32, global_y: i32) -> Option<NotepadClickAction> {
        if self.kind != WindowKind::Notepad {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }

        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };

        let new_btn = self.notepad_button_rect(0);
        let save_btn = self.notepad_button_rect(1);
        let delete_btn = self.notepad_button_rect(2);
        let name_rect = self.notepad_filename_rect();
        let editor_rect = self.notepad_editor_rect();

        if new_btn.contains(p) {
            return Some(NotepadClickAction::New);
        }
        if save_btn.contains(p) {
            return Some(NotepadClickAction::Save);
        }
        if delete_btn.contains(p) {
            return Some(NotepadClickAction::Delete);
        }
        if name_rect.contains(p) {
            return Some(NotepadClickAction::FilenameField);
        }
        if editor_rect.contains(p) {
            return Some(NotepadClickAction::EditorArea);
        }

        None
    }

    pub fn set_search_results(
        &mut self,
        query: &str,
        status: &str,
        results: Vec<SearchResultEntry>,
    ) {
        if self.kind != WindowKind::Search {
            return;
        }
        self.search_query = String::from(query);
        self.search_status = String::from(status);
        self.search_results = results;
        self.search_input_active = true;
        self.render();
    }

    pub fn set_search_status(&mut self, status: &str) {
        if self.kind != WindowKind::Search {
            return;
        }
        self.search_status = String::from(status);
        self.render();
    }

    pub fn set_search_input_focus(&mut self, focus: bool) {
        if self.kind != WindowKind::Search {
            return;
        }
        self.search_input_active = focus;
        self.render();
    }

    pub fn search_action_at(&self, global_x: i32, global_y: i32) -> Option<SearchClickAction> {
        if self.kind != WindowKind::Search {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }
        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };

        let query = self.search_query_rect();
        if query.contains(p) {
            return Some(SearchClickAction::QueryField);
        }

        let button = self.search_button_rect();
        if button.contains(p) {
            return Some(SearchClickAction::SearchButton);
        }

        let visible_rows = self.search_visible_rows();
        let to_check = core::cmp::min(visible_rows, self.search_results.len());
        for idx in 0..to_check {
            let Some(row) = self.search_result_row_rect(idx) else {
                continue;
            };
            if row.contains(p) {
                return Some(SearchClickAction::Result(idx));
            }
        }

        None
    }

    pub fn task_manager_action_at(
        &self,
        global_x: i32,
        global_y: i32,
    ) -> Option<TaskManagerClickAction> {
        if self.kind != WindowKind::TaskManager {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }

        let content_h = self.content_height();
        if content_h <= 0 {
            return None;
        }

        let w = self.rect.width as i32;
        let content_h_i32 = content_h as i32;
        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };

        // Footer buttons (match render_task_manager layout)
        let footer_y = content_h_i32 - TASK_MGR_FOOTER_H;
        let btn_w = ((w - 30).max(140) / 2).max(120);
        let btn_h = 22;
        let left_x = 12;
        let right_x = left_x + btn_w + 10;
        let row1_y = footer_y + 8;
        let row2_y = row1_y + btn_h + 8;

        let cancel_install = Rect::new(left_x, row1_y, btn_w as u32, btn_h as u32);
        if cancel_install.contains(p) {
            return Some(TaskManagerClickAction::CancelInstall);
        }
        let cancel_fs = Rect::new(right_x, row1_y, btn_w as u32, btn_h as u32);
        if cancel_fs.contains(p) {
            return Some(TaskManagerClickAction::CancelFs);
        }
        let cancel_paste = Rect::new(left_x, row2_y, btn_w as u32, btn_h as u32);
        if cancel_paste.contains(p) {
            return Some(TaskManagerClickAction::CancelPaste);
        }
        let cancel_all = Rect::new(right_x, row2_y, btn_w as u32, btn_h as u32);
        if cancel_all.contains(p) {
            return Some(TaskManagerClickAction::CancelAll);
        }

        // List selection
        let list_y = TASK_MGR_HEADER_H + 6;
        let list_h = (content_h_i32 - TASK_MGR_HEADER_H - TASK_MGR_FOOTER_H).max(60);
        if local_y >= list_y && local_y < list_y + list_h {
            let row_offset = local_y - (list_y + 4);
            if row_offset >= 0 {
                let row = (row_offset / TASK_MGR_ROW_H) as usize;
                let visible_rows = (list_h / TASK_MGR_ROW_H).max(1) as usize;
                if row < visible_rows {
                    let idx = self.task_manager_scroll + row;
                    if idx < self.task_manager_lines.len() {
                        return Some(TaskManagerClickAction::Select(idx));
                    }
                }
            }
        }

        None
    }

    pub fn browser_link_at(&self, global_x: i32, global_y: i32) -> Option<String> {
        if self.kind != WindowKind::Browser {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }

        let view_rect = self.browser_viewport_rect();
        if !view_rect.contains(crate::gui::Point {
            x: local_x,
            y: local_y,
        }) {
            return None;
        }

        let y_in_view = local_y - view_rect.y;
        let row = ((y_in_view - 4).max(0) / 10) as usize;

        let flat = self.browser_flat_lines();
        let visible_rows = self.browser_visible_rows();
        let max_scroll = flat.len().saturating_sub(visible_rows);
        let scroll = self.browser_scroll.min(max_scroll);
        let idx = scroll.saturating_add(row);
        if idx < flat.len() {
            return Self::extract_first_url_from_text(flat[idx].as_str());
        }

        None
    }

    pub fn browser_go_clicked(&self, global_x: i32, global_y: i32) -> bool {
        if self.kind != WindowKind::Browser {
            return false;
        }
        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        self.browser_go_rect()
            .contains(crate::gui::Point { x: local_x, y: local_y })
    }

    pub fn browser_back_clicked(&self, global_x: i32, global_y: i32) -> bool {
        if self.kind != WindowKind::Browser {
            return false;
        }
        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        Rect::new(10, 10, 24, 24).contains(crate::gui::Point {
            x: local_x,
            y: local_y,
        })
    }

    pub fn browser_forward_clicked(&self, global_x: i32, global_y: i32) -> bool {
        if self.kind != WindowKind::Browser {
            return false;
        }
        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        Rect::new(40, 10, 24, 24).contains(crate::gui::Point {
            x: local_x,
            y: local_y,
        })
    }

    pub fn browser_surface_point_at(
        &self,
        global_x: i32,
        global_y: i32,
    ) -> Option<(u32, u32)> {
        if self.kind != WindowKind::Browser {
            return None;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        if local_x < 0 || local_y < 0 {
            return None;
        }
        if local_x >= self.rect.width as i32 || local_y >= self.content_height() {
            return None;
        }

        let view_rect = self.browser_viewport_rect();
        if !view_rect.contains(crate::gui::Point {
            x: local_x,
            y: local_y,
        }) {
            return None;
        }

        let surf_w = self.browser_surface_width as usize;
        let surf_h = self.browser_surface_height as usize;
        if surf_w == 0 || surf_h == 0 {
            return None;
        }
        if self.browser_surface_pixels.len() < surf_w.saturating_mul(surf_h) {
            return None;
        }

        let avail_w = (view_rect.width as i32 - 8).max(1) as usize;
        let avail_h = (view_rect.height as i32 - 8).max(1) as usize;

        let mut draw_w = avail_w;
        let mut draw_h = (surf_h.saturating_mul(draw_w)).max(1) / surf_w.max(1);
        if draw_h > avail_h {
            draw_h = avail_h;
            draw_w = (surf_w.saturating_mul(draw_h)).max(1) / surf_h.max(1);
        }
        draw_w = draw_w.max(1).min(avail_w);
        draw_h = draw_h.max(1).min(avail_h);

        let start_x = view_rect.x + ((view_rect.width as i32 - draw_w as i32) / 2);
        let start_y = view_rect.y + ((view_rect.height as i32 - draw_h as i32) / 2);

        let rel_x = local_x - start_x;
        let rel_y = local_y - start_y;
        if rel_x < 0 || rel_y < 0 || rel_x >= draw_w as i32 || rel_y >= draw_h as i32 {
            return None;
        }

        let src_x = ((rel_x as usize).saturating_mul(surf_w) / draw_w.max(1))
            .min(surf_w.saturating_sub(1)) as u32;
        let src_y = ((rel_y as usize).saturating_mul(surf_h) / draw_h.max(1))
            .min(surf_h.saturating_sub(1)) as u32;
        Some((src_x, src_y))
    }

    pub fn browser_scroll_clicked(&self, global_x: i32, global_y: i32) -> i32 {
        if self.kind != WindowKind::Browser {
            return 0;
        }

        let local_x = global_x - self.rect.x;
        let local_y = global_y - (self.rect.y + TITLE_BAR_H);
        let p = crate::gui::Point {
            x: local_x,
            y: local_y,
        };
        if self.browser_scroll_up_rect().contains(p) {
            return -1;
        }
        if self.browser_scroll_down_rect().contains(p) {
            return 1;
        }
        0
    }

    pub fn browser_scroll_by(&mut self, delta_rows: i32) -> bool {
        if self.kind != WindowKind::Browser {
            return false;
        }
        let max_scroll = self.browser_max_scroll() as i32;
        let before = self.browser_scroll as i32;
        let after = (before + delta_rows).clamp(0, max_scroll);
        if after == before {
            return false;
        }
        self.browser_scroll = after as usize;
        self.render();
        true
    }

    pub fn handle_char(&mut self, ch: char) {
        match self.kind {
            WindowKind::Terminal => {
                if ch.is_ascii() && !ch.is_control() {
                    self.input_buffer.push(ch);
                    self.render();
                }
            }
            WindowKind::Notepad => {
                if !ch.is_ascii() || ch.is_control() {
                    return;
                }

                if self.notepad_edit_name {
                    let b = ch as u8;
                    if (b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
                        && self.notepad_file_name.len() < 12
                    {
                        self.notepad_file_name.push(ch.to_ascii_uppercase());
                        self.render();
                    }
                } else {
                    self.notepad_text.push(ch);
                    self.render();
                }
            }
            WindowKind::Search => {
                if !self.search_input_active || !ch.is_ascii() || ch.is_control() {
                    return;
                }
                if self.search_query.len() < 72 {
                    self.search_query.push(ch);
                    self.render();
                }
            }
            WindowKind::Explorer => {
                if !self.explorer_search_input_active || !ch.is_ascii() || ch.is_control() {
                    return;
                }
                if self.explorer_search_query.len() < 72 {
                    self.explorer_search_query.push(ch);
                    self.render();
                }
            }
            WindowKind::Browser => {
                if ch.is_ascii() && !ch.is_control() {
                    self.browser_url.push(ch);
                    self.render();
                }
            }
            WindowKind::ImageViewer => {}
            WindowKind::AppRunner => {}
            WindowKind::IdeStudio => {
                if !ch.is_ascii() || ch.is_control() {
                    return;
                }
                if self.ide_preview_view_input_active {
                    let b = ch as u8;
                    let valid =
                        b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.';
                    if valid && self.ide_preview_view_input.len() < 48 {
                        self.ide_preview_view_input.push(ch);
                        self.render();
                    }
                    return;
                }
                let mut inserted = String::new();
                inserted.push(ch);
                if self.ide_insert_text_at_cursor_or_selection(inserted.as_str()) {
                    self.render();
                }
            }
            WindowKind::DoomLauncher => {
                let _ = self.doom_native_handle_input(Some(ch), None);
            }
            WindowKind::LinuxBridge => {}
            WindowKind::Settings => {}
            WindowKind::MediaPlayer => {}
            WindowKind::VideoPlayer => {}
            WindowKind::WifiManager => {
                if self.wifi_password_editing && ch.is_ascii() && !ch.is_control() {
                    self.wifi_password_input.push(ch);
                    self.render();
                }
            }
            WindowKind::TaskManager => {}
        }
    }

    pub fn handle_backspace(&mut self) {
        match self.kind {
            WindowKind::Terminal => {
                if !self.input_buffer.is_empty() {
                    self.input_buffer.pop();
                    self.render();
                }
            }
            WindowKind::Notepad => {
                if self.notepad_edit_name {
                    if !self.notepad_file_name.is_empty() {
                        self.notepad_file_name.pop();
                        self.render();
                    }
                } else if !self.notepad_text.is_empty() {
                    self.notepad_text.pop();
                    self.render();
                }
            }
            WindowKind::Search => {
                if self.search_input_active && !self.search_query.is_empty() {
                    self.search_query.pop();
                    self.render();
                }
            }
            WindowKind::Explorer => {
                if self.explorer_search_input_active && !self.explorer_search_query.is_empty() {
                    self.explorer_search_query.pop();
                    self.render();
                }
            }
            WindowKind::Browser => {
                if !self.browser_url.is_empty() {
                    self.browser_url.pop();
                    self.render();
                }
            }
            WindowKind::ImageViewer => {}
            WindowKind::AppRunner => {}
            WindowKind::IdeStudio => {
                if self.ide_preview_view_input_active {
                    if !self.ide_preview_view_input.is_empty() {
                        self.ide_preview_view_input.pop();
                        self.render();
                    }
                    return;
                }
                if self.ide_active_tab == 4 {
                    return;
                }
                if self.ide_delete_selection_only() {
                    self.render();
                    return;
                }
                let can_backspace = {
                    let (text, cursor_raw) = self.ide_active_text_and_cursor();
                    let cur = Self::ide_clamp_cursor_index(text, cursor_raw);
                    cur > 0
                };
                if can_backspace {
                    self.ide_push_undo_snapshot();
                    let (target, cursor, sel_start, sel_end) = self.ide_active_text_cursor_selection_mut();
                    let cur = Self::ide_clamp_cursor_index(target.as_str(), *cursor);
                    if cur == 0 {
                        return;
                    }
                    let prev = Self::ide_prev_cursor_index(target.as_str(), cur);
                    target.replace_range(prev..cur, "");
                    *cursor = prev;
                    *sel_start = prev;
                    *sel_end = prev;
                    self.render();
                }
            }
            WindowKind::DoomLauncher => {}
            WindowKind::LinuxBridge => {}
            WindowKind::Settings => {}
            WindowKind::MediaPlayer => {}
            WindowKind::VideoPlayer => {}
            WindowKind::WifiManager => {
                if self.wifi_password_editing && !self.wifi_password_input.is_empty() {
                    self.wifi_password_input.pop();
                    self.render();
                }
            }
            WindowKind::TaskManager => {}
        }
    }

    pub fn handle_enter(&mut self) -> Option<String> {
        match self.kind {
            WindowKind::Terminal => {
                if self.input_buffer.is_empty() {
                    return None;
                }

                let cmd = self.input_buffer.clone();
                let full_prompt = alloc::format!("{}> {}", self.current_path, cmd);
                self.terminal_push_line(full_prompt);
                self.terminal_scroll = 0;
                self.input_buffer.clear();
                self.render();
                Some(cmd)
            }
            WindowKind::Notepad => {
                if self.notepad_edit_name {
                    self.notepad_edit_name = false;
                    self.notepad_status = String::from("Filename set.");
                } else {
                    self.notepad_text.push('\n');
                }
                self.render();
                None
            }
            WindowKind::Search => {
                if self.search_query.trim().is_empty() {
                    None
                } else {
                    Some(self.search_query.clone())
                }
            }
            WindowKind::Explorer => None,
            WindowKind::Browser => {
                if self.browser_url.trim().is_empty() {
                    None
                } else {
                    Some(self.browser_url.clone())
                }
            }
            WindowKind::ImageViewer => None,
            WindowKind::AppRunner => None,
            WindowKind::IdeStudio => {
                if self.ide_preview_view_input_active {
                    return None;
                }
                if self.ide_active_tab == 4 {
                    return None;
                }
                if self.ide_insert_text_at_cursor_or_selection("\n") {
                    self.render();
                }
                None
            }
            WindowKind::DoomLauncher => {
                if self.doom_native_running {
                    self.stop_doom_native_session();
                } else {
                    self.start_doom_native_session();
                }
                None
            }
            WindowKind::LinuxBridge => None,
            WindowKind::Settings => None,
            WindowKind::MediaPlayer => None,
            WindowKind::VideoPlayer => None,
            WindowKind::WifiManager => None,
            WindowKind::TaskManager => None,
        }
    }

    pub fn add_output(&mut self, line: &str) {
        if self.kind != WindowKind::Terminal {
            return;
        }

        for logical in line.split('\n') {
            self.terminal_push_line(String::from(logical));
        }
        self.terminal_scroll = self.terminal_scroll.min(self.terminal_max_scroll());

        self.render();
    }

    pub fn terminal_scroll_by(&mut self, delta_rows: i32) -> bool {
        if self.kind != WindowKind::Terminal || delta_rows == 0 {
            return false;
        }

        let max_scroll = self.terminal_max_scroll();
        if max_scroll == 0 {
            self.terminal_scroll = 0;
            return false;
        }

        let magnitude = if delta_rows < 0 {
            delta_rows.saturating_neg() as usize
        } else {
            delta_rows as usize
        };
        let rows = magnitude.max(1);
        let before = self.terminal_scroll;
        if delta_rows > 0 {
            self.terminal_scroll = self.terminal_scroll.saturating_add(rows).min(max_scroll);
        } else {
            self.terminal_scroll = self.terminal_scroll.saturating_sub(rows);
        }
        if self.terminal_scroll == before {
            return false;
        }

        self.render();
        true
    }

    pub fn task_manager_scroll_by(&mut self, delta_rows: i32) -> bool {
        if self.kind != WindowKind::TaskManager || delta_rows == 0 {
            return false;
        }
        let content_h = self.content_height();
        if content_h <= 0 {
            return false;
        }
        let list_h = (content_h as i32 - TASK_MGR_HEADER_H - TASK_MGR_FOOTER_H).max(60);
        let visible_rows = (list_h / TASK_MGR_ROW_H).max(1) as usize;
        let max_scroll = self.task_manager_lines.len().saturating_sub(visible_rows);
        if max_scroll == 0 {
            self.task_manager_scroll = 0;
            return false;
        }

        let magnitude = if delta_rows < 0 {
            delta_rows.saturating_neg() as usize
        } else {
            delta_rows as usize
        };
        let rows = magnitude.max(1);
        let before = self.task_manager_scroll;
        if delta_rows > 0 {
            self.task_manager_scroll = self.task_manager_scroll.saturating_add(rows).min(max_scroll);
        } else {
            self.task_manager_scroll = self.task_manager_scroll.saturating_sub(rows);
        }
        if self.task_manager_scroll == before {
            return false;
        }
        self.render();
        true
    }

    pub fn clear_terminal_output(&mut self) {
        if self.kind != WindowKind::Terminal {
            return;
        }

        self.output_lines.clear();
        self.terminal_scroll = 0;
        self.render();
    }
}
