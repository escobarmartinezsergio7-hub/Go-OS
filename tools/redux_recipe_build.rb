#!/usr/bin/env ruby
# frozen_string_literal: true

require "digest"
require "fileutils"
require "json"
require "pathname"
require "time"

ROOT = Pathname.new(__dir__).parent

module MiniToml
  module_function

  def parse(path)
    root = {}
    current = root

    File.readlines(path, chomp: true).each_with_index do |raw_line, idx|
      line = strip_comments(raw_line).strip
      next if line.empty?

      if line.start_with?("[[") && line.end_with?("]]")
        key_path = split_path(line[2..-3].strip)
        current = enter_array_table(root, key_path, idx + 1)
        next
      end

      if line.start_with?("[") && line.end_with?("]")
        key_path = split_path(line[1..-2].strip)
        current = enter_table(root, key_path, idx + 1)
        next
      end

      key, value = parse_assignment(line, idx + 1)
      current[key] = parse_value(value, idx + 1)
    end

    root
  end

  def split_path(text)
    parts = text.split(".").map(&:strip).reject(&:empty?)
    raise "TOML path vacio" if parts.empty?
    parts
  end

  def enter_table(root, key_path, line_no)
    table = root
    key_path.each do |key|
      table[key] ||= {}
      unless table[key].is_a?(Hash)
        raise "Linea #{line_no}: '#{key}' no es tabla."
      end
      table = table[key]
    end
    table
  end

  def enter_array_table(root, key_path, line_no)
    if key_path.length == 1
      parent = root
      key = key_path[0]
    else
      parent = enter_table(root, key_path[0...-1], line_no)
      key = key_path[-1]
    end

    parent[key] ||= []
    unless parent[key].is_a?(Array)
      raise "Linea #{line_no}: '#{key}' no es arreglo de tablas."
    end
    row = {}
    parent[key] << row
    row
  end

  def parse_assignment(line, line_no)
    eq = line.index("=")
    raise "Linea #{line_no}: asignacion invalida." if eq.nil?

    key = line[0...eq].strip
    value = line[(eq + 1)..].strip
    raise "Linea #{line_no}: llave vacia." if key.empty?
    raise "Linea #{line_no}: valor vacio para '#{key}'." if value.empty?

    [key, value]
  end

  def parse_value(raw, line_no)
    if raw.start_with?("\"") && raw.end_with?("\"")
      # JSON parser handles escaped double-quoted strings.
      return JSON.parse(raw)
    end
    if raw.start_with?("'") && raw.end_with?("'")
      return raw[1..-2]
    end
    return true if raw == "true"
    return false if raw == "false"
    return raw.to_i if raw.match?(/\A-?\d+\z/)

    raise "Linea #{line_no}: valor TOML no soportado '#{raw}'."
  end

  def strip_comments(line)
    out = +""
    in_single = false
    in_double = false
    escaped = false

    line.each_char do |ch|
      if in_double
        out << ch
        if escaped
          escaped = false
        elsif ch == "\\"
          escaped = true
        elsif ch == "\""
          in_double = false
        end
        next
      end

      if in_single
        out << ch
        in_single = false if ch == "'"
        next
      end

      break if ch == "#"

      if ch == "\""
        in_double = true
      elsif ch == "'"
        in_single = true
      end
      out << ch
    end

    out
  end
end

def usage!
  warn "Usage: ruby tools/redux_recipe_build.rb <recipe.toml>"
  exit 1
end

def must_string!(obj, key, ctx)
  value = obj[key]
  if value.nil? || !value.is_a?(String) || value.strip.empty?
    raise "#{ctx}: '#{key}' es requerido y debe ser string."
  end
  value.strip
end

def optional_string(obj, key)
  value = obj[key]
  return nil if value.nil?
  return value.strip if value.is_a?(String)
  nil
end

def sanitize_rel_path!(raw, field)
  text = raw.to_s.tr("\\", "/").strip
  raise "#{field}: ruta vacia." if text.empty?
  path = Pathname.new(text)
  raise "#{field}: ruta absoluta no permitida (#{text})." if path.absolute?

  clean = []
  path.each_filename do |part|
    next if part == "."
    raise "#{field}: '..' no permitido (#{text})." if part == ".."
    clean << part
  end

  raise "#{field}: ruta vacia." if clean.empty?
  clean.join("/")
end

def collect_default_file_entries(source_dir)
  entries = []
  Dir.glob(source_dir.join("**", "*"), File::FNM_DOTMATCH).sort.each do |path_text|
    next if path_text.end_with?("/.")
    next if path_text.end_with?("/..")

    path = Pathname.new(path_text)
    next unless path.file?

    rel = path.relative_path_from(source_dir).to_s.tr("\\", "/")
    next if rel.empty?
    entries << { "src" => rel, "dst" => rel }
  end
  entries
end

def build_manifest_json(stage_dir, pkg)
  manifest_path = stage_dir.join("manifest.json")
  return if manifest_path.file?

  app = pkg.fetch("app", {})
  payload = {
    id: pkg.fetch("id"),
    name: pkg.fetch("name"),
    version: pkg.fetch("version"),
    entry: app.fetch("entry"),
    layout: app.fetch("layout")
  }
  File.write(manifest_path, JSON.pretty_generate(payload) + "\n")
end

if ARGV.size != 1
  usage!
end

recipe_path = Pathname.new(ARGV[0]).expand_path
unless recipe_path.file?
  warn "Recipe not found: #{recipe_path}"
  exit 1
end

begin
  doc = MiniToml.parse(recipe_path)
  package_cfg = doc.fetch("package", {})
  raise "recipe.toml: falta [package]." unless package_cfg.is_a?(Hash)

  app_id = must_string!(package_cfg, "id", "[package]")
  app_name = optional_string(package_cfg, "name") || app_id
  app_version = optional_string(package_cfg, "version") || "0.1.0"
  source_rel = sanitize_rel_path!(
    optional_string(package_cfg, "source") || "apps/#{app_id.tr('-', '_')}",
    "[package].source"
  )
  output_rel = sanitize_rel_path!(
    optional_string(package_cfg, "output") || "packages/#{app_id.tr('-', '_')}.rpx",
    "[package].output"
  )

  app_cfg = doc.fetch("app", {})
  app_entry = optional_string(app_cfg, "entry") || "main.rdx"
  app_layout = optional_string(app_cfg, "layout") || "main.rml"

  files_cfg = doc.fetch("files", nil)
  file_entries = if files_cfg.nil?
                   []
                 else
                   unless files_cfg.is_a?(Array)
                     raise "recipe.toml: [[files]] debe ser arreglo de tablas."
                   end
                   files_cfg
                 end

  source_dir = ROOT.join(source_rel)
  raise "No existe source: #{source_dir}" unless source_dir.directory?

  if file_entries.empty?
    file_entries = collect_default_file_entries(source_dir)
  else
    file_entries = file_entries.map.with_index(1) do |row, idx|
      unless row.is_a?(Hash)
        raise "recipe.toml: [[files]] ##{idx} invalido."
      end
      src = sanitize_rel_path!(must_string!(row, "src", "[[files]]"), "[[files]].src")
      dst = sanitize_rel_path!(
        optional_string(row, "dst") || src,
        "[[files]].dst"
      )
      { "src" => src, "dst" => dst }
    end
  end

  if file_entries.empty?
    raise "No hay archivos para empaquetar."
  end

  sign_cfg = doc.fetch("sign", {})
  sign_enabled = sign_cfg.fetch("enabled", true)
  unless sign_enabled == true || sign_enabled == false
    raise "[sign].enabled debe ser true/false."
  end
  sign_output_rel = sanitize_rel_path!(
    optional_string(sign_cfg, "output") || "#{output_rel}.sig",
    "[sign].output"
  )

  build_root = ROOT.join("build", "recipes", app_id.tr("^a-zA-Z0-9_-", "_"))
  stage_dir = build_root.join("stage")
  FileUtils.rm_rf(stage_dir)
  FileUtils.mkdir_p(stage_dir)

  file_entries.each do |entry|
    src_path = source_dir.join(entry["src"])
    raise "Archivo no existe: #{src_path}" unless src_path.file?

    dst_rel = sanitize_rel_path!(entry["dst"], "dst")
    dst_path = stage_dir.join(dst_rel)
    FileUtils.mkdir_p(dst_path.dirname)
    FileUtils.cp(src_path, dst_path)
  end

  pkg = {
    "id" => app_id,
    "name" => app_name,
    "version" => app_version,
    "app" => {
      "entry" => app_entry,
      "layout" => app_layout
    }
  }
  build_manifest_json(stage_dir, pkg)

  output_path = ROOT.join(output_rel)
  FileUtils.mkdir_p(output_path.dirname)
  system("ruby", ROOT.join("tools", "rpx_pack.rb").to_s, stage_dir.to_s, output_path.to_s, exception: true)

  if sign_enabled
    payload = File.binread(output_path)
    sha256 = Digest::SHA256.hexdigest(payload)
    signature_text = +""
    signature_text << "REDUX-SIG-V1\n"
    signature_text << "ALGO=SHA256\n"
    signature_text << "PACKAGE=#{output_path.basename}\n"
    signature_text << "APP_ID=#{app_id}\n"
    signature_text << "VERSION=#{app_version}\n"
    signature_text << "SIZE=#{payload.bytesize}\n"
    signature_text << "SHA256=#{sha256}\n"
    signature_text << "SIG=#{sha256}\n"
    signature_text << "BUILT_AT=#{Time.now.utc.iso8601}\n"

    sign_path = ROOT.join(sign_output_rel)
    FileUtils.mkdir_p(sign_path.dirname)
    File.write(sign_path, signature_text)
    puts "Signed package: #{sign_path}"
  end

  mapping = file_entries.map { |e| { src: e["src"], dst: e["dst"] } }
  build_info = {
    recipe: recipe_path.to_s,
    app_id: app_id,
    package: output_path.to_s,
    signed: sign_enabled,
    files: mapping
  }
  File.write(build_root.join("build.json"), JSON.pretty_generate(build_info) + "\n")

  puts "Recipe build OK"
  puts "  app_id: #{app_id}"
  puts "  source: #{source_dir}"
  puts "  files:  #{file_entries.length}"
  puts "  output: #{output_path}"
rescue StandardError => e
  warn "Recipe build error: #{e.message}"
  exit 1
end
