#!/usr/bin/env ruby
# frozen_string_literal: true

require "cgi"
require "fileutils"
require "json"
require "open3"
require "optparse"
require "pathname"
require "socket"
require "timeout"

ROOT = Pathname.new(__dir__).parent

DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 37_999
DEFAULT_PROJECT = "my_app"
DEFAULT_TIMEOUT_SEC = 20

DEFAULT_RUST = <<~RUST
  fn main() {
      println!("Hello from Redux IDE Rust workspace");
  }
RUST

DEFAULT_RUBY = <<~RUBY
  puts "Hello from Redux IDE Ruby workspace"
RUBY

DEFAULT_RML = <<~RML
  <App title="My Redux App" theme="dark">
    <View padding="16" background="#0F172A">
      <Header text="My Redux App" color="#22D3EE" size="24" />
      <Text id="status" value="Edit this RML and click Preview." color="#E5E7EB" />
      <Button id="action" label="Run Ruby Action" color="#0EA5E9" />
    </View>
  </App>
RML

DEFAULT_RDX = <<~RDX
  fn on_start() {
    log("Redux IDE app started");
  }

  fn on_click_action() {
    puts "Action button clicked from RDX";
  }
RDX

def sanitize_project_id(raw)
  text = raw.to_s.strip.downcase
  text = text.gsub(/[^a-z0-9_-]+/, "_")
  text = text.gsub(/\A_+/, "").gsub(/_+\z/, "")
  text.empty? ? DEFAULT_PROJECT : text
end

def titleize_project(project_id)
  project_id
    .split(/[_-]+/)
    .reject(&:empty?)
    .map { |part| part[0].upcase + part[1..].to_s }
    .join(" ")
end

class ReduxIdeServer
  attr_reader :project_id, :project_dir, :source_dir

  def initialize(root:, workspace:, project_id:)
    @root = root
    @workspace = Pathname.new(workspace).expand_path
    @project_id = sanitize_project_id(project_id)
    @project_dir = @workspace.join(@project_id)
    @source_dir = @project_dir.join("app")
    @rust_path = @source_dir.join("main.rs")
    @ruby_path = @source_dir.join("main.rb")
    @rml_path = @source_dir.join("main.rml")
    @rdx_path = @source_dir.join("main.rdx")
    @manifest_path = @source_dir.join("manifest.json")
    @recipe_path = @project_dir.join("recipe.toml")
    @target_dir = @project_dir.join("target")
    @package_path = @root.join("packages", "#{@project_id}.rpx")
    @signature_path = @root.join("packages", "#{@project_id}.rpx.sig")
    ensure_workspace!
  end

  def ensure_workspace!
    FileUtils.mkdir_p(@source_dir)
    FileUtils.mkdir_p(@target_dir)

    write_if_missing(@rust_path, DEFAULT_RUST)
    write_if_missing(@ruby_path, DEFAULT_RUBY)
    write_if_missing(@rml_path, DEFAULT_RML)
    write_if_missing(@rdx_path, DEFAULT_RDX)
    write_if_missing(@manifest_path, JSON.pretty_generate(default_manifest) + "\n")
  end

  def default_manifest
    {
      id: @project_id.tr("_", "-"),
      name: titleize_project(@project_id),
      version: "0.1.0",
      entry: "main.rdx",
      layout: "main.rml"
    }
  end

  def state_payload
    {
      ok: true,
      project: {
        id: @project_id,
        name: titleize_project(@project_id),
        workspace: @project_dir.to_s,
        source: @source_dir.to_s
      },
      files: {
        rust: safe_read(@rust_path),
        ruby: safe_read(@ruby_path),
        rml: safe_read(@rml_path),
        rdx: safe_read(@rdx_path)
      },
      package: {
        rpx: @package_path.to_s,
        sig: @signature_path.to_s
      }
    }
  end

  def save_from_payload(payload)
    files = payload.fetch("files", {})
    write_text(@rust_path, files.fetch("rust", safe_read(@rust_path)))
    write_text(@ruby_path, files.fetch("ruby", safe_read(@ruby_path)))
    write_text(@rml_path, files.fetch("rml", safe_read(@rml_path)))
    write_text(@rdx_path, files.fetch("rdx", safe_read(@rdx_path)))
  end

  def rust_check(payload)
    save_from_payload(payload)
    out_bin = @target_dir.join("rust_host_bin")
    cmd = [
      "rustc",
      "--edition=2021",
      "--crate-name",
      @project_id.gsub("-", "_"),
      @rust_path.to_s,
      "-o",
      out_bin.to_s
    ]
    result = run_command(cmd, timeout_sec: DEFAULT_TIMEOUT_SEC)
    {
      ok: result[:ok],
      action: "rust_check",
      command: result[:command],
      exit_code: result[:exit_code],
      timed_out: result[:timed_out],
      stdout: result[:stdout],
      stderr: result[:stderr],
      output_binary: out_bin.to_s
    }
  end

  def ruby_run(payload)
    save_from_payload(payload)
    cmd = ["ruby", @ruby_path.to_s]
    result = run_command(cmd, timeout_sec: DEFAULT_TIMEOUT_SEC)
    {
      ok: result[:ok],
      action: "ruby_run",
      command: result[:command],
      exit_code: result[:exit_code],
      timed_out: result[:timed_out],
      stdout: result[:stdout],
      stderr: result[:stderr]
    }
  end

  def rml_preview(payload)
    save_from_payload(payload)
    preview = build_rml_preview(safe_read(@rml_path))
    {
      ok: true,
      action: "rml_preview",
      preview_html: preview[:html],
      spec: preview[:spec]
    }
  end

  def bridge_ruby_to_rdx(payload)
    save_from_payload(payload)
    ruby_code = safe_read(@ruby_path)
    rml_code = safe_read(@rml_path)
    generated, callback = generate_rdx_from_ruby(ruby_code, rml_code)
    write_text(@rdx_path, generated)
    {
      ok: true,
      action: "bridge",
      callback: callback,
      generated_rdx: generated
    }
  end

  def package_project(payload)
    save_from_payload(payload)

    connect_ruby = payload.fetch("connect_ruby", true)
    callback_name = nil
    if connect_ruby
      generated, callback = generate_rdx_from_ruby(safe_read(@ruby_path), safe_read(@rml_path))
      callback_name = callback
      write_text(@rdx_path, generated)
    end

    write_text(@manifest_path, JSON.pretty_generate(default_manifest) + "\n")
    write_text(@recipe_path, generate_recipe_toml)

    cmd = ["ruby", @root.join("tools", "redux_recipe_build.rb").to_s, @recipe_path.to_s]
    result = run_command(cmd, timeout_sec: 60)

    {
      ok: result[:ok],
      action: "package",
      command: result[:command],
      exit_code: result[:exit_code],
      timed_out: result[:timed_out],
      stdout: result[:stdout],
      stderr: result[:stderr],
      callback: callback_name,
      package: {
        rpx: @package_path.to_s,
        sig: @signature_path.to_s,
        exists: @package_path.file?,
        sig_exists: @signature_path.file?,
        size_bytes: @package_path.file? ? @package_path.size : 0
      },
      install_hint: "Copia #{@package_path.basename} y #{@signature_path.basename} al USB, luego en ReduxOS ejecuta: install #{@package_path.basename}"
    }
  end

  def generate_recipe_toml
    source_rel = begin
      @source_dir.relative_path_from(@root).to_s.tr("\\", "/")
    rescue StandardError
      @source_dir.to_s.tr("\\", "/")
    end

    <<~TOML
      [package]
      id = "#{@project_id.tr("_", "-")}"
      name = "#{titleize_project(@project_id)}"
      version = "0.1.0"
      source = "#{source_rel}"
      output = "packages/#{@project_id}.rpx"

      [app]
      entry = "main.rdx"
      layout = "main.rml"

      [sign]
      enabled = true
      output = "packages/#{@project_id}.rpx.sig"
    TOML
  end

  def generate_rdx_from_ruby(ruby_source, rml_source)
    button_tag = extract_tag(rml_source, "Button")
    button_id = tag_attr(button_tag, "id").to_s.strip
    callback = if button_id.empty?
                 "on_click"
               else
                 sanitized = button_id.gsub(/[^a-zA-Z0-9_]/, "_")
                 "on_click_#{sanitized}"
               end

    body_lines = ruby_source.lines.map { |line| "  #{line.rstrip}" }
    body_lines = ["  log(\"ruby action vacia\");"] if body_lines.empty?
    body = body_lines.join("\n")

    rdx = +"fn on_start() {\n"
    rdx << "  log(\"#{@project_id} started\");\n"
    rdx << "}\n\n"
    rdx << "fn #{callback}() {\n"
    rdx << body
    rdx << "\n}\n"
    [rdx, callback]
  end

  def build_rml_preview(rml_text)
    app_tag = extract_tag(rml_text, "App")
    view_tag = extract_tag(rml_text, "View")
    header_tag = extract_tag(rml_text, "Header")
    text_tag = extract_tag(rml_text, "Text")
    button_tag = extract_tag(rml_text, "Button")

    title = non_empty_or(tag_attr(app_tag, "title"), "Redux App")
    theme = non_empty_or(tag_attr(app_tag, "theme"), "light").downcase
    dark = theme == "dark"

    bg = parse_color(tag_attr(view_tag, "background"), dark ? "#111827" : "#F4F8FC")
    header_color = parse_color(tag_attr(header_tag, "color"), dark ? "#22D3EE" : "#1F4D78")
    body_color = parse_color(tag_attr(text_tag, "color"), dark ? "#E5E7EB" : "#203345")
    button_color = parse_color(tag_attr(button_tag, "color"), dark ? "#0EA5E9" : "#2D89D6")

    header_text = non_empty_or(tag_attr(header_tag, "text"), title)
    body_text = non_empty_or(tag_attr(text_tag, "value") || tag_attr(text_tag, "text"), "No body text")
    button_label = non_empty_or(tag_attr(button_tag, "label"), "Run")
    button_id = tag_attr(button_tag, "id").to_s

    html = <<~HTML
      <!doctype html>
      <html>
      <head>
        <meta charset="utf-8" />
        <style>
          body {
            margin: 0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
            background: #{escape_css_color(bg)};
            color: #{escape_css_color(body_color)};
          }
          .canvas {
            min-height: 100vh;
            padding: 18px;
            box-sizing: border-box;
          }
          h1 {
            margin: 0 0 12px 0;
            color: #{escape_css_color(header_color)};
            font-size: 26px;
          }
          p {
            margin: 0 0 16px 0;
            white-space: pre-wrap;
            line-height: 1.35;
          }
          button {
            border: 0;
            border-radius: 8px;
            padding: 10px 16px;
            color: #ffffff;
            background: #{escape_css_color(button_color)};
            font-weight: 700;
            cursor: default;
          }
          .meta {
            margin-top: 12px;
            opacity: 0.78;
            font-size: 12px;
          }
        </style>
      </head>
      <body>
        <div class="canvas">
          <h1>#{CGI.escapeHTML(header_text)}</h1>
          <p>#{CGI.escapeHTML(body_text)}</p>
          <button>#{CGI.escapeHTML(button_label)}</button>
          <div class="meta">theme=#{CGI.escapeHTML(theme)} button_id=#{CGI.escapeHTML(button_id.empty? ? "<none>" : button_id)}</div>
        </div>
      </body>
      </html>
    HTML

    {
      html: html,
      spec: {
        title: title,
        theme: theme,
        button_id: button_id,
        header_text: header_text,
        body_text: body_text,
        button_label: button_label
      }
    }
  end

  def safe_read(path)
    path.file? ? File.binread(path) : ""
  end

  def write_if_missing(path, text)
    return if path.file?
    write_text(path, text)
  end

  def write_text(path, text)
    FileUtils.mkdir_p(path.dirname)
    File.binwrite(path, text.to_s)
  end

  def run_command(cmd, timeout_sec:)
    stdout = +""
    stderr = +""
    status = nil
    timed_out = false
    begin
      Timeout.timeout(timeout_sec) do
        stdout, stderr, status = Open3.capture3(*cmd, chdir: @root.to_s)
      end
    rescue Timeout::Error
      timed_out = true
      stderr = "Timeout after #{timeout_sec}s"
    rescue Errno::ENOENT => e
      stderr = e.message
    end

    {
      ok: !timed_out && status && status.success?,
      command: cmd.join(" "),
      exit_code: status&.exitstatus,
      timed_out: timed_out,
      stdout: stdout,
      stderr: stderr
    }
  end

  def extract_tag(text, tag)
    return nil if text.nil? || text.empty?
    match = text.match(/<#{Regexp.escape(tag)}\b[^>]*>/i)
    match && match[0]
  end

  def tag_attr(tag_fragment, attr_name)
    return nil if tag_fragment.nil?
    quoted = tag_fragment.match(/#{Regexp.escape(attr_name)}\s*=\s*"([^"]*)"/i)
    return quoted[1] if quoted
    single = tag_fragment.match(/#{Regexp.escape(attr_name)}\s*=\s*'([^']*)'/i)
    return single[1] if single
    nil
  end

  def parse_color(raw, fallback)
    text = raw.to_s.strip
    return fallback if text.empty?
    text = "##{text}" unless text.start_with?("#")
    return fallback unless text.match?(/\A#[0-9a-fA-F]{6}\z/)
    text
  end

  def escape_css_color(color)
    color.match?(/\A#[0-9a-fA-F]{6}\z/) ? color : "#000000"
  end

  def non_empty_or(text, fallback)
    value = text.to_s.strip
    value.empty? ? fallback : value
  end
end

def html_shell(project_id)
  <<~HTML
    <!doctype html>
    <html>
    <head>
      <meta charset="utf-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1" />
      <title>Redux IDE - #{CGI.escapeHTML(project_id)}</title>
      <style>
        :root {
          --bg: #09101f;
          --panel: #111b2e;
          --panel-2: #16233b;
          --line: #2b3e66;
          --txt: #e6eefc;
          --muted: #9ab1d8;
          --accent: #28a8ff;
          --ok: #3fb950;
          --bad: #ff6b6b;
        }
        * { box-sizing: border-box; }
        body {
          margin: 0;
          background: radial-gradient(1200px 500px at 10% -10%, #173159 0%, var(--bg) 45%);
          color: var(--txt);
          font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
          min-height: 100vh;
        }
        header {
          padding: 12px 16px;
          border-bottom: 1px solid var(--line);
          display: flex;
          flex-wrap: wrap;
          align-items: center;
          gap: 8px;
          background: rgba(5, 9, 18, 0.7);
          backdrop-filter: blur(4px);
          position: sticky;
          top: 0;
          z-index: 20;
        }
        h1 {
          margin: 0;
          font-size: 15px;
          color: #8fd0ff;
        }
        .pill {
          border: 1px solid var(--line);
          border-radius: 999px;
          font-size: 11px;
          padding: 4px 8px;
          color: var(--muted);
        }
        button {
          border: 1px solid var(--line);
          border-radius: 8px;
          background: linear-gradient(#193152, #132744);
          color: var(--txt);
          font-weight: 700;
          cursor: pointer;
          padding: 8px 10px;
        }
        button:hover { border-color: var(--accent); }
        .layout {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 12px;
          padding: 12px;
        }
        .stack {
          display: grid;
          gap: 12px;
          min-height: 0;
        }
        .card {
          background: linear-gradient(180deg, var(--panel-2), var(--panel));
          border: 1px solid var(--line);
          border-radius: 12px;
          min-height: 240px;
          overflow: hidden;
          display: grid;
          grid-template-rows: auto 1fr;
        }
        .card h2 {
          margin: 0;
          font-size: 12px;
          letter-spacing: 0.08em;
          text-transform: uppercase;
          padding: 8px 10px;
          border-bottom: 1px solid var(--line);
          color: #9dc8ff;
        }
        textarea {
          width: 100%;
          height: 100%;
          border: 0;
          outline: none;
          resize: none;
          background: #0a1424;
          color: var(--txt);
          font: 13px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
          padding: 10px;
        }
        iframe {
          width: 100%;
          height: 100%;
          border: 0;
          background: #ffffff;
        }
        pre {
          margin: 0;
          height: 100%;
          overflow: auto;
          white-space: pre-wrap;
          padding: 10px;
          background: #08101d;
          color: #cfe3ff;
          font-size: 12px;
          line-height: 1.35;
        }
        .status-ok { color: var(--ok); }
        .status-bad { color: var(--bad); }
        @media (max-width: 1100px) {
          .layout { grid-template-columns: 1fr; }
        }
      </style>
    </head>
    <body>
      <header>
        <h1>Redux IDE: Rust + Ruby + RML</h1>
        <span class="pill" id="project-pill">project=#{CGI.escapeHTML(project_id)}</span>
        <button id="btn-save">Guardar</button>
        <button id="btn-rust">Compilar Rust</button>
        <button id="btn-ruby">Ejecutar Ruby</button>
        <button id="btn-preview">Preview RML</button>
        <button id="btn-bridge">Conectar Ruby -> RDX</button>
        <button id="btn-package">Empaquetar .RPX</button>
      </header>
      <main class="layout">
        <section class="stack">
          <article class="card">
            <h2>Rust (main.rs)</h2>
            <textarea id="rust"></textarea>
          </article>
          <article class="card">
            <h2>Ruby (main.rb)</h2>
            <textarea id="ruby"></textarea>
          </article>
        </section>
        <section class="stack">
          <article class="card">
            <h2>RML (main.rml)</h2>
            <textarea id="rml"></textarea>
          </article>
          <article class="card">
            <h2>RDX (main.rdx)</h2>
            <textarea id="rdx"></textarea>
          </article>
        </section>
        <section class="stack">
          <article class="card">
            <h2>Preview RML</h2>
            <iframe id="preview"></iframe>
          </article>
          <article class="card">
            <h2>Consola</h2>
            <pre id="log"></pre>
          </article>
        </section>
      </main>
      <script>
        const rustEl = document.getElementById("rust");
        const rubyEl = document.getElementById("ruby");
        const rmlEl = document.getElementById("rml");
        const rdxEl = document.getElementById("rdx");
        const logEl = document.getElementById("log");
        const previewEl = document.getElementById("preview");
        const projectPill = document.getElementById("project-pill");

        function ts() {
          return new Date().toLocaleTimeString();
        }

        function appendLog(text, ok = null) {
          const cls = ok === null ? "" : (ok ? "status-ok" : "status-bad");
          const line = cls ? `[${ts()}] <span class="${cls}">${escapeHtml(text)}</span>` : `[${ts()}] ${escapeHtml(text)}`;
          logEl.innerHTML += line + "\\n";
          logEl.scrollTop = logEl.scrollHeight;
        }

        function escapeHtml(text) {
          return String(text)
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;");
        }

        function collectFiles() {
          return {
            rust: rustEl.value,
            ruby: rubyEl.value,
            rml: rmlEl.value,
            rdx: rdxEl.value
          };
        }

        async function api(path, payload = {}) {
          const res = await fetch(path, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload)
          });
          const data = await res.json();
          if (!res.ok) {
            throw new Error(data.error || `HTTP ${res.status}`);
          }
          return data;
        }

        async function loadState() {
          const res = await fetch("/api/state");
          const data = await res.json();
          if (!data.ok) throw new Error(data.error || "No state");
          rustEl.value = data.files.rust || "";
          rubyEl.value = data.files.ruby || "";
          rmlEl.value = data.files.rml || "";
          rdxEl.value = data.files.rdx || "";
          projectPill.textContent = `project=${data.project.id}`;
          appendLog(`Workspace listo: ${data.project.workspace}`, true);
        }

        function renderCommandResult(data) {
          if (data.command) appendLog(`$ ${data.command}`);
          if (data.stdout) appendLog(data.stdout.trimEnd(), data.ok);
          if (data.stderr) appendLog(data.stderr.trimEnd(), false);
          if (typeof data.exit_code === "number") {
            appendLog(`exit_code=${data.exit_code}`, data.ok);
          }
          if (data.timed_out) appendLog("timeout", false);
        }

        async function saveOnly() {
          const data = await api("/api/save", { files: collectFiles() });
          appendLog(data.message || "Guardado", true);
        }

        document.getElementById("btn-save").addEventListener("click", async () => {
          try {
            await saveOnly();
          } catch (err) {
            appendLog(err.message, false);
          }
        });

        document.getElementById("btn-rust").addEventListener("click", async () => {
          try {
            const data = await api("/api/rust/check", { files: collectFiles() });
            renderCommandResult(data);
            appendLog(data.ok ? "Rust compilo OK" : "Rust compilo con errores", data.ok);
          } catch (err) {
            appendLog(err.message, false);
          }
        });

        document.getElementById("btn-ruby").addEventListener("click", async () => {
          try {
            const data = await api("/api/ruby/run", { files: collectFiles() });
            renderCommandResult(data);
            appendLog(data.ok ? "Ruby ejecutado OK" : "Ruby con errores", data.ok);
          } catch (err) {
            appendLog(err.message, false);
          }
        });

        document.getElementById("btn-preview").addEventListener("click", async () => {
          try {
            const data = await api("/api/rml/preview", { files: collectFiles() });
            previewEl.srcdoc = data.preview_html || "";
            appendLog(`Preview actualizado (button_id=${data.spec.button_id || "<none>"})`, true);
          } catch (err) {
            appendLog(err.message, false);
          }
        });

        document.getElementById("btn-bridge").addEventListener("click", async () => {
          try {
            const data = await api("/api/bridge", { files: collectFiles() });
            rdxEl.value = data.generated_rdx || "";
            appendLog(`Bridge Ruby->RDX generado (${data.callback})`, true);
          } catch (err) {
            appendLog(err.message, false);
          }
        });

        document.getElementById("btn-package").addEventListener("click", async () => {
          try {
            const data = await api("/api/package", {
              files: collectFiles(),
              connect_ruby: true
            });
            renderCommandResult(data);
            if (data.callback) {
              appendLog(`Callback usado: ${data.callback}`, true);
            }
            if (data.package) {
              appendLog(`RPX: ${data.package.rpx}`, data.package.exists);
              appendLog(`SIG: ${data.package.sig}`, data.package.sig_exists);
              appendLog(`size=${data.package.size_bytes} bytes`, data.package.exists);
            }
            if (data.install_hint) appendLog(data.install_hint, true);
          } catch (err) {
            appendLog(err.message, false);
          }
        });

        loadState().then(async () => {
          try {
            const data = await api("/api/rml/preview", { files: collectFiles() });
            previewEl.srcdoc = data.preview_html || "";
          } catch (err) {
            appendLog(err.message, false);
          }
        }).catch((err) => appendLog(err.message, false));
      </script>
    </body>
    </html>
  HTML
end

def parse_json(body)
  return {} if body.strip.empty?
  JSON.parse(body)
rescue JSON::ParserError
  nil
end

class TinyHttpServer
  def initialize(host:, port:, ide:)
    @host = host
    @port = port
    @ide = ide
    @listener = nil
    @running = false
  end

  def start
    @listener = TCPServer.new(@host, @port)
    @running = true
    while @running
      begin
        socket = @listener.accept
      rescue IOError
        break
      end
      handle_client(socket)
    end
  ensure
    @listener&.close
  end

  def stop
    @running = false
    @listener&.close
  end

  private

  def handle_client(socket)
    request_line = socket.gets
    unless request_line
      socket.close
      return
    end

    method, target, _http_version = request_line.strip.split(" ", 3)
    path = target.to_s.split("?", 2).first.to_s
    headers = {}

    while (line = socket.gets)
      line = line.chomp
      break if line.empty?
      key, value = line.split(":", 2)
      next if key.nil?
      headers[key.downcase.strip] = value.to_s.strip
    end

    body = +""
    if headers.key?("content-length")
      length = headers["content-length"].to_i
      body = socket.read(length).to_s if length.positive?
    end

    status, content_type, payload = route_request(method, path, body)
    write_response(socket, status, content_type, payload)
  rescue StandardError => e
    write_response(
      socket,
      500,
      "application/json; charset=utf-8",
      JSON.generate(ok: false, error: e.message)
    )
  ensure
    socket.close unless socket.closed?
  end

  def route_request(method, path, body)
    if method == "GET" && path == "/"
      return [200, "text/html; charset=utf-8", html_shell(@ide.project_id)]
    end

    if method == "GET" && path == "/api/state"
      return [200, "application/json; charset=utf-8", JSON.generate(@ide.state_payload)]
    end

    if method == "POST"
      payload = parse_json(body)
      return [400, "application/json; charset=utf-8", JSON.generate(ok: false, error: "JSON invalido")] if payload.nil?

      case path
      when "/api/save"
        @ide.save_from_payload(payload)
        return [200, "application/json; charset=utf-8", JSON.generate(ok: true, message: "Archivos guardados.")]
      when "/api/rust/check"
        return [200, "application/json; charset=utf-8", JSON.generate(@ide.rust_check(payload))]
      when "/api/ruby/run"
        return [200, "application/json; charset=utf-8", JSON.generate(@ide.ruby_run(payload))]
      when "/api/rml/preview"
        return [200, "application/json; charset=utf-8", JSON.generate(@ide.rml_preview(payload))]
      when "/api/bridge"
        return [200, "application/json; charset=utf-8", JSON.generate(@ide.bridge_ruby_to_rdx(payload))]
      when "/api/package"
        return [200, "application/json; charset=utf-8", JSON.generate(@ide.package_project(payload))]
      end
    end

    [404, "application/json; charset=utf-8", JSON.generate(ok: false, error: "Not found")]
  end

  def write_response(socket, status, content_type, payload)
    reason = case status
             when 200 then "OK"
             when 400 then "Bad Request"
             when 404 then "Not Found"
             else "Internal Server Error"
             end

    bytes = payload.to_s.b
    socket.write("HTTP/1.1 #{status} #{reason}\r\n")
    socket.write("Content-Type: #{content_type}\r\n")
    socket.write("Content-Length: #{bytes.bytesize}\r\n")
    socket.write("Connection: close\r\n")
    socket.write("\r\n")
    socket.write(bytes)
  end
end

def run_cli(argv)
  options = {
    host: DEFAULT_HOST,
    port: DEFAULT_PORT,
    project: DEFAULT_PROJECT,
    workspace: ROOT.join("build", "ide_workspace").to_s
  }

  OptionParser.new do |opts|
    opts.banner = "Usage: ruby tools/redux_ide.rb [--host 127.0.0.1] [--port 37999] [--project my_app] [--workspace build/ide_workspace]"
    opts.on("--host HOST", String) { |v| options[:host] = v }
    opts.on("--port PORT", Integer) { |v| options[:port] = v }
    opts.on("--project NAME", String) { |v| options[:project] = v }
    opts.on("--workspace PATH", String) { |v| options[:workspace] = v }
  end.parse!(argv)

  ide = ReduxIdeServer.new(
    root: ROOT,
    workspace: options[:workspace],
    project_id: options[:project]
  )

  server = TinyHttpServer.new(host: options[:host], port: options[:port], ide: ide)
  trap("INT") { server.stop }
  trap("TERM") { server.stop }

  puts "Redux IDE listening on http://#{options[:host]}:#{options[:port]} (project=#{ide.project_id})"
  server.start
end

run_cli(ARGV) if $PROGRAM_NAME == __FILE__
