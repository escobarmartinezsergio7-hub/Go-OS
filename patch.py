import re

# 1. Update window.rs
with open('kernel/src/gui/window.rs', 'r') as f:
    window_rs = f.read()

window_rs = window_rs.replace(
    'self.draw_text(20, (btn_y + 8) as u32, b"DEMO_VIDEO.RPV", Color(0x9CA3AF));',
    '''let video_name = if self.notepad_file_name.is_empty() {
            "DEMO_VIDEO.RPV"
        } else {
            self.notepad_file_name.as_str()
        };
        self.draw_text(20, (btn_y + 8) as u32, video_name.as_bytes(), Color(0x9CA3AF));'''
)
with open('kernel/src/gui/window.rs', 'w') as f:
    f.write(window_rs)

# 2. Update compositor.rs
with open('kernel/src/gui/compositor.rs', 'r') as f:
    c = f.read()

func_to_add = """    fn is_video_file_name(name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        n.ends_with(".mp4") || n.ends_with(".avi") || n.ends_with(".mkv") || n.ends_with(".rpv")
    }

    fn open_video_player_file(&mut self, file_cluster: u32, file_label: &str, file_size: u32) {
        let title = alloc::format!("Video Player - {}", file_label);
        let vp_id = self.create_video_player_window(title.as_str(), 100, 100, 640, 480);
        let recent_cmd = Self::recent_file_command(
            "vid",
            self.current_volume_device_index,
            0,
            file_cluster,
            file_size,
            "/",
            file_label,
        );
        self.set_window_recent_binding(vp_id, file_label, recent_cmd.as_str());

        if let Some(win) = self.windows.iter_mut().find(|w| w.id == vp_id) {
            win.notepad_file_name = String::from(file_label);
        }
    }

"""

if "fn is_video_file_name" not in c:
    c = c.replace(
        "    fn open_media_player_file(&mut self, file_cluster: u32, file_label: &str, file_size: u32) {",
        func_to_add + "    fn open_media_player_file(&mut self, file_cluster: u32, file_label: &str, file_size: u32) {"
    )

# Now replace the `else if Self::is_audio_file_name` occurrences
repl1 = """        } else if Self::is_audio_file_name(item.label.as_str()) {
            "aud"
        } else if Self::is_video_file_name(item.label.as_str()) {
            "vid"
        } else {"""
c = c.replace(
    """        } else if Self::is_audio_file_name(item.label.as_str()) {
            "aud"
        } else {""",
    repl1
)
c = c.replace(
    """                } else if Self::is_audio_file_name(item.label.as_str()) {
                    "aud"
                } else {""",
    "        " + repl1
)

repl2 = """            } else if Self::is_audio_file_name(item.label.as_str()) {
                self.open_media_player_file(item.cluster, item.label.as_str(), item.size);
            } else if Self::is_video_file_name(item.label.as_str()) {
                self.open_video_player_file(item.cluster, item.label.as_str(), item.size);
            } else {"""
c = c.replace(
    """            } else if Self::is_audio_file_name(item.label.as_str()) {
                self.open_media_player_file(item.cluster, item.label.as_str(), item.size);
            } else {""",
    repl2
)

repl3 = """                } else if Self::is_audio_file_name(item.label.as_str()) {
                    self.open_media_player_file(item.cluster, item.label.as_str(), item.size);
                } else if Self::is_video_file_name(item.label.as_str()) {
                    self.open_video_player_file(item.cluster, item.label.as_str(), item.size);
                } else {"""
c = c.replace(
    """                } else if Self::is_audio_file_name(item.label.as_str()) {
                    self.open_media_player_file(item.cluster, item.label.as_str(), item.size);
                } else {""",
    repl3
)

repl4 = """                            let kind = if Self::is_audio_file_name(item.label.as_str()) {
                                PinnedItemKind::Audio
                            } else if Self::is_video_file_name(item.label.as_str()) {
                                PinnedItemKind::Video
                            } else if Self::is_png_file_name(item.label.as_str()) {"""
c = c.replace(
    """                            let kind = if Self::is_audio_file_name(item.label.as_str()) {
                                PinnedItemKind::Audio
                            } else if Self::is_png_file_name(item.label.as_str()) {""",
    repl4
)

# And the "Audio file — open in media player" comment one
c = c.replace(
    """                } else if Self::is_audio_file_name(item.label.as_str()) {
                    // Audio file — open in media player
                    self.open_media_player_file(item.cluster, item.label.as_str(), item.size);
                } else {""",
    """                } else if Self::is_audio_file_name(item.label.as_str()) {
                    // Audio file — open in media player
                    self.open_media_player_file(item.cluster, item.label.as_str(), item.size);
                } else if Self::is_video_file_name(item.label.as_str()) {
                    self.open_video_player_file(item.cluster, item.label.as_str(), item.size);
                } else {"""
)

with open('kernel/src/gui/compositor.rs', 'w') as f:
    f.write(c)

