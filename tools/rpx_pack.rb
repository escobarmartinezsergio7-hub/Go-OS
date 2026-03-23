#!/usr/bin/env ruby
# frozen_string_literal: true

require "pathname"

if ARGV.size != 2
  warn "Usage: ruby tools/rpx_pack.rb <input_dir> <output.rpx>"
  exit 1
end

input_dir = Pathname.new(ARGV[0]).expand_path
output = Pathname.new(ARGV[1]).expand_path

unless input_dir.directory?
  warn "Input directory not found: #{input_dir}"
  exit 1
end

files = Dir.glob(input_dir.join("**", "*"))
           .select { |p| File.file?(p) }
           .map { |p| Pathname.new(p) }

File.binwrite(output, "")

File.open(output, "wb") do |f|
  f.write("RPX1")
  f.write([files.size].pack("L<"))

  files.each do |path|
    rel = path.relative_path_from(input_dir).to_s.tr("\\", "/")
    data = File.binread(path)
    rel_bytes = rel.b

    if rel_bytes.bytesize > 0xFFFF
      raise "Path too long: #{rel}"
    end

    f.write([rel_bytes.bytesize].pack("S<"))
    f.write([data.bytesize].pack("L<"))
    f.write(rel_bytes)
    f.write(data)
  end
end

puts "Packed #{files.size} files into #{output}"
