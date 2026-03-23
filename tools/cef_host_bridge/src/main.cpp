#include <algorithm>
#include <array>
#include <cctype>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <sstream>
#include <string>
#include <thread>
#include <vector>

#include "include/cef_app.h"
#include "include/cef_browser.h"
#include "include/cef_client.h"
#include "include/cef_command_line.h"
#include "include/cef_task.h"

#ifdef _WIN32
#include <winsock2.h>
#include <ws2tcpip.h>
using SocketHandle = SOCKET;
constexpr SocketHandle kInvalidSocket = INVALID_SOCKET;
static void CloseSocket(SocketHandle s) {
  if (s != INVALID_SOCKET) {
    closesocket(s);
  }
}
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <unistd.h>
using SocketHandle = int;
constexpr SocketHandle kInvalidSocket = -1;
static void CloseSocket(SocketHandle s) {
  if (s >= 0) {
    close(s);
  }
}
#endif

namespace {

struct Args {
  std::string bind_addr = "127.0.0.1:37820";
  std::string start_url = "https://www.google.com";
  int view_width = 1024;
  int view_height = 640;
};

struct SharedState {
  std::mutex mu;
  bool running = true;
  std::string bind_addr;
  std::string current_url;
  std::string title;
  std::string last_error;
  std::string last_ipc;
  uint64_t open_requests = 0;
  uint64_t eval_requests = 0;
  uint64_t input_requests = 0;
  bool frame_capture = false;
  uint32_t frame_width = 0;
  uint32_t frame_height = 0;
  uint64_t frame_seq = 0;
  std::vector<uint8_t> frame_bgra;
};

struct HttpRequest {
  std::string method;
  std::string path;
  std::map<std::string, std::string> query;
  std::map<std::string, std::string> headers;
  std::map<std::string, std::string> form;
  std::string body;
};

std::mutex g_browser_mu;
CefRefPtr<CefBrowser> g_browser;

void SetGlobalBrowser(CefRefPtr<CefBrowser> browser) {
  std::lock_guard<std::mutex> lock(g_browser_mu);
  g_browser = browser;
}

CefRefPtr<CefBrowser> GetGlobalBrowser() {
  std::lock_guard<std::mutex> lock(g_browser_mu);
  return g_browser;
}

std::string ToLowerAscii(std::string text) {
  for (char& c : text) {
    c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  }
  return text;
}

std::string JsonEscape(const std::string& in) {
  std::string out;
  out.reserve(in.size() + 16);
  for (char c : in) {
    switch (c) {
      case '\\':
        out += "\\\\";
        break;
      case '"':
        out += "\\\"";
        break;
      case '\n':
        out += "\\n";
        break;
      case '\r':
        out += "\\r";
        break;
      case '\t':
        out += "\\t";
        break;
      default:
        out.push_back(c);
        break;
    }
  }
  return out;
}

std::string JsSingleQuoteEscape(const std::string& in) {
  std::string out;
  out.reserve(in.size() + 16);
  for (char c : in) {
    switch (c) {
      case '\\':
        out += "\\\\";
        break;
      case '\'':
        out += "\\'";
        break;
      case '\n':
        out += "\\n";
        break;
      case '\r':
        out += "\\r";
        break;
      default:
        out.push_back(c);
        break;
    }
  }
  return out;
}

std::string UrlDecode(const std::string& text) {
  std::string out;
  out.reserve(text.size());
  for (size_t i = 0; i < text.size(); ++i) {
    if (text[i] == '+' ) {
      out.push_back(' ');
      continue;
    }
    if (text[i] == '%' && i + 2 < text.size()) {
      const auto hex = text.substr(i + 1, 2);
      char* end = nullptr;
      const long v = std::strtol(hex.c_str(), &end, 16);
      if (end != nullptr && *end == '\0') {
        out.push_back(static_cast<char>(v & 0xFF));
        i += 2;
        continue;
      }
    }
    out.push_back(text[i]);
  }
  return out;
}

std::map<std::string, std::string> ParseUrlEncoded(const std::string& text) {
  std::map<std::string, std::string> out;
  std::stringstream ss(text);
  std::string pair;
  while (std::getline(ss, pair, '&')) {
    if (pair.empty()) {
      continue;
    }
    const auto eq = pair.find('=');
    if (eq == std::string::npos) {
      out[UrlDecode(pair)] = "";
      continue;
    }
    out[UrlDecode(pair.substr(0, eq))] = UrlDecode(pair.substr(eq + 1));
  }
  return out;
}

std::optional<std::pair<std::string, uint16_t>> ParseBindAddr(const std::string& bind_addr) {
  const auto colon = bind_addr.rfind(':');
  if (colon == std::string::npos || colon == 0 || colon + 1 >= bind_addr.size()) {
    return std::nullopt;
  }

  const auto host = bind_addr.substr(0, colon);
  const auto port_str = bind_addr.substr(colon + 1);
  char* end = nullptr;
  const long port = std::strtol(port_str.c_str(), &end, 10);
  if (end == nullptr || *end != '\0' || port <= 0 || port > 65535) {
    return std::nullopt;
  }
  return std::make_pair(host, static_cast<uint16_t>(port));
}

void SetError(const std::shared_ptr<SharedState>& state, const std::string& err) {
  std::lock_guard<std::mutex> lock(state->mu);
  state->last_error = err;
}

void SetRunning(const std::shared_ptr<SharedState>& state, bool running) {
  std::lock_guard<std::mutex> lock(state->mu);
  state->running = running;
}

bool IsRunning(const std::shared_ptr<SharedState>& state) {
  std::lock_guard<std::mutex> lock(state->mu);
  return state->running;
}

void SetCurrentUrl(const std::shared_ptr<SharedState>& state, const std::string& url) {
  std::lock_guard<std::mutex> lock(state->mu);
  state->current_url = url;
}

void SetTitle(const std::shared_ptr<SharedState>& state, const std::string& title) {
  std::lock_guard<std::mutex> lock(state->mu);
  state->title = title;
}

void SetIpc(const std::shared_ptr<SharedState>& state, const std::string& ipc) {
  std::lock_guard<std::mutex> lock(state->mu);
  state->last_ipc = ipc;
}

std::string BuildStatusJson(const std::shared_ptr<SharedState>& state) {
  std::lock_guard<std::mutex> lock(state->mu);
  std::ostringstream os;
  os << "{";
  os << "\"ok\":true,";
  os << "\"backend\":\"cef\",";
  os << "\"mode\":\"host-http-bridge\",";
  os << "\"running\":" << (state->running ? "true" : "false") << ",";
  os << "\"bind_addr\":\"" << JsonEscape(state->bind_addr) << "\",";
  os << "\"current_url\":\"" << JsonEscape(state->current_url) << "\",";
  os << "\"title\":\"" << JsonEscape(state->title) << "\",";
  os << "\"open_requests\":" << state->open_requests << ",";
  os << "\"eval_requests\":" << state->eval_requests << ",";
  os << "\"input_requests\":" << state->input_requests << ",";
  os << "\"frame_capture\":" << (state->frame_capture ? "true" : "false") << ",";
  os << "\"frame_width\":" << state->frame_width << ",";
  os << "\"frame_height\":" << state->frame_height << ",";
  os << "\"frame_seq\":" << state->frame_seq << ",";
  os << "\"last_error\":\"" << JsonEscape(state->last_error) << "\",";
  os << "\"last_ipc\":\"" << JsonEscape(state->last_ipc) << "\"";
  os << "}";
  return os.str();
}

std::string StatusText(int status) {
  switch (status) {
    case 200:
      return "OK";
    case 400:
      return "Bad Request";
    case 404:
      return "Not Found";
    case 500:
      return "Internal Server Error";
    case 501:
      return "Not Implemented";
    default:
      return "OK";
  }
}

void SendHttpResponse(SocketHandle client,
                      int status,
                      const std::string& content_type,
                      const std::string& body) {
  std::ostringstream header;
  header << "HTTP/1.1 " << status << " " << StatusText(status) << "\r\n";
  header << "Content-Type: " << content_type << "\r\n";
  header << "Content-Length: " << body.size() << "\r\n";
  header << "Connection: close\r\n";
  header << "\r\n";

  const std::string bytes = header.str() + body;
  size_t sent = 0;
  while (sent < bytes.size()) {
#ifdef _WIN32
    const int n = send(client, bytes.data() + sent,
                       static_cast<int>(bytes.size() - sent), 0);
#else
    const ssize_t n = send(client, bytes.data() + sent, bytes.size() - sent, 0);
#endif
    if (n <= 0) {
      break;
    }
    sent += static_cast<size_t>(n);
  }
}

bool ReadHttpRequest(SocketHandle client, std::string* raw_out) {
  constexpr size_t kMaxReq = 1024 * 1024;
  std::string raw;
  raw.reserve(8192);
  std::array<char, 4096> buf{};

  size_t header_end = std::string::npos;
  size_t content_length = 0;

  while (raw.size() < kMaxReq) {
#ifdef _WIN32
    const int n = recv(client, buf.data(), static_cast<int>(buf.size()), 0);
#else
    const ssize_t n = recv(client, buf.data(), buf.size(), 0);
#endif
    if (n <= 0) {
      break;
    }
    raw.append(buf.data(), static_cast<size_t>(n));

    if (header_end == std::string::npos) {
      header_end = raw.find("\r\n\r\n");
      if (header_end == std::string::npos) {
        continue;
      }
      const std::string head = raw.substr(0, header_end);
      std::stringstream ss(head);
      std::string line;
      while (std::getline(ss, line)) {
        if (!line.empty() && line.back() == '\r') {
          line.pop_back();
        }
        const auto colon = line.find(':');
        if (colon == std::string::npos) {
          continue;
        }
        const std::string key = ToLowerAscii(line.substr(0, colon));
        if (key == "content-length") {
          content_length = static_cast<size_t>(
              std::strtoull(line.substr(colon + 1).c_str(), nullptr, 10));
          break;
        }
      }
    }

    if (header_end != std::string::npos &&
        raw.size() >= header_end + 4 + content_length) {
      break;
    }
  }

  if (raw.empty()) {
    return false;
  }
  *raw_out = raw;
  return true;
}

bool ParseHttpRequest(const std::string& raw, HttpRequest* out) {
  const auto header_end = raw.find("\r\n\r\n");
  if (header_end == std::string::npos) {
    return false;
  }
  const std::string head = raw.substr(0, header_end);
  out->body = raw.substr(header_end + 4);

  std::stringstream hs(head);
  std::string line;
  if (!std::getline(hs, line)) {
    return false;
  }
  if (!line.empty() && line.back() == '\r') {
    line.pop_back();
  }

  std::stringstream req_line(line);
  std::string method;
  std::string target;
  std::string version;
  if (!(req_line >> method >> target >> version)) {
    return false;
  }

  out->method = ToLowerAscii(method);
  out->path = target;

  const auto q = target.find('?');
  if (q != std::string::npos) {
    out->path = target.substr(0, q);
    out->query = ParseUrlEncoded(target.substr(q + 1));
  }

  while (std::getline(hs, line)) {
    if (!line.empty() && line.back() == '\r') {
      line.pop_back();
    }
    const auto colon = line.find(':');
    if (colon == std::string::npos) {
      continue;
    }
    const auto key = ToLowerAscii(line.substr(0, colon));
    std::string value = line.substr(colon + 1);
    value.erase(value.begin(),
                std::find_if(value.begin(), value.end(),
                             [](unsigned char ch) { return !std::isspace(ch); }));
    out->headers[key] = value;
  }

  auto content_type_it = out->headers.find("content-type");
  if (content_type_it != out->headers.end()) {
    const auto content_type = ToLowerAscii(content_type_it->second);
    if (content_type.find("application/x-www-form-urlencoded") !=
        std::string::npos) {
      out->form = ParseUrlEncoded(out->body);
    }
  }

  return true;
}

std::string GetParam(const HttpRequest& req, const std::string& key) {
  auto q = req.query.find(key);
  if (q != req.query.end()) {
    return q->second;
  }
  auto f = req.form.find(key);
  if (f != req.form.end()) {
    return f->second;
  }
  return "";
}

class OpenUrlTask final : public CefTask {
 public:
  OpenUrlTask(std::shared_ptr<SharedState> state, std::string url)
      : state_(std::move(state)), url_(std::move(url)) {}

  void Execute() override {
    auto browser = GetGlobalBrowser();
    if (!browser || !browser->GetMainFrame()) {
      SetError(state_, "open failed: browser not ready");
      return;
    }
    browser->GetMainFrame()->LoadURL(url_);
    SetCurrentUrl(state_, url_);
  }

 private:
  std::shared_ptr<SharedState> state_;
  std::string url_;

  IMPLEMENT_REFCOUNTING(OpenUrlTask);
};

class EvalTask final : public CefTask {
 public:
  EvalTask(std::shared_ptr<SharedState> state, std::string js)
      : state_(std::move(state)), js_(std::move(js)) {}

  void Execute() override {
    auto browser = GetGlobalBrowser();
    if (!browser || !browser->GetMainFrame()) {
      SetError(state_, "eval failed: browser not ready");
      return;
    }
    browser->GetMainFrame()->ExecuteJavaScript(
        js_, browser->GetMainFrame()->GetURL(), 0);
  }

 private:
  std::shared_ptr<SharedState> state_;
  std::string js_;

  IMPLEMENT_REFCOUNTING(EvalTask);
};

enum class NativeInputKind {
  MouseClick,
  MouseScroll,
};

class NativeInputTask final : public CefTask {
 public:
  NativeInputTask(std::shared_ptr<SharedState> state,
                  NativeInputKind kind,
                  int x,
                  int y,
                  int delta)
      : state_(std::move(state)),
        kind_(kind),
        x_(x),
        y_(y),
        delta_(delta) {}

  void Execute() override {
    auto browser = GetGlobalBrowser();
    if (!browser || !browser->GetHost()) {
      SetError(state_, "input failed: browser not ready");
      return;
    }

    CefMouseEvent ev;
    ev.x = x_;
    ev.y = y_;
    ev.modifiers = 0;
    auto host = browser->GetHost();

    if (kind_ == NativeInputKind::MouseClick) {
      host->SendMouseMoveEvent(ev, false);
      host->SendMouseClickEvent(ev, MBT_LEFT, false, 1);
      host->SendMouseClickEvent(ev, MBT_LEFT, true, 1);
      return;
    }

    if (kind_ == NativeInputKind::MouseScroll) {
      host->SendMouseMoveEvent(ev, false);
      host->SendMouseWheelEvent(ev, 0, delta_);
      return;
    }
  }

 private:
  std::shared_ptr<SharedState> state_;
  NativeInputKind kind_;
  int x_;
  int y_;
  int delta_;

  IMPLEMENT_REFCOUNTING(NativeInputTask);
};

class QuitTask final : public CefTask {
 public:
  explicit QuitTask(std::shared_ptr<SharedState> state) : state_(std::move(state)) {}

  void Execute() override {
    SetRunning(state_, false);
    CefQuitMessageLoop();
  }

 private:
  std::shared_ptr<SharedState> state_;

  IMPLEMENT_REFCOUNTING(QuitTask);
};

std::string BuildInputScript(const HttpRequest& req) {
  const std::string type = ToLowerAscii(GetParam(req, "type"));
  if (type == "text") {
    const std::string text = GetParam(req, "text");
    return "(()=>{const t='" + JsSingleQuoteEscape(text) +
           "';const el=document.activeElement;if(el&&('value' in el)){el.value+=t;el.dispatchEvent(new Event('input',{bubbles:true}));}else{document.body.append(t);}})();";
  }

  if (type == "key") {
    std::string key = GetParam(req, "key");
    if (key.empty()) {
      key = "Enter";
    }
    return "(()=>{const k='" + JsSingleQuoteEscape(key) +
           "';document.dispatchEvent(new KeyboardEvent('keydown',{key:k,bubbles:true}));document.dispatchEvent(new KeyboardEvent('keyup',{key:k,bubbles:true}));})();";
  }

  if (type == "back") {
    return "(()=>{history.back();})();";
  }
  if (type == "forward") {
    return "(()=>{history.forward();})();";
  }
  if (type == "reload") {
    return "(()=>{location.reload();})();";
  }

  return "";
}

int ParseIntDefault(const std::string& text, int fallback) {
  if (text.empty()) {
    return fallback;
  }
  char* end = nullptr;
  const long value = std::strtol(text.c_str(), &end, 10);
  if (end == nullptr || *end != '\0') {
    return fallback;
  }
  if (value < static_cast<long>(INT32_MIN) || value > static_cast<long>(INT32_MAX)) {
    return fallback;
  }
  return static_cast<int>(value);
}

void ServeHttp(const std::shared_ptr<SharedState>& state) {
#ifdef _WIN32
  WSADATA wsa_data;
  if (WSAStartup(MAKEWORD(2, 2), &wsa_data) != 0) {
    SetError(state, "WSAStartup failed");
    return;
  }
#endif

  const auto bind_info = ParseBindAddr(state->bind_addr);
  if (!bind_info.has_value()) {
    SetError(state, "invalid bind addr (expected host:port)");
    return;
  }

  SocketHandle listener = socket(AF_INET, SOCK_STREAM, 0);
  if (listener == kInvalidSocket) {
    SetError(state, "socket() failed");
    return;
  }

  int reuse = 1;
  setsockopt(listener, SOL_SOCKET, SO_REUSEADDR, reinterpret_cast<const char*>(&reuse),
             sizeof(reuse));

  sockaddr_in addr{};
  addr.sin_family = AF_INET;
  addr.sin_port = htons(bind_info->second);
  std::string host = bind_info->first;
  if (host == "localhost") {
    host = "127.0.0.1";
  }
  if (inet_pton(AF_INET, host.c_str(), &addr.sin_addr) != 1) {
    SetError(state, "invalid bind host (IPv4 required)");
    CloseSocket(listener);
    return;
  }

  if (::bind(listener, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) != 0) {
    SetError(state, "bind() failed");
    CloseSocket(listener);
    return;
  }
  if (listen(listener, 16) != 0) {
    SetError(state, "listen() failed");
    CloseSocket(listener);
    return;
  }

  while (IsRunning(state)) {
    fd_set read_set;
    FD_ZERO(&read_set);
    FD_SET(listener, &read_set);
    timeval tv{};
    tv.tv_sec = 0;
    tv.tv_usec = 200000;
    const int ready = select(static_cast<int>(listener + 1), &read_set, nullptr,
                             nullptr, &tv);
    if (ready <= 0) {
      continue;
    }
    if (!FD_ISSET(listener, &read_set)) {
      continue;
    }

    SocketHandle client = accept(listener, nullptr, nullptr);
    if (client == kInvalidSocket) {
      continue;
    }

    std::string raw;
    if (!ReadHttpRequest(client, &raw)) {
      SendHttpResponse(client, 400, "application/json",
                       "{\"ok\":false,\"error\":\"empty request\"}");
      CloseSocket(client);
      continue;
    }

    HttpRequest req;
    if (!ParseHttpRequest(raw, &req)) {
      SendHttpResponse(client, 400, "application/json",
                       "{\"ok\":false,\"error\":\"parse failed\"}");
      CloseSocket(client);
      continue;
    }

    if (req.path == "/status") {
      SendHttpResponse(client, 200, "application/json", BuildStatusJson(state));
      CloseSocket(client);
      continue;
    }

    if (req.path == "/open") {
      const std::string url = GetParam(req, "url");
      if (url.empty()) {
        SendHttpResponse(client, 400, "application/json",
                         "{\"ok\":false,\"error\":\"missing url\"}");
      } else {
        {
          std::lock_guard<std::mutex> lock(state->mu);
          state->open_requests++;
        }
        if (!CefPostTask(TID_UI, new OpenUrlTask(state, url))) {
          SetError(state, "open failed: CefPostTask error");
          SendHttpResponse(client, 500, "application/json",
                           "{\"ok\":false,\"error\":\"post task failed\"}");
        } else {
          SendHttpResponse(client, 200, "application/json",
                           std::string("{\"ok\":true,\"queued\":\"open\",\"url\":\"") +
                               JsonEscape(url) + "\"}");
        }
      }
      CloseSocket(client);
      continue;
    }

    if (req.path == "/eval") {
      const std::string js = GetParam(req, "js");
      if (js.empty()) {
        SendHttpResponse(client, 400, "application/json",
                         "{\"ok\":false,\"error\":\"missing js\"}");
      } else {
        {
          std::lock_guard<std::mutex> lock(state->mu);
          state->eval_requests++;
        }
        if (!CefPostTask(TID_UI, new EvalTask(state, js))) {
          SetError(state, "eval failed: CefPostTask error");
          SendHttpResponse(client, 500, "application/json",
                           "{\"ok\":false,\"error\":\"post task failed\"}");
        } else {
          SendHttpResponse(client, 200, "application/json",
                           "{\"ok\":true,\"queued\":\"eval\"}");
        }
      }
      CloseSocket(client);
      continue;
    }

    if (req.path == "/input") {
      const std::string input_type = ToLowerAscii(GetParam(req, "type"));
      if (input_type.empty()) {
        SendHttpResponse(client, 400, "application/json",
                         "{\"ok\":false,\"error\":\"input type invalid\"}");
        CloseSocket(client);
        continue;
      }

      {
        std::lock_guard<std::mutex> lock(state->mu);
        state->input_requests++;
      }

      if (input_type == "click") {
        const int x = ParseIntDefault(GetParam(req, "x"), -1);
        const int y = ParseIntDefault(GetParam(req, "y"), -1);
        if (x < 0 || y < 0) {
          SendHttpResponse(client, 400, "application/json",
                           "{\"ok\":false,\"error\":\"click requires x and y\"}");
        } else if (!CefPostTask(
                       TID_UI,
                       new NativeInputTask(state, NativeInputKind::MouseClick, x, y, 0))) {
          SetError(state, "input click failed: CefPostTask error");
          SendHttpResponse(client, 500, "application/json",
                           "{\"ok\":false,\"error\":\"post task failed\"}");
        } else {
          SendHttpResponse(client, 200, "application/json",
                           "{\"ok\":true,\"queued\":\"click\"}");
        }
      } else if (input_type == "scroll") {
        int x = ParseIntDefault(GetParam(req, "x"), -1);
        int y = ParseIntDefault(GetParam(req, "y"), -1);
        const int delta = ParseIntDefault(GetParam(req, "delta"), 120);
        {
          std::lock_guard<std::mutex> lock(state->mu);
          if (x < 0) {
            x = static_cast<int>(state->frame_width / 2);
          }
          if (y < 0) {
            y = static_cast<int>(state->frame_height / 2);
          }
        }

        if (!CefPostTask(TID_UI, new NativeInputTask(
                                     state, NativeInputKind::MouseScroll, x, y, delta))) {
          SetError(state, "input scroll failed: CefPostTask error");
          SendHttpResponse(client, 500, "application/json",
                           "{\"ok\":false,\"error\":\"post task failed\"}");
        } else {
          SendHttpResponse(client, 200, "application/json",
                           "{\"ok\":true,\"queued\":\"scroll\"}");
        }
      } else {
        std::string js = BuildInputScript(req);
        if (js.empty()) {
          SendHttpResponse(client, 400, "application/json",
                           "{\"ok\":false,\"error\":\"input type invalid\"}");
        } else if (!CefPostTask(TID_UI, new EvalTask(state, js))) {
          SetError(state, "input failed: CefPostTask error");
          SendHttpResponse(client, 500, "application/json",
                           "{\"ok\":false,\"error\":\"post task failed\"}");
        } else {
          SendHttpResponse(client, 200, "application/json",
                           "{\"ok\":true,\"queued\":\"input\"}");
        }
      }
      CloseSocket(client);
      continue;
    }

    if (req.path == "/frame") {
      uint32_t width = 0;
      uint32_t height = 0;
      std::vector<uint8_t> bgra;
      {
        std::lock_guard<std::mutex> lock(state->mu);
        width = state->frame_width;
        height = state->frame_height;
        bgra = state->frame_bgra;
      }

      const size_t expected = static_cast<size_t>(width) * static_cast<size_t>(height) * 4u;
      if (width == 0 || height == 0 || bgra.size() < expected) {
        SendHttpResponse(
            client, 503, "application/json",
            "{\"ok\":false,\"error\":\"no frame yet (wait for OnPaint)\"}");
        CloseSocket(client);
        continue;
      }

      std::ostringstream ppm_header;
      ppm_header << "P6\n" << width << " " << height << "\n255\n";
      std::string body = ppm_header.str();
      body.reserve(body.size() + static_cast<size_t>(width) * static_cast<size_t>(height) * 3u);

      for (size_t i = 0; i < expected; i += 4) {
        const char r = static_cast<char>(bgra[i + 2]);
        const char g = static_cast<char>(bgra[i + 1]);
        const char b = static_cast<char>(bgra[i + 0]);
        body.push_back(r);
        body.push_back(g);
        body.push_back(b);
      }

      SendHttpResponse(client, 200, "image/x-portable-pixmap", body);
      CloseSocket(client);
      continue;
    }

    if (req.path == "/quit") {
      CefPostTask(TID_UI, new QuitTask(state));
      SendHttpResponse(client, 200, "application/json",
                       "{\"ok\":true,\"queued\":\"quit\"}");
      CloseSocket(client);
      continue;
    }

    SendHttpResponse(
        client, 404, "text/plain",
        "routes: /status /open?url= /eval?js= /input?type=... /frame /quit\n");
    CloseSocket(client);
  }

  CloseSocket(listener);
#ifdef _WIN32
  WSACleanup();
#endif
}

Args ParseArgs(int argc, char** argv) {
  Args args;
  for (int i = 1; i < argc; ++i) {
    const std::string cur = argv[i] == nullptr ? "" : argv[i];
    if (cur == "--bind" && i + 1 < argc) {
      args.bind_addr = argv[++i];
      continue;
    }
    if (cur == "--url" && i + 1 < argc) {
      args.start_url = argv[++i];
      continue;
    }
    if (cur == "--width" && i + 1 < argc) {
      args.view_width = ParseIntDefault(argv[++i], args.view_width);
      continue;
    }
    if (cur == "--height" && i + 1 < argc) {
      args.view_height = ParseIntDefault(argv[++i], args.view_height);
      continue;
    }
    if (!cur.empty() && cur.rfind("--", 0) != 0) {
      args.start_url = cur;
    }
  }
  args.view_width = std::max(320, args.view_width);
  args.view_height = std::max(200, args.view_height);
  return args;
}

class ReduxBrowserClient final : public CefClient,
                                 public CefDisplayHandler,
                                 public CefLifeSpanHandler,
                                 public CefRenderHandler {
 public:
  explicit ReduxBrowserClient(std::shared_ptr<SharedState> state,
                              int view_width,
                              int view_height)
      : state_(std::move(state)),
        view_width_(std::max(320, view_width)),
        view_height_(std::max(200, view_height)) {}

  CefRefPtr<CefDisplayHandler> GetDisplayHandler() override { return this; }
  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
  CefRefPtr<CefRenderHandler> GetRenderHandler() override { return this; }

  bool GetViewRect(CefRefPtr<CefBrowser> /*browser*/, CefRect& rect) override {
    rect = CefRect(0, 0, view_width_, view_height_);
    return true;
  }

  void OnPaint(CefRefPtr<CefBrowser> /*browser*/,
               PaintElementType type,
               const RectList& /*dirty_rects*/,
               const void* buffer,
               int width,
               int height) override {
    if (type != PET_VIEW || buffer == nullptr || width <= 0 || height <= 0) {
      return;
    }
    const size_t bytes =
        static_cast<size_t>(width) * static_cast<size_t>(height) * 4u;
    const uint8_t* src = reinterpret_cast<const uint8_t*>(buffer);

    std::lock_guard<std::mutex> lock(state_->mu);
    state_->frame_capture = true;
    state_->frame_width = static_cast<uint32_t>(width);
    state_->frame_height = static_cast<uint32_t>(height);
    state_->frame_seq++;
    state_->frame_bgra.assign(src, src + bytes);
  }

  void OnTitleChange(CefRefPtr<CefBrowser> browser,
                     const CefString& title) override {
    if (browser && browser->GetHost()) {
      browser->GetHost()->SetWindowTitle(title);
    }
    SetTitle(state_, title.ToString());
  }

  void OnAfterCreated(CefRefPtr<CefBrowser> browser) override {
    SetGlobalBrowser(browser);
    if (browser && browser->GetHost()) {
      browser->GetHost()->WasResized();
    }
  }

  bool DoClose(CefRefPtr<CefBrowser> /*browser*/) override { return false; }

  void OnBeforeClose(CefRefPtr<CefBrowser> /*browser*/) override {
    SetGlobalBrowser(nullptr);
    SetRunning(state_, false);
    CefQuitMessageLoop();
  }

 private:
  std::shared_ptr<SharedState> state_;
  int view_width_;
  int view_height_;
  IMPLEMENT_REFCOUNTING(ReduxBrowserClient);
};

class ReduxCefApp final : public CefApp, public CefBrowserProcessHandler {
 public:
  ReduxCefApp() = default;
  CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override {
    return this;
  }

 private:
  IMPLEMENT_REFCOUNTING(ReduxCefApp);
};

}  // namespace

int main(int argc, char** argv) {
  const Args args = ParseArgs(argc, argv);

#if defined(_WIN32)
  CefMainArgs main_args(GetModuleHandle(nullptr));
#else
  CefMainArgs main_args(argc, argv);
#endif

  CefRefPtr<ReduxCefApp> app(new ReduxCefApp());
  const int exit_code = CefExecuteProcess(main_args, app, nullptr);
  if (exit_code >= 0) {
    return exit_code;
  }

  CefSettings settings;
  settings.no_sandbox = true;
  settings.windowless_rendering_enabled = true;

  if (!CefInitialize(main_args, settings, app, nullptr)) {
    std::cerr << "CEF init failed." << std::endl;
    return 1;
  }

  auto state = std::make_shared<SharedState>();
  state->bind_addr = args.bind_addr;
  state->current_url = args.start_url;
  state->title = "ReduxOS CEF Host Bridge (OSR)";
  state->frame_width = static_cast<uint32_t>(args.view_width);
  state->frame_height = static_cast<uint32_t>(args.view_height);

  CefWindowInfo window_info;
  window_info.SetAsWindowless(0, false);

  CefBrowserSettings browser_settings;
  browser_settings.windowless_frame_rate = 30;
  CefRefPtr<ReduxBrowserClient> client(
      new ReduxBrowserClient(state, args.view_width, args.view_height));

  const bool created = CefBrowserHost::CreateBrowser(
      window_info, client, args.start_url, browser_settings, nullptr, nullptr);
  if (!created) {
    std::cerr << "CEF CreateBrowser failed." << std::endl;
    CefShutdown();
    return 2;
  }

  std::thread http_thread([state]() { ServeHttp(state); });

  std::cout << "CEF bridge running\n";
  std::cout << "  bind: " << args.bind_addr << "\n";
  std::cout << "  url : " << args.start_url << "\n";
  std::cout << "  view: " << args.view_width << "x" << args.view_height << "\n";
  std::cout << "  api : /status /open /eval /input /frame /quit\n";

  CefRunMessageLoop();

  SetRunning(state, false);
  if (http_thread.joinable()) {
    http_thread.join();
  }

  CefShutdown();
  return 0;
}
