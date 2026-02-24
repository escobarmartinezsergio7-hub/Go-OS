#!/usr/bin/env ruby
# frozen_string_literal: true

require "json"
require "digest"
require "fileutils"
require "pathname"

ROOT = Pathname.new(__dir__).parent
REPO_FILE = ROOT.join("tools", "repo.json")
INSTALLED_DIR = ROOT.join("installed_apps")
PACKAGES_DIR = ROOT.join("packages")

def parse_sig_text(text)
  lines = text.lines.map(&:strip).reject(&:empty?)
  raise "Signature file is empty" if lines.empty?
  raise "Invalid signature header" unless lines.first == "REDUX-SIG-V1"

  kv = {}
  lines.drop(1).each do |line|
    next unless line.include?("=")
    key, value = line.split("=", 2)
    kv[key.strip.upcase] = value.to_s.strip
  end

  kv
end

def verify_package_signature!(package_path, sig_path)
  raw = File.binread(package_path)
  kv = parse_sig_text(File.read(sig_path))

  algo = kv.fetch("ALGO", "")
  raise "Unsupported signature algorithm: #{algo}" unless algo == "SHA256"

  pkg_name = kv.fetch("PACKAGE", "")
  unless pkg_name.empty? || pkg_name.casecmp(package_path.basename.to_s).zero?
    raise "Signature package mismatch (#{pkg_name} != #{package_path.basename})"
  end

  size_text = kv.fetch("SIZE", "")
  unless size_text.empty?
    size = Integer(size_text, 10)
    raise "Signature size mismatch (#{size} != #{raw.bytesize})" unless size == raw.bytesize
  end

  expected = kv.fetch("SHA256", "").downcase
  raise "Missing SHA256 in signature" unless expected.match?(/\A[0-9a-f]{64}\z/)

  actual = Digest::SHA256.hexdigest(raw)
  raise "SHA256 mismatch (expected #{expected}, got #{actual})" unless actual == expected

  sig = kv.fetch("SIG", "").downcase
  unless sig.empty? || sig == expected
    raise "SIG field mismatch (expected #{expected}, got #{sig})"
  end
end

cmd = ARGV.shift

if cmd.nil?
  warn "Usage: ruby tools/redux_get.rb <update|search|install|remove|list> [arg]"
  exit 1
end

unless REPO_FILE.file?
  warn "Missing repo file: #{REPO_FILE}"
  exit 1
end

repo = JSON.parse(File.read(REPO_FILE))
apps = repo.fetch("apps")

FileUtils.mkdir_p(INSTALLED_DIR)

case cmd
when "update"
  puts "Repository: #{repo.fetch('name')}"
  puts "Apps available:"
  apps.each do |name, meta|
    puts "- #{name} (#{meta['version']})"
  end
when "search"
  term = (ARGV.shift || "").downcase
  apps.each do |name, meta|
    text = "#{name} #{meta['description']}".downcase
    puts "- #{name} (#{meta['version']})" if text.include?(term)
  end
when "install"
  name = ARGV.shift
  abort "Missing app name" unless name

  meta = apps[name]
  abort "App not found: #{name}" unless meta

  package_path = ROOT.join(meta.fetch("package"))
  abort "Package missing: #{package_path}" unless package_path.file?
  signature_path = Pathname.new("#{package_path}.sig")

  if signature_path.file?
    verify_package_signature!(package_path, signature_path)
    puts "Signature OK: #{signature_path}"
  else
    puts "Signature missing: #{signature_path} (install continues without verification)"
  end

  target = INSTALLED_DIR.join(name)
  FileUtils.rm_rf(target)
  FileUtils.mkdir_p(target)

  system("ruby", ROOT.join("tools", "rpx_unpack.rb").to_s, package_path.to_s, target.to_s, exception: true)
  puts "Installed #{name} into #{target}"
when "remove"
  name = ARGV.shift
  abort "Missing app name" unless name

  target = INSTALLED_DIR.join(name)
  if target.exist?
    FileUtils.rm_rf(target)
    puts "Removed #{name}"
  else
    puts "Not installed: #{name}"
  end
when "list"
  dirs = Dir.glob(INSTALLED_DIR.join("*"))
  if dirs.empty?
    puts "No installed apps"
  else
    puts "Installed apps:"
    dirs.each { |d| puts "- #{File.basename(d)}" }
  end
else
  warn "Unknown command: #{cmd}"
  exit 1
end
