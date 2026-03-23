#include <litehtml.h>

#include <algorithm>
#include <cctype>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <new>
#include <string>
#include <vector>

extern "C" int redux_litehtml_fetch_raw(
    const unsigned char* url_ptr,
    size_t url_len,
    unsigned char* out_ptr,
    size_t out_cap,
    size_t* out_len);

namespace {

constexpr size_t kFetchBufferMax = 2 * 1024 * 1024;
constexpr size_t kPayloadBufferMax = 2 * 1024 * 1024;
constexpr size_t kMaxLines = 1024;

struct FontHandle {
    int size = 16;
    int weight = 400;
};

struct DrawOp {
    int x = 0;
    int y = 0;
    std::string text;
};

static std::string trim_copy(const std::string& in) {
    size_t start = 0;
    while (start < in.size() && std::isspace(static_cast<unsigned char>(in[start]))) {
        start++;
    }
    size_t end = in.size();
    while (end > start && std::isspace(static_cast<unsigned char>(in[end - 1]))) {
        end--;
    }
    return in.substr(start, end - start);
}

static std::string collapse_spaces(const std::string& in) {
    std::string out;
    out.reserve(in.size());
    bool prev_space = false;
    for (unsigned char c : in) {
        if (std::isspace(c)) {
            if (!prev_space) {
                out.push_back(' ');
                prev_space = true;
            }
        } else {
            out.push_back(static_cast<char>(c));
            prev_space = false;
        }
    }
    return trim_copy(out);
}

static std::string to_lower_ascii(const std::string& in) {
    std::string out = in;
    for (char& c : out) {
        c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    }
    return out;
}

static std::string sanitize_inline(const std::string& in) {
    std::string out;
    out.reserve(in.size());
    for (char c : in) {
        if (c == '\n' || c == '\r') {
            out.push_back(' ');
        } else {
            out.push_back(c);
        }
    }
    return trim_copy(out);
}

class ReduxContainer final : public litehtml::document_container {
public:
    ReduxContainer(int viewport_w, int viewport_h)
        : viewport_w_(viewport_w), viewport_h_(viewport_h) {}

    litehtml::uint_ptr create_font(const litehtml::font_description& descr,
                                   const litehtml::document* /*doc*/,
                                   litehtml::font_metrics* fm) override {
        auto* font = new (std::nothrow) FontHandle();
        if (!font) {
            if (fm) {
                *fm = litehtml::font_metrics{};
            }
            return 0;
        }

        font->size = descr.size > 0 ? static_cast<int>(descr.size) : 16;
        font->weight = descr.weight;

        if (fm) {
            fm->font_size = static_cast<litehtml::pixel_t>(font->size);
            fm->height = static_cast<litehtml::pixel_t>(font->size + 2);
            fm->ascent = static_cast<litehtml::pixel_t>((font->size * 8) / 10);
            fm->descent = static_cast<litehtml::pixel_t>(fm->height - fm->ascent);
            fm->x_height = static_cast<litehtml::pixel_t>((font->size * 5) / 10);
            fm->ch_width = static_cast<litehtml::pixel_t>((font->size * 6) / 10);
            fm->draw_spaces = true;
            fm->sub_shift = static_cast<litehtml::pixel_t>(font->size / 5);
            fm->super_shift = static_cast<litehtml::pixel_t>(font->size / 3);
        }

        return reinterpret_cast<litehtml::uint_ptr>(font);
    }

    void delete_font(litehtml::uint_ptr hFont) override {
        auto* font = reinterpret_cast<FontHandle*>(hFont);
        delete font;
    }

    litehtml::pixel_t text_width(const char* text, litehtml::uint_ptr hFont) override {
        if (!text) {
            return 0;
        }
        auto* font = reinterpret_cast<FontHandle*>(hFont);
        const int size = (font && font->size > 0) ? font->size : 16;
        const int glyph = std::max(4, (size * 6) / 10);
        return static_cast<litehtml::pixel_t>(std::strlen(text) * static_cast<size_t>(glyph));
    }

    void draw_text(litehtml::uint_ptr /*hdc*/,
                   const char* text,
                   litehtml::uint_ptr /*hFont*/,
                   litehtml::web_color /*color*/,
                   const litehtml::position& pos) override {
        if (!text || !*text) {
            return;
        }
        DrawOp op;
        op.x = static_cast<int>(pos.x);
        op.y = static_cast<int>(pos.y);
        op.text = collapse_spaces(text);
        if (!op.text.empty()) {
            draw_ops_.push_back(op);
        }
    }

    litehtml::pixel_t pt_to_px(float pt) const override {
        return static_cast<litehtml::pixel_t>(pt * 96.0f / 72.0f);
    }

    litehtml::pixel_t get_default_font_size() const override {
        return 16;
    }

    const char* get_default_font_name() const override {
        return "sans";
    }

    void draw_list_marker(litehtml::uint_ptr /*hdc*/, const litehtml::list_marker& /*marker*/) override {}
    void load_image(const char* /*src*/, const char* /*baseurl*/, bool /*redraw_on_ready*/) override {}

    void get_image_size(const char* /*src*/, const char* /*baseurl*/, litehtml::size& sz) override {
        sz.width = 0;
        sz.height = 0;
    }

    void draw_image(litehtml::uint_ptr /*hdc*/,
                    const litehtml::background_layer& /*layer*/,
                    const std::string& /*url*/,
                    const std::string& /*base_url*/) override {}

    void draw_solid_fill(litehtml::uint_ptr /*hdc*/,
                         const litehtml::background_layer& /*layer*/,
                         const litehtml::web_color& /*color*/) override {}

    void draw_linear_gradient(litehtml::uint_ptr /*hdc*/,
                              const litehtml::background_layer& /*layer*/,
                              const litehtml::background_layer::linear_gradient& /*gradient*/) override {}

    void draw_radial_gradient(litehtml::uint_ptr /*hdc*/,
                              const litehtml::background_layer& /*layer*/,
                              const litehtml::background_layer::radial_gradient& /*gradient*/) override {}

    void draw_conic_gradient(litehtml::uint_ptr /*hdc*/,
                             const litehtml::background_layer& /*layer*/,
                             const litehtml::background_layer::conic_gradient& /*gradient*/) override {}

    void draw_borders(litehtml::uint_ptr /*hdc*/,
                      const litehtml::borders& /*borders*/,
                      const litehtml::position& /*draw_pos*/,
                      bool /*root*/) override {}

    void set_caption(const char* caption) override {
        caption_ = caption ? caption : "";
    }

    void set_base_url(const char* base_url) override {
        base_url_ = base_url ? base_url : "";
    }

    void link(const std::shared_ptr<litehtml::document>& /*doc*/, const litehtml::element::ptr& /*el*/) override {}
    void on_anchor_click(const char* /*url*/, const litehtml::element::ptr& /*el*/) override {}
    void on_mouse_event(const litehtml::element::ptr& /*el*/, litehtml::mouse_event /*event*/) override {}
    void set_cursor(const char* /*cursor*/) override {}

    void transform_text(litehtml::string& text, litehtml::text_transform tt) override {
        switch (tt) {
            case litehtml::text_transform_uppercase:
                for (char& c : text) {
                    c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
                }
                break;
            case litehtml::text_transform_lowercase:
                for (char& c : text) {
                    c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
                }
                break;
            case litehtml::text_transform_capitalize: {
                bool next_upper = true;
                for (char& c : text) {
                    if (std::isspace(static_cast<unsigned char>(c))) {
                        next_upper = true;
                    } else if (next_upper) {
                        c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
                        next_upper = false;
                    }
                }
                break;
            }
            case litehtml::text_transform_none:
            default:
                break;
        }
    }

    void import_css(litehtml::string& text, const litehtml::string& /*url*/, litehtml::string& /*baseurl*/) override {
        text.clear();
    }

    void set_clip(const litehtml::position& /*pos*/, const litehtml::border_radiuses& /*bdr_radius*/) override {}
    void del_clip() override {}

    void get_viewport(litehtml::position& viewport) const override {
        viewport.x = 0;
        viewport.y = 0;
        viewport.width = static_cast<litehtml::pixel_t>(viewport_w_);
        viewport.height = static_cast<litehtml::pixel_t>(viewport_h_);
    }

    litehtml::element::ptr create_element(const char* /*tag_name*/,
                                          const litehtml::string_map& /*attributes*/,
                                          const std::shared_ptr<litehtml::document>& /*doc*/) override {
        return nullptr;
    }

    void get_media_features(litehtml::media_features& media) const override {
        media.type = litehtml::media_type_screen;
        media.width = static_cast<litehtml::pixel_t>(viewport_w_);
        media.height = static_cast<litehtml::pixel_t>(viewport_h_);
        media.device_width = media.width;
        media.device_height = media.height;
        media.color = 8;
        media.color_index = 0;
        media.monochrome = 0;
        media.resolution = 96;
    }

    void get_language(litehtml::string& language, litehtml::string& culture) const override {
        language = "en";
        culture = "US";
    }

    std::vector<std::string> take_lines() const {
        std::vector<DrawOp> ops = draw_ops_;
        std::sort(ops.begin(), ops.end(), [](const DrawOp& a, const DrawOp& b) {
            if (a.y != b.y) {
                return a.y < b.y;
            }
            return a.x < b.x;
        });

        std::vector<std::string> out;
        out.reserve(128);

        int current_y = -1;
        std::string current;
        for (const auto& op : ops) {
            const std::string piece = trim_copy(op.text);
            if (piece.empty()) {
                continue;
            }

            if (current_y < 0 || std::abs(op.y - current_y) > 8) {
                if (!current.empty()) {
                    out.push_back(collapse_spaces(current));
                    if (out.size() >= kMaxLines) {
                        break;
                    }
                }
                current = piece;
                current_y = op.y;
            } else {
                if (!current.empty()) {
                    current.push_back(' ');
                }
                current.append(piece);
            }
        }

        if (!current.empty() && out.size() < kMaxLines) {
            out.push_back(collapse_spaces(current));
        }

        return out;
    }

private:
    int viewport_w_ = 1024;
    int viewport_h_ = 720;
    std::string caption_;
    std::string base_url_;
    std::vector<DrawOp> draw_ops_;
};

static bool parse_fetch_payload(const std::string& payload,
                                std::string& status,
                                std::string& final_url,
                                std::string& body) {
    const std::string marker = "\n---\n";
    size_t split = payload.find(marker);
    if (split == std::string::npos) {
        return false;
    }

    const std::string head = payload.substr(0, split);
    body = payload.substr(split + marker.size());

    status.clear();
    final_url.clear();

    size_t start = 0;
    while (start <= head.size()) {
        size_t end = head.find('\n', start);
        std::string line = (end == std::string::npos)
                               ? head.substr(start)
                               : head.substr(start, end - start);
        if (!line.empty() && line.back() == '\r') {
            line.pop_back();
        }

        if (line.rfind("STATUS:", 0) == 0) {
            status = trim_copy(line.substr(7));
        } else if (line.rfind("FINAL_URL:", 0) == 0) {
            final_url = trim_copy(line.substr(10));
        }

        if (end == std::string::npos) {
            break;
        }
        start = end + 1;
    }

    return !body.empty();
}

static std::string extract_title(const std::string& html) {
    std::string lower = to_lower_ascii(html);
    size_t open = lower.find("<title>");
    if (open == std::string::npos) {
        return std::string();
    }
    open += 7;
    size_t close = lower.find("</title>", open);
    if (close == std::string::npos || close <= open) {
        return std::string();
    }
    return collapse_spaces(html.substr(open, close - open));
}

static std::vector<std::string> fallback_text_lines(const std::string& html) {
    std::vector<std::string> out;
    out.reserve(128);

    std::string current;
    current.reserve(160);
    bool in_tag = false;

    for (char ch : html) {
        if (ch == '<') {
            in_tag = true;
            continue;
        }
        if (ch == '>') {
            in_tag = false;
            continue;
        }
        if (in_tag) {
            continue;
        }

        unsigned char c = static_cast<unsigned char>(ch);
        if (std::isspace(c)) {
            if (!current.empty() && current.back() != ' ') {
                current.push_back(' ');
            }
        } else {
            current.push_back(static_cast<char>(c));
        }

        if (current.size() >= 96) {
            std::string line = trim_copy(current);
            if (!line.empty()) {
                out.push_back(line);
                if (out.size() >= kMaxLines) {
                    break;
                }
            }
            current.clear();
        }
    }

    if (!current.empty() && out.size() < kMaxLines) {
        std::string line = trim_copy(current);
        if (!line.empty()) {
            out.push_back(line);
        }
    }

    if (out.empty()) {
        out.push_back("LiteHTML: documento sin texto visible.");
    }

    return out;
}

static void append_line_payload(std::string& out, const std::string& line) {
    out.append("LINE: ");
    out.append(sanitize_inline(line));
    out.push_back('\n');
}

} // namespace

extern "C" int litehtml_bridge_is_ready() {
    return 1;
}

extern "C" int litehtml_bridge_render_text(
    const unsigned char* url_ptr,
    size_t url_len,
    unsigned char* out_ptr,
    size_t out_cap,
    size_t* out_len) {
    if (!url_ptr || !out_ptr || !out_len || out_cap == 0 || url_len == 0) {
        return -1;
    }

    std::string request_url(reinterpret_cast<const char*>(url_ptr), url_len);
    request_url = trim_copy(request_url);
    if (request_url.empty()) {
        return -2;
    }

    std::vector<unsigned char> fetch_buf(kFetchBufferMax + 1, 0);
    size_t fetch_len = 0;
    int fetch_rc = redux_litehtml_fetch_raw(
        url_ptr,
        url_len,
        fetch_buf.data(),
        fetch_buf.size(),
        &fetch_len);
    if (fetch_rc < 0 || fetch_len == 0 || fetch_len > fetch_buf.size()) {
        return -3;
    }

    std::string fetch_payload(reinterpret_cast<const char*>(fetch_buf.data()), fetch_len);
    std::string status = "HTTP error";
    std::string final_url = request_url;
    std::string html_body;
    if (!parse_fetch_payload(fetch_payload, status, final_url, html_body)) {
        return -4;
    }

    ReduxContainer container(1024, 720);
    std::vector<std::string> lines;
    std::string title;

    auto doc = litehtml::document::createFromString(html_body.c_str(), &container);
    if (doc) {
        (void)doc->render(1024);
        doc->draw(reinterpret_cast<litehtml::uint_ptr>(&container), 0, 0, nullptr);
        lines = container.take_lines();
    }

    title = extract_title(html_body);
    if (lines.empty()) {
        lines = fallback_text_lines(html_body);
    }

    std::string result;
    result.reserve(8192);
    result.append("STATUS: ");
    result.append(sanitize_inline(status));
    result.push_back('\n');

    if (!title.empty()) {
        result.append("TITLE: ");
        result.append(sanitize_inline(title));
        result.push_back('\n');
    }

    result.append("FINAL_URL: ");
    result.append(sanitize_inline(final_url));
    result.push_back('\n');
    result.append("---\n");

    size_t line_count = 0;
    for (const auto& line : lines) {
        append_line_payload(result, line);
        line_count++;
        if (line_count >= kMaxLines) {
            break;
        }
    }

    const size_t copy_len = std::min(result.size(), out_cap - 1);
    std::memcpy(out_ptr, result.data(), copy_len);
    out_ptr[copy_len] = 0;
    *out_len = copy_len;

    if (copy_len < result.size() || fetch_rc > 0) {
        return 1;
    }
    return 0;
}
