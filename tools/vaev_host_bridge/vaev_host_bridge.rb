#!/usr/bin/env ruby
# frozen_string_literal: true

require "json"
require "time"
require "uri"
require "webrick"

DEFAULT_BIND = "0.0.0.0:37810"
DEFAULT_START_URL = "about:blank"
DEFAULT_VAEV_DIR = "/Users/mac/Documents/vaev"

class VaevHostBridge
  attr_reader :bind_addr, :vaev_dir, :last_url, :last_pid, :launch_error

  def initialize(bind_addr:, vaev_dir:)
    @bind_addr = bind_addr
    @vaev_dir = File.expand_path(vaev_dir)
    @started_at = Time.now.utc
    @last_url = nil
    @last_pid = nil
    @launch_error = nil
  end

  def open(url)
    clean_url = normalize_url(url)
    cmd, chdir = launch_command(clean_url)

    log_dir = File.join(Dir.pwd, "build", "vaev_host_bridge")
    Dir.mkdir(File.join(Dir.pwd, "build")) unless Dir.exist?(File.join(Dir.pwd, "build"))
    Dir.mkdir(log_dir) unless Dir.exist?(log_dir)
    log_path = File.join(log_dir, "vaev-browser.log")

    io = File.open(log_path, "a")
    io.sync = true
    pid = Process.spawn(*cmd, chdir: chdir, out: io, err: io)
    Process.detach(pid)

    @last_url = clean_url
    @last_pid = pid
    @launch_error = nil

    {
      ok: true,
      message: "vaev launch requested",
      url: clean_url,
      pid: pid,
      log_path: log_path
    }
  rescue StandardError => e
    @launch_error = e.message
    {
      ok: false,
      error: e.message,
      url: clean_url
    }
  end

  def status
    {
      ok: true,
      bridge: "vaev_host_bridge",
      ready: @launch_error.nil?,
      bind: @bind_addr,
      vaev_dir: @vaev_dir,
      started_at_utc: @started_at.iso8601,
      last_url: @last_url,
      last_pid: @last_pid,
      launch_error: @launch_error
    }
  end

  private

  def normalize_url(url)
    clean = (url || "").strip
    raise "missing url query parameter" if clean.empty?
    clean
  end

  def launch_command(url)
    raise "vaev directory not found: #{@vaev_dir}" unless Dir.exist?(@vaev_dir)

    browser_bin = ENV["VAEV_BROWSER_BIN"].to_s.strip
    unless browser_bin.empty?
      bin = File.expand_path(browser_bin)
      raise "VAEV_BROWSER_BIN is not executable: #{bin}" unless File.executable?(bin)

      return [[bin, url], @vaev_dir]
    end

    python = ENV["VAEV_PYTHON"].to_s.strip
    python = detect_python if python.empty?
    raise "python executable not found (set VAEV_PYTHON)" if python.nil?

    unless cutekit_installed?(python)
      raise "python -m cutekit not available. Install cutekit or set VAEV_BROWSER_BIN."
    end

    [
      [python, "-m", "cutekit", "run", "--release", "vaev-browser", "--", url],
      @vaev_dir
    ]
  end

  def detect_python
    return "python3" if system("python3", "--version", out: File::NULL, err: File::NULL)
    return "python" if system("python", "--version", out: File::NULL, err: File::NULL)
    nil
  end

  def cutekit_installed?(python)
    system(python, "-m", "cutekit", "--version", out: File::NULL, err: File::NULL)
  end
end

def parse_bind_addr(value)
  clean = value.to_s.strip
  m = /\A([^:]+):(\d+)\z/.match(clean)
  raise "invalid bind address '#{clean}', expected host:port" if m.nil?

  host = m[1]
  port = m[2].to_i
  raise "invalid TCP port '#{m[2]}'" if port <= 0 || port > 65_535

  [host, port]
end

bind_addr = DEFAULT_BIND
start_url = DEFAULT_START_URL
vaev_dir = ENV["VAEV_DIR"].to_s.strip
vaev_dir = DEFAULT_VAEV_DIR if vaev_dir.empty?

args = ARGV.dup
until args.empty?
  key = args.shift
  case key
  when "--bind"
    bind_addr = args.shift.to_s
  when "--url"
    start_url = args.shift.to_s
  when "--vaev-dir"
    vaev_dir = args.shift.to_s
  when "-h", "--help"
    puts "Usage: #{File.basename($PROGRAM_NAME)} [--bind host:port] [--url URL] [--vaev-dir /path/to/vaev]"
    puts "Env: VAEV_DIR, VAEV_PYTHON, VAEV_BROWSER_BIN"
    exit 0
  else
    warn "Unknown argument: #{key}"
    exit 2
  end
end

host, port = parse_bind_addr(bind_addr)
bridge = VaevHostBridge.new(bind_addr: bind_addr, vaev_dir: vaev_dir)

puts "Starting Vaev host bridge..."
puts "  bind: #{bind_addr}"
puts "  vaev: #{File.expand_path(vaev_dir)}"
puts "  url : #{start_url}"

initial = bridge.open(start_url)
unless initial[:ok]
  warn "WARN: initial launch failed: #{initial[:error]}"
end

server = WEBrick::HTTPServer.new(
  BindAddress: host,
  Port: port,
  AccessLog: [],
  Logger: WEBrick::Log.new($stderr, WEBrick::Log::WARN)
)

trap("INT") { server.shutdown }
trap("TERM") { server.shutdown }

server.mount_proc("/") do |_req, res|
  res.status = 200
  res["Content-Type"] = "application/json"
  res.body = JSON.generate(bridge.status.merge(routes: ["/status", "/open?url=", "/quit"]))
end

server.mount_proc("/status") do |_req, res|
  res.status = 200
  res["Content-Type"] = "application/json"
  res.body = JSON.generate(bridge.status)
end

server.mount_proc("/open") do |req, res|
  payload = bridge.open(req.query["url"])
  res.status = payload[:ok] ? 200 : 500
  res["Content-Type"] = "application/json"
  res.body = JSON.generate(payload)
end

unsupported = lambda do |action, res|
  res.status = 501
  res["Content-Type"] = "application/json"
  res.body = JSON.generate(
    ok: false,
    error: "#{action} not supported by vaev_host_bridge",
    hint: "Use /open?url=... for navigation."
  )
end

server.mount_proc("/eval") { |_req, res| unsupported.call("eval", res) }
server.mount_proc("/input") { |_req, res| unsupported.call("input", res) }
server.mount_proc("/frame") { |_req, res| unsupported.call("frame", res) }

server.mount_proc("/quit") do |_req, res|
  res.status = 200
  res["Content-Type"] = "application/json"
  res.body = JSON.generate(ok: true, message: "shutting down")
  Thread.new { sleep 0.05; server.shutdown }
end

server.start
