#!/usr/bin/env ruby
# frozen_string_literal: true

require "fileutils"
require "pathname"

if ARGV.size != 2
  warn "Usage: ruby tools/rpx_unpack.rb <package.rpx> <output_dir>"
  exit 1
end

package = Pathname.new(ARGV[0]).expand_path
out_dir = Pathname.new(ARGV[1]).expand_path

unless package.file?
  warn "Package not found: #{package}"
  exit 1
end

FileUtils.mkdir_p(out_dir)

File.open(package, "rb") do |f|
  magic = f.read(4)
  raise "Invalid RPX magic" unless magic == "RPX1"

  count = f.read(4)&.unpack1("L<")
  raise "Corrupt package" unless count

  count.times do
    path_len = f.read(2)&.unpack1("S<")
    size = f.read(4)&.unpack1("L<")
    raise "Corrupt entry header" unless path_len && size

    rel = f.read(path_len)
    data = f.read(size)
    raise "Corrupt entry payload" unless rel && data

    target = out_dir.join(rel)
    FileUtils.mkdir_p(target.dirname)
    File.binwrite(target, data)
  end

  puts "Unpacked #{count} files into #{out_dir}"
end
